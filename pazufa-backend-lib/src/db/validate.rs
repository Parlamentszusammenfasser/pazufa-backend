use crate::{Result, error::DataValidationError};
use pazufa_models::{self as models, Parlament};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
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
    pub parl_class: (Box<[String]>, Box<[i32]>),
}

fn parl_classes() -> (Box<[String]>, Box<[i32]>) {
    let (mut v1, mut v2) = (vec![], vec![]);
    for p in Parlament::all_variants() {
        v1.push(p.to_string());
        v2.push(i32::from(p.class()));
    }
    (v1.into_boxed_slice(), v2.into_boxed_slice())
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
                        Regex::new(rx).expect("Malformed tracks file")
                    })
                })
            })
            .collect();
        let sm: BTreeMap<_, char> = rt.stations.iter().map(|(k, v)| (*v, *k)).collect();
        Self {
            version: rt.version,
            station_mapping: sm,
            trackmap,
            parl_class: parl_classes(),
        }
    }
    /// Validates a Vorgang, meaning it checks whether it adheres to the constraints specified
    /// in the tracks file.
    /// # Errors
    /// Fails to validate if:
    /// - there is more than one parliament of one legislative class in there
    /// - the track validation of any single-parliament track failed
    pub fn validate(&self, vg: &models::Vorgang) -> Result<()> {
        let mut stations = vg.stationen.clone();
        stations.sort_by_key(|s| (s.zp_start, s.meta_tiebreaker));
        let parls: BTreeSet<models::Parlament> =
            stations.iter().map(|s| s.gremium.parlament).collect();
        Self::singular_legislative_class(&parls)?;

        if stations
            .iter()
            .any(|s| !self.station_mapping.contains_key(&s.typ))
        {
            return Err(crate::error::DataValidationError::MalformedVorgang {
                reason: "Vorgang contains unmapped station type (probably 'sonstig')".to_owned(),
            }
            .into());
        }
        let mut validation_errors = Vec::new();
        for p in parls {
            tracing::trace!("now in parliament {p}");
            let regex = self.trackmap.get(&(p, vg.typ)).ok_or(
            DataValidationError::MalformedVorgang { reason: format!("A vorgang of type {} is not specified for parliament {p}. If you believe this to be an error, open an issue: https://codeberg.org/PaZuFa/parlamentszusammenfasser/issues", vg.typ) }
            )?;
            tracing::trace!("Tracks retrieved");
            let p_subvec: Vec<models::Station> = stations
                .iter()
                .filter(|x| x.gremium.parlament == p)
                .cloned()
                .collect();
            tracing::trace!("Subvec constructed");
            let p_substr: String = p_subvec
                .iter()
                .map(|s| self.station_mapping[&s.typ])
                .collect();
            tracing::trace!("Substr constructed: {:?}", p_substr);

            // matching and checking
            let (matched, consumed) = regex.matches(&p_substr);
            tracing::trace!("{:?} matched like so: {:?}", p_substr, (matched, consumed));
            if !matched {
                let conflicts: Vec<_> =
                    regex.conflicting_stations(&p_subvec, &self.station_mapping, |s, m| m[&s.typ]);
                let conflicts_str = conflicts.iter().fold("Conflicts:".to_owned(), |accu, s| {
                    format!("{accu}\n({} / {})", s.zp_start, s.typ)
                });
                let station_ordering = p_subvec.iter().fold("Stations:".to_owned(), |accu, s| {
                    format!("{accu}\n({} / {})", s.zp_start, s.typ)
                });

                let reason = format!(
                    "The Vorgang with this station ordering: {station_ordering} has at least one station that is not adhering to the track for parliament {p} and vgtyp {}: {conflicts_str}",
                    vg.typ
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

    fn singular_legislative_class(parls: &BTreeSet<models::Parlament>) -> Result<()> {
        let mut class = None;
        for p in parls {
            if class.is_none() {
                class = Some(p.class());
            } else if let Some(c) = class
                && c != p.class()
            {
                let s_parls: Vec<_> = parls.iter().map(ToString::to_string).collect();
                let reason = format!(
                    "The Vorgang contains Stations from more than one legislative class (parliament) in it's body: {s_parls:?}, which invalidates it"
                );
                tracing::warn!("{reason}");
                return Err(DataValidationError::MalformedVorgang { reason }.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test_regex {
    use super::*;
    use crate::utils::testing::generate;
    use tracing_test::traced_test;

    fn open_tracks() -> super::Tracks {
        Tracks::new("tracks.toml")
    }

    #[tokio::test]
    async fn sonstig_station_run() {
        let tracks = open_tracks();

        let vg = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::Sonstig,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let res = tracks.validate(&vg);
        assert!(matches!(
            res,
            Err(crate::error::PazufaError::Validation { .. })
        ));
    }

    #[traced_test]
    #[tokio::test]
    async fn vorgang_validate_starmatching_consumption() {
        let tracks = open_tracks();

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
        let res = tracks.validate(&catastrophe);
        assert!(matches!(
            res,
            Err(crate::error::PazufaError::Validation { .. })
        ));
    }

    #[tokio::test]
    async fn validate_vorgang_full_fail() {
        let tracks = open_tracks();
        let vorgang = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let res = tracks.validate(&vorgang);
        assert!(res.is_err(), "{res:?}");
    }

    #[tokio::test]
    async fn tiebreaker_orders_stations() {
        let tracks = open_tracks();
        let now = chrono::Utc::now();
        let vorgang = models::Vorgang {
            stationen: vec![
                models::Station {
                    typ: models::Stationstyp::ParlInitiativ,
                    zp_start: now,
                    meta_tiebreaker: Some(2),
                    ..generate::default_station()
                },
                models::Station {
                    typ: models::Stationstyp::ParlVollvlsgn,
                    zp_start: now,
                    meta_tiebreaker: Some(1),
                    ..generate::default_station()
                },
            ],
            ..generate::default_vorgang()
        };
        let result = tracks.validate(&vorgang);
        assert!(result.is_err(), "{result:?}");

        let vorgang = models::Vorgang {
            stationen: vec![
                models::Station {
                    typ: models::Stationstyp::ParlInitiativ,
                    zp_start: now,
                    meta_tiebreaker: Some(1),
                    ..generate::default_station()
                },
                models::Station {
                    typ: models::Stationstyp::ParlVollvlsgn,
                    zp_start: now,
                    meta_tiebreaker: Some(2),
                    ..generate::default_station()
                },
            ],
            ..generate::default_vorgang()
        };
        let result = tracks.validate(&vorgang);
        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn validate_vorgang_formalia_wrong() {
        let tracks = open_tracks();
        let vorgang = generate::default_vorgang();
        let vorgang = models::Vorgang {
            typ: models::Vorgangstyp::GgLandVolk,
            ..vorgang
        };
        let res = tracks.validate(&vorgang);
        assert!(
            match res {
                Err(crate::error::PazufaError::Validation { ref source }) =>
                    matches!(**source, DataValidationError::MalformedVorgang { .. }),
                _ => false,
            },
            "{res:?}"
        );
    }
}
