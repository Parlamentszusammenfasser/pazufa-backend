use crate::PazufaServer;
use crate::Result;
use crate::db::merge::MatchState;
use crate::db::retrieve;
use crate::error::DataValidationError;
use crate::error::PazufaError;
use crate::models;
use pazufa_models::enumerations::HashStrategy;
#[cfg(feature = "track-validation")]
use std::collections::BTreeSet;
use std::string::ToString;
use uuid::Uuid;

/// this function determines what means "matching enough".
/// A Vorgang is matched to another, these criteria must hold:
/// (wp, typ)_old == (wp, typ)_new &&
/// (a `vg_ident` matches AND a bundesland matches within the stations
/// OR
///  the api id matches
/// )
pub(crate) async fn vorgang_merge_candidates(
    model: &models::Vorgang,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &PazufaServer,
) -> Result<MatchState<i32>> {
    let obj = "merged Vorgang";
    let (mut ident_list, mut identtyp_list) = (vec![], vec![]);
    for id in &model.ids {
        ident_list.push(id.id.clone());
        identtyp_list.push(id.typ.clone());
    }

    let parls: BTreeSet<_> = model
        .stationen
        .iter()
        .map(|s| s.gremium.parlament)
        .collect();
    let parls: Vec<_> = parls.iter().map(ToString::to_string).collect();

    let statlen = i32::try_from(model.stationen.len()).map_err(|e| PazufaError::Other {
        message: format!("value was unable to be converted to i32: {e}"),
    })?;
    let result = sqlx::query!(
        "WITH 
        parl_class AS(
            SELECT p.id as p_id, classcode as c_id
            FROM parlament p, UNNEST($8::text[], $9::int4[]) as pinp(parlshort, classcode)
            WHERE p.value = pinp.parlshort
        ),
        db_id_table AS (
            SELECT rel_vorgang_ident.vg_id as vg_id, identifikator as ident, vg_ident_typ.value as idt_str
            FROM vg_ident_typ, rel_vorgang_ident 
            WHERE vg_ident_typ.id = rel_vorgang_ident.typ),

	    initds_vwtable AS ( --vorworte von initiativdrucksachen von stationen
			SELECT s.vg_id, d.vorwort, d.volltext FROM dokument d
				INNER JOIN rel_station_dokument rsd ON rsd.dok_id=d.id
				INNER JOIN dokumententyp dt ON dt.id=d.typ
				INNER JOIN station s ON s.id = rsd.stat_id
				WHERE rsd.stat_id=s.id
				AND (dt.value='entwurf' OR dt.value = 'preparl-entwurf')
		),
        vorgang_class AS (
            SELECT s.vg_id as vid, pc.c_id as cid 
                FROM gremium g, station s, parl_class pc
                WHERE s.gr_id = g.id AND g.parl = pc.p_id
            GROUP BY vid, cid
        )


    SELECT DISTINCT(vorgang.id), vorgang.api_id 
        FROM vorgang -- gib vorgänge, bei denen
    INNER JOIN vgtyp_class vtc  ON vtc.t_id = vorgang.typ
	WHERE
    (vorgang.wahlperiode = $4 AND   -- wahlperiode und 
        vtc.c_id = (                -- vgtyp-class übereinstimmen
            SELECT c_id FROM vgtyp_class 
                INNER JOIN vorgangstyp vt ON vt.id = vgtyp_class.t_id
            WHERE vt.value = $5
            ) 
    ) AND
    (
        vorgang.api_id = $1         -- entweder die API ID genau übereinstimmt (trivialer Fall) ODER
    OR                     
	(                               -- eine übereinstimmende ID existiert
		(EXISTS (SELECT 1 FROM UNNEST($2::text[], $3::text[]) as eingabe(ident, typ), db_id_table WHERE 
			db_id_table.vg_id = vorgang.id AND
			eingabe.ident = db_id_table.ident AND
			eingabe.typ = db_id_table.idt_str)
		) 
        AND -- ein übereinstimmendes Bundesland existiert, 
            -- <=> eine station existiert, die dasselbe Bundesland wie eine Station der Eingabe hat 
        (EXISTS (
                SELECT 1 FROM UNNEST($6::text[]) as in_parls(ip), vorgang_class vc, parlament p, parl_class pc
            WHERE
                ip = p.value AND
                pc.p_id = p.id AND
                pc.c_id = vc.cid AND
                vorgang.id = vid
            )
            -- oder eine der beiden keine stationen hat, an denen man das bundesland feststellen könnte
            OR $7 = 0 OR NOT EXISTS(SELECT 1 FROM station s WHERE s.vg_id = vorgang.id)
        )
	    )
    );",
        model.api_id, &ident_list[..], &identtyp_list[..], i32::from(model.wahlperiode),
        srv.guard_ts(&model.typ, model.api_id, obj), &parls[..],
        statlen,
        &srv.tracks.parl_class.0[..], &srv.tracks.parl_class.1[..]
    ).fetch_all(executor).await?;

    tracing::debug!(
        "Found {} matches for Vorgang with api_id: {}",
        result.len(),
        model.api_id
    );

    Ok(match result.len() {
        0 => MatchState::NoMatch,
        1 => MatchState::ExactlyOne(result[0].id),
        _ => {
            tracing::warn!(
                "Mehrere Vorgänge gefunden, die als Kandidaten für Merge infrage kommen für den Vorgang `{}`:\n{:?}",
                model.api_id,
                result.iter().map(|r| r.api_id).collect::<Vec<_>>()
            );
            MatchState::Ambiguous(result.iter().map(|x| x.id).collect())
        }
    })
}

/// matches IFF:
/// `vg_id_old` == `vg_id_new` AND
/// (`api_id_old` == `api_id_new`
/// OR
///  (typ, gremium)_old == (typ, gremium)_new AND
///     (one matching document exists OR both are empty)
/// )
pub(crate) async fn station_merge_candidates(
    model: &models::Station,
    vorgang: i32,
    executor: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<MatchState<i32>> {
    let obj = "merged station";
    let api_id = model.api_id.unwrap_or(uuid::Uuid::now_v7());

    let conflict: Vec<uuid::Uuid> = sqlx::query!(
        "SELECT v.api_id vapi FROM station s, vorgang v
            WHERE
            s.api_id = $1 AND s.vg_id <> $2",
        model.api_id,
        vorgang
    )
    .map(|r| r.vapi)
    .fetch_all(&mut **executor)
    .await?;

    if !conflict.is_empty() {
        let err = format!(
            "Tried to insert a station {api_id} into the database that matches by api id, but is associated with a different Vorgang ({}).\n
            This always indicates an error.",
            conflict[0],
        );
        tracing::error!("{}", &err);
        return Err(DataValidationError::Conflict { reason: err }.into());
    }
    // now check for similar stations
    let (gr_name, gr_wp, gr_parl) = (
        model.gremium.name.clone(),
        i32::from(model.gremium.wahlperiode),
        model.gremium.parlament.to_string(),
    );
    let mut similar_doc_ids = vec![];

    for d in &model.dokumente {
        let doc: &models::Dokument = match d {
            models::DokumentOrApiId::ApiId(dapi_id) => {
                // retrieve, then do the same as below
                let doc_id =
                    sqlx::query_scalar!("SELECT id FROM dokument d WHERE d.api_id = $1", dapi_id)
                        .fetch_optional(&mut **executor)
                        .await?;
                let Some(doc_id) = doc_id else {
                    return Err(DataValidationError::MalformedVorgang {
                        reason: format!(
                            "This API ID for a document is not in the database: {dapi_id}"
                        ),
                    }
                    .into());
                };
                &retrieve::dokument_by_id(doc_id, &mut *executor).await?
            }
            models::DokumentOrApiId::Dokument(doc) => doc,
        };
        if let MatchState::ExactlyOne(id) =
            dokument_merge_candidates(doc, &mut **executor, srv).await?
        {
            similar_doc_ids.push(id);
        }
    }

    let result = sqlx::query!(
        "SELECT s.id, s.api_id FROM station s
        INNER JOIN stationstyp st ON st.id=s.typ
        INNER JOIN gremium g ON g.id=s.gr_id
        INNER JOIN parlament p ON p.id = g.parl
        WHERE s.api_id = $1 OR
        (s.vg_id = $2 AND st.value = $3 AND  -- vorgang und stationstyp übereinstimmen
        (g.name = $4) AND   -- gremium.name übereinstimmt
        (p.value = $5) AND  -- gremium.parlamentname übereinstimmt
        (g.wp = $6) AND     -- gremium.wahlperiode übereinstimmt

        --mindestens ein Dokument auch in der anderen Station angehängt ist:
        (EXISTS (SELECT 1 FROM rel_station_dokument rsd 
            WHERE rsd.stat_id = s.id
            AND rsd.dok_id IN (SELECT i FROM UNNEST($7::int4[]) blub(i))
	        ) OR -- oder beide stationen keine Dokumente enthalten
             ( 
                NOT EXISTS(SELECT 1 FROM rel_station_dokument WHERE stat_id = s.id) AND
                NOT EXISTS(SELECT 1 FROM UNNEST($7::int4[]))
            )
        ))",
        model.api_id,
        vorgang,
        srv.guard_ts(&model.typ, api_id, obj),
        gr_name,
        gr_parl,
        gr_wp,
        &similar_doc_ids[..]
    )
    .fetch_all(&mut **executor)
    .await?;
    tracing::debug!(
        "Found {} matches for Station with api_id: {}",
        result.len(),
        api_id
    );

    Ok(match result.len() {
        0 => MatchState::NoMatch,
        1 => MatchState::ExactlyOne(result[0].id),
        _ => {
            tracing::warn!(
                "Found multiple Stations that are merge candidates for Station `{}`:\n{:?}",
                api_id,
                result.iter().map(|r| r.api_id).collect::<Vec<_>>()
            );
            MatchState::Ambiguous(result.iter().map(|x| x.id).collect())
        }
    })
}

/// matches IFF:
/// `api_id_old` == `api_id_new` OR
/// EXISTS(`(hash, hs)_old` == `(hash, hs)_new`) AND (`subdoc_id_old` == `subdoc_id_new`)
/// OR
/// (typ, drucksNr, `zp_referenz`~12h)_old == (typ, drucksnr, `zp_referenz`~12h)_new
pub(crate) async fn dokument_merge_candidates(
    model: &models::Dokument,
    executor: impl sqlx::PgExecutor<'_>,
    srv: &PazufaServer,
) -> Result<MatchState<i32>> {
    let mut hash_values: Vec<String> = vec![];
    let mut hash_types: Vec<String> = vec![];
    match &model.hash {
        models::dokument::HashWrapper::Legacy(h) => {
            hash_values.push(h.clone());
            hash_types.push(HashStrategy::Sha256Bytes.to_string());
        }
        models::dokument::HashWrapper::Future(hashes) => {
            for ht in hashes {
                hash_values.push(ht.value.clone());
                hash_types.push(ht.strategy.to_string());
            }
        }
    }
    let dids = sqlx::query_scalar!(
        "SELECT d.id FROM dokument d
        INNER JOIN dokumententyp dt ON dt.id = d.typ 
        WHERE 
        -- ein übereinstimmender hash existiert
        ( EXISTS (
            SELECT 1 FROM 
                dok_hashes dh,
                hash_strategy hs,
                UNNEST($1::text[], $2::text[]) as input_values(value, tp)
            WHERE dh.dok_id = d.id
            AND hs.id = dh.hash_strat_id
            AND input_values.value = dh.value
            AND input_values.tp = hs.value
            )
            AND
        -- und eine übereinstimmende subdoc_id
           ( d.subdoc_id = $3 OR ( d.subdoc_id IS NULL AND $3 IS NULL ))
         )
        OR
        d.api_id = $4 OR
        (d.drucksnr = $5 AND dt.value = $6
            AND ($7 BETWEEN (d.zp_referenz-'12 hours'::interval) 
            AND (d.zp_referenz+'12 hours'::interval)))",
        &hash_values[..],
        &hash_types[..],
        model.subdoc_id.map(i32::from),
        model.api_id,
        model.drucksnr,
        srv.guard_ts(
            &model.typ,
            model.api_id.unwrap_or(Uuid::nil()),
            "dok_merge_candidates"
        ),
        model.zp_referenz
    )
    .fetch_all(executor)
    .await?;
    if dids.is_empty() {
        Ok(MatchState::NoMatch)
    } else if dids.len() == 1 {
        Ok(MatchState::ExactlyOne(dids[0]))
    } else {
        Ok(MatchState::Ambiguous(dids))
    }
}

#[cfg(test)]
mod candid_test {
    use super::*;
    use crate::api::vorgang::vorgang_id_put;
    use crate::db::merge::MatchState;
    use crate::utils::testing::generate::random::random_uuid;
    use crate::utils::testing::{TestSetup, generate};
    use chrono::DateTime;
    use pazufa_models::{self, params};
    use rand::{SeedableRng, rngs::StdRng};
    use tracing_test::traced_test;

    #[tokio::test]
    #[traced_test]
    async fn vorgang_test() {
        let setup = TestSetup::new("test_vorgang_candidates").await;
        let srv = &setup.server;

        #[cfg(feature = "track-validation")]
        let vgs: Vec<_> = vec![
            generate::random::vorgang_with_stations(
                1,
                models::Vorgangstyp::BuEinspruchInibreg,
                "IL",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                2,
                models::Vorgangstyp::GgLandParl,
                "RSILAZ",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                3,
                models::Vorgangstyp::GgLandParl,
                "ILAALJG",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                4,
                models::Vorgangstyp::BuEinspruchInibreg,
                "ILAAA",
                &srv.tracks.station_mapping,
            ),
        ];

        for vg in &vgs {
            tracing::debug!("Vorgang: {}", vg.api_id);
            for s in &vg.stationen {
                tracing::debug!("Station api id: {}", s.api_id.expect(""));
            }
        }

        #[cfg(not(feature = "track-validation"))]
        let vgs: Vec<_> = (1..5).map(generate::random::vorgang).collect();
        // insert vorgang 1,2,3, ...
        for vg in &vgs {
            tracing::info!("Inserting {}", vg.api_id);
            vorgang_id_put(srv, 1, &params::ApiIdPathParam { api_id: vg.api_id }, vg)
                .await
                .expect("Failed ot get valid server respone when trying to push new Vorgang.");
        }
        let mut tx = srv
            .sqlx_db
            .begin()
            .await
            .expect("Failed to start dabase transaction.");
        tracing::info!("check wether all conditions are 'enough' to find a specific station");
        let mod_v0 = models::Vorgang {
            api_id: uuid::Uuid::nil(),
            ..vgs[0].clone()
        };
        tracing::info!("matching on exactly one if only differing el is the api_id");
        let candidate = super::vorgang_merge_candidates(&mod_v0, &mut *tx, srv)
            .await
            .expect("Failed to find merge candidate.");
        assert!(matches!(candidate, MatchState::ExactlyOne(_)));
        tracing::info!(
            "check wether insufficient uniqueness conditions yield appropriate results: no match for something not in the db"
        );
        let candidate =
            super::vorgang_merge_candidates(&generate::default_vorgang(), &mut *tx, srv)
                .await
                .expect("Failed to find non-existent merge candidate.");
        assert!(matches!(candidate, MatchState::NoMatch));

        tracing::info!("ambiguous match for ambiguous conditions");
        let mut rng = StdRng::seed_from_u64(0);
        let double_vg = models::Vorgang {
            api_id: Uuid::nil(), // different UUID, but same other criteria
            stationen: vgs[0]
                .stationen
                .iter()
                .cloned()
                .map(|s| models::Station {
                    api_id: Some(random_uuid(&mut rng)),
                    ..s
                })
                .collect(),
            ..vgs[0].clone()
        };
        vorgang_id_put(
                srv,
                1,
                &params::ApiIdPathParam {
                    api_id: double_vg.api_id,
                },
                &double_vg,
            )
            .await
            .expect("Failed to get valid server response when trying to push Vorgang with ambigous merge conditions.");

        let candidates = super::vorgang_merge_candidates(&double_vg, &mut *tx, srv).await;
        assert!(matches!(candidates, Ok(MatchState::Ambiguous(_))));
        setup.teardown().await;
    }
    #[tokio::test]
    async fn vorgang_gg_equivalence_test() {
        let setup = TestSetup::new("vorgang_gg_equivalence_test").await;
        let srv = &setup.server;
        let vg_a = generate::random::vorgang_with_stations(
            470,
            models::Vorgangstyp::BuEinspruchInibreg,
            "IL",
            &srv.tracks.station_mapping,
        );
        let vg_b = models::Vorgang {
            typ: models::Vorgangstyp::BuZustimmungInibreg,
            ..vg_a.clone()
        };
        let _ = vorgang_id_put(
            srv,
            1,
            &params::ApiIdPathParam {
                api_id: vg_a.api_id,
            },
            &vg_a,
        )
        .await
        .expect("Vorgang loading failed");

        let mut tx = srv
            .sqlx_db
            .begin()
            .await
            .expect("Starting a Transaction failed");
        let candidates = super::vorgang_merge_candidates(&vg_b, &mut *tx, srv).await;
        assert!(matches!(candidates, Ok(MatchState::ExactlyOne(_))));
        setup.teardown().await;
    }

    #[tokio::test]
    async fn station_test() {
        let setup = TestSetup::new("test_station_candidates").await;
        let srv = &setup.server;
        let vg = generate::default_vorgang();
        let stat = generate::default_station();
        let _ = vorgang_id_put(srv, 1, &params::ApiIdPathParam { api_id: vg.api_id }, &vg)
            .await
            .expect("Vorgang loading failed");

        let mut tx = srv
            .sqlx_db
            .begin()
            .await
            .expect("Starting a Transaction failed");
        let singular_match = station_merge_candidates(&stat, 1, &mut tx, srv).await;
        assert!(matches!(singular_match, Ok(MatchState::ExactlyOne(_))));

        let conflicting_match = station_merge_candidates(&stat, 2, &mut tx, srv).await;
        assert!(match conflicting_match {
            Err(crate::error::PazufaError::Validation { source }) =>
                matches!(*source, DataValidationError::Conflict { .. }),
            _ => false,
        });

        setup.teardown().await;
    }

    #[traced_test]
    #[tokio::test]
    #[allow(clippy::too_many_lines, reason = "Too many lines are fine for testing")]
    async fn dokument_test() {
        let setup = TestSetup::new("test_dokument_candidates").await;
        let srv = &setup.server;

        #[cfg(feature = "track-validation")]
        let vgs = [
            models::Vorgang {
                stationen: vec![models::Station {
                    dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                        generate::random::dokument(0),
                    ))],
                    gremium: models::Gremium {
                        parlament: models::Parlament::Bt,
                        ..generate::default_gremium()
                    },
                    typ: models::Stationstyp::ParlInitiativ,
                    ..generate::random::station(0)
                }],
                typ: models::Vorgangstyp::BuEinspruchInibreg,
                ..generate::random::vorgang(0)
            },
            generate::random::vorgang_with_stations(
                1,
                models::Vorgangstyp::BuEinspruchInibreg,
                "IL",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                2,
                models::Vorgangstyp::GgLandParl,
                "RSILAZ",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                3,
                models::Vorgangstyp::GgLandParl,
                "ILAALJG",
                &srv.tracks.station_mapping,
            ),
            generate::random::vorgang_with_stations(
                4,
                models::Vorgangstyp::BuEinspruchInibreg,
                "ILAAA",
                &srv.tracks.station_mapping,
            ),
        ];

        #[cfg(not(feature = "track-validation"))]
        let vgs: Vec<_> = (1..5)
            .map(generate::random::vorgang)
            .chain(std::iter::once(models::Vorgang {
                stationen: vec![models::Station {
                    dokumente: vec![models::DokumentOrApiId::Dokument(
                        generate::random::dokument(0),
                    )],
                    gremium: models::Gremium {
                        parlament: models::Parlament::Bt,
                        ..generate::default_gremium()
                    },
                    typ: models::Stationstyp::ParlInitiativ,
                    ..generate::random::station(0)
                }],
                typ: models::Vorgangstyp::BuEinspruchInibreg,
                ..generate::random::vorgang(0)
            }))
            .collect();
        // insert vorgang 1,2,3, ...
        for vg in &vgs {
            vorgang_id_put(srv, 1, &params::ApiIdPathParam { api_id: vg.api_id }, vg)
                .await
                .expect("Failed to get valid server response when trying to push new Vorgang.");
        }
        let mut tx = srv
            .sqlx_db
            .begin()
            .await
            .expect("Failed to start dabase transaction.");
        let test_docs = [
            // by api_id
            models::Dokument {
                hash: models::dokument::HashWrapper::Legacy(
                    "918439184791824719184391847918247183919388".to_string(),
                ),
                drucksnr: Some("123/1241204".to_string()),
                zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                    .expect("Failed to parse datetime. Test broken.")
                    .to_utc(),
                typ: models::Doktyp::Antwort,
                ..generate::random::dokument(0)
            },
            // by hash
            models::Dokument {
                api_id: Some(uuid::Uuid::now_v7()),
                drucksnr: Some("123/1241204".to_string()),
                zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                    .expect("Failed to parse datetime. Test broken.")
                    .to_utc(),
                typ: models::Doktyp::Antwort,
                ..generate::random::dokument(0)
            },
            // by typ, refts, drucksnr
            models::Dokument {
                api_id: Some(uuid::Uuid::now_v7()),
                hash: models::dokument::HashWrapper::Legacy(
                    "918439184791824719184391847918247183919388".to_string(),
                ),
                ..generate::random::dokument(0)
            },
        ];
        for (i, d) in test_docs.iter().enumerate() {
            let r = dokument_merge_candidates(d, &mut *tx, srv)
                .await
                .expect("Failed to find merge candidate");
            assert!(matches!(r, MatchState::ExactlyOne(_)), "Dok {i} was {r:?}");
        }
        // no matching identifiers
        let fail = models::Dokument {
            api_id: Some(uuid::Uuid::now_v7()),
            hash: models::dokument::HashWrapper::Legacy(
                "918439184791824719184391847918247183919388".to_string(),
            ),
            drucksnr: Some("123/1241204".to_string()),
            zp_referenz: DateTime::parse_from_rfc3339("2025-09-17T12:12:14Z")
                .expect("Failed to parse datetime. Test broken.")
                .to_utc(),
            typ: models::Doktyp::Antwort,
            ..generate::random::dokument(0)
        };
        let r = dokument_merge_candidates(&fail, &mut *tx, srv)
            .await
            .expect("Failed to find non-existent merge candidate.");
        assert!(matches!(r, MatchState::NoMatch));
        setup.teardown().await;
    }
}
