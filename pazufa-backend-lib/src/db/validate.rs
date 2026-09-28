use crate::models;
use crate::{PazufaServer, Result, error::DataValidationError};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use track_validation::Regex;

#[derive(Deserialize)]
struct RawTracks {
    version: String,
    stations: BTreeMap<char, models::Stationstyp>,
    tracks: BTreeMap<models::Parlament, BTreeMap<models::Vorgangstyp, String>>,
}

pub struct Tracks {
    pub version: String,
    pub station_mapping: BTreeMap<models::Stationstyp, char>,
    pub trackmap: BTreeMap<(models::Parlament, models::Vorgangstyp), Regex>,
}
impl Tracks {
    /// Creates a new Track struct from a supplied file at runtime
    /// # Panics
    /// May panic if the file could not be found OR the parsing fails,
    /// but since that happens at program startup, it is not really an issue
    #[must_use]
    #[allow(clippy::expect_used, reason = "Only called at startup time")]
    pub fn new(path: &str) -> Self {
        let file = std::fs::read_to_string(std::path::Path::new(path)).unwrap_or_else(|error| {
            panic!("Expected to find a tracks file configured as: `{path}`. Error: {error}")
        });
        let rt: RawTracks =
            toml::from_str(&file).unwrap_or_else(|e| panic!("Trackfile could not be loaded: {e}"));
        let trackmap: BTreeMap<_, Regex> = rt
            .tracks
            .iter()
            .flat_map(|(parl, v)| {
                v.iter().map(|(&vtyp, rx)| {
                    ((*parl, vtyp), {
                        tracing::trace!("parsing {rx}");
                        Regex::from_str(rx).expect("Malformed tracks file")
                    })
                })
            })
            .collect();
        let sm: BTreeMap<_, char> = rt.stations.iter().map(|(k, v)| (*v, *k)).collect();
        Self {
            version: rt.version,
            station_mapping: sm,
            trackmap,
        }
    }
}

/// Validates a Vorgang, meaning it checks whether it adheres to the constraints specified
/// in the tracks file.
/// # Errors
/// Can error due to database errors or due to a violation of the trackfile specifications
pub async fn validate_vorgang(vg: &models::Vorgang, srv: &PazufaServer) -> Result<()> {
    let mut stations = vg.stationen.clone();
    stations.sort_by_key(|s| s.zp_start);
    let parls: BTreeSet<models::Parlament> = stations.iter().map(|s| s.gremium.parlament).collect();
    let s_parls: Vec<_> = parls.iter().map(ToString::to_string).collect();

    let num_classes: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(DISTINCT(c_id)) as "cnt!"
        FROM parl_class pc, parlament p
        WHERE 
        p_id = p.id AND
        EXISTS (SELECT 1 FROM UNNEST($1::text[]) as in_parls(ip) WHERE ip = p.value)"#,
        &s_parls[..]
    )
    .fetch_one(&srv.sqlx_db)
    .await?;
    if num_classes != 1 {
        let reason = format!(
            "The Vorgang contains Stations from more than one legislative class (parliament) in it's body: {s_parls:?}, which invalidates it"
        );
        tracing::warn!("{reason}");
        return Err(DataValidationError::MalformedVorgang { reason }.into());
    }
    if stations
        .iter()
        .any(|s| !srv.tracks.station_mapping.contains_key(&s.typ))
    {
        return Err(crate::error::DataValidationError::MalformedVorgang {
            reason: "Vorgang contains unmapped station type (probably 'sonstig')".to_owned(),
        }
        .into());
    }
    let mut validation_errors = Vec::new();
    for p in parls {
        tracing::trace!("now in parliament {p}");
        let regex = srv.tracks.trackmap.get(&(p, vg.typ)).ok_or(
            DataValidationError::MalformedVorgang { reason: format!("A vorgang of type {} is not specified for parliament {p}. If you believe this to be an error, open an issue: https://codeberg.org/PaZuFa/pazufa-backend/issues", vg.typ) }
            )?;
        tracing::trace!("Tracks retrieved");
        let p_subvec: Vec<models::Station> = stations
            .iter()
            .filter(|x| x.gremium.parlament == p)
            .cloned()
            .collect();
        tracing::trace!("Subvec constructed");
        let p_substr: Vec<char> = p_subvec
            .iter()
            .map(|s| srv.tracks.station_mapping[&s.typ])
            .collect();
        tracing::trace!("Substr constructed: {:?}", p_substr);

        // matching and checking
        let (matched, consumed) = regex.match_regex(&p_substr);
        tracing::trace!("{:?} matched like so: {:?}", p_substr, (matched, consumed));
        if !matched {
            let conflicts: Vec<_> =
                regex
                    .conflicting_stations(&p_subvec, &srv.tracks.station_mapping, |s, m| m[&s.typ]);
            let conflicts_str: Vec<_> = conflicts
                .iter()
                .map(|s| format!("({} / {})", s.zp_start, s.typ))
                .collect();
            let p_subvec: Vec<_> = p_subvec
                .iter()
                .map(|s| format!("({} / {})", s.zp_start, s.typ))
                .collect();

            let reason = format!(
                "The Vorgang with this station ordering: {:?} has at least one station that is not adhering to the track for parliament {p} and vgtyp {}: {:?}",
                p_subvec, vg.typ, conflicts_str
            );
            tracing::warn!("{reason}");
            validation_errors.push(reason);
        } else if matched && consumed < p_substr.len() {
            #[allow(
                clippy::arithmetic_side_effects,
                reason = "consumed is bounded to be less than length"
            )]
            let reason = format!(
                "Some stations have been matched and are in valid order, but there are stations left over that cannot be assigned to a track: {:?}",
                &p_substr[p_substr.len() - consumed + 1..]
            );
            tracing::warn!("{reason}");
            validation_errors.push(reason);
        }
    }
    if validation_errors.is_empty() {
        Ok(())
    } else {
        let fmt = validation_errors.iter().fold(
            "Track validation Failed. Reasons:\n".to_owned(),
            |a, new| format!("{a}\n{new}"),
        );
        Err(DataValidationError::MalformedVorgang { reason: fmt }.into())
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test_regex {
    use super::*;
    use crate::utils::testing::{TestSetup, generate};
    use tracing_test::traced_test;

    #[tokio::test]
    async fn sonstig_station_run() {
        let scenario = TestSetup::new("sonstig_station_run").await;

        let vg = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::Sonstig,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let res = validate_vorgang(&vg, &scenario.server).await;
        assert!(matches!(
            res,
            Err(crate::error::PazufaError::Validation { .. })
        ));
        scenario.teardown().await;
    }

    #[traced_test]
    #[tokio::test]
    async fn vorgang_validate_starmatching_consumption() {
        let scenario = TestSetup::new("vorgang_validate_memleak_starmatching").await;
        let gremium = models::Gremium {
            parlament: models::Parlament::Bt,
            ..generate::default_gremium()
        };
        let stations = [
            ("2021-11-14T12:57:08Z", models::Stationstyp::ParlVollvlsgn),
            ("2022-01-05T04:56:12Z", models::Stationstyp::ParlGgentwurf),
            ("2022-10-24T13:02:26Z", models::Stationstyp::ParlAusschber),
            ("2023-03-14T08:47:49Z", models::Stationstyp::ParlAblehnung),
            ("2024-06-21T10:58:01Z", models::Stationstyp::ParlAblehnung),
        ];
        let mut i = 0;
        let catastrophe = models::Vorgang {
            stationen: stations
                .iter()
                .map(|(t, typ)| {
                    i += 1;
                    models::Station {
                        zp_start: chrono::DateTime::parse_from_rfc3339(t.to_owned())
                            .expect("Hardcoded Date parsing failed")
                            .into(),
                        gremium: gremium.clone(),
                        typ: *typ,
                        ..generate::random::station(i)
                    }
                })
                .collect(),
            ..generate::random::vorgang(1)
        };
        let res = validate_vorgang(&catastrophe, &scenario.server).await;
        assert!(matches!(
            res,
            Err(crate::error::PazufaError::Validation { .. })
        ));
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn validate_vorgang_full_fail() {
        let scenario = TestSetup::new("validate_vorgang_fine").await;
        let vorgang = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let result = validate_vorgang(&vorgang, &scenario.server).await;
        assert!(result.is_err(), "{result:?}");
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn validate_vorgang_formalia_wrong() {
        let scenario = TestSetup::new("validate_vorgang_formalia").await;
        let vorgang = generate::default_vorgang();
        let vorgang = models::Vorgang {
            typ: models::Vorgangstyp::GgLandVolk,
            ..vorgang
        };
        let result = validate_vorgang(&vorgang, &scenario.server).await;
        assert!(
            match result {
                Err(crate::error::PazufaError::Validation { ref source }) =>
                    matches!(**source, DataValidationError::MalformedVorgang { .. }),
                _ => false,
            },
            "{result:?}"
        );
        scenario.teardown().await;
    }
}
