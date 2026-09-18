/// Handles merging of two datasets.
/// vorgang, station and dokument are mergeable, meaning their data is not atomic.
/// Stellungnahme is handled like dokument with the rest being overridable data points
/// `API_ID` or other uniquely identifying information is not overridden, but preserved.
/// array-like structures are merged by a modified union operation:
/// for each element:
///     - if it is mergeable and one merge candidate found, merge
///     - if it is not mergeable and has a match in the existing set, the existing element takes precedence and is not replaced
///     - if it is not mergeable and has no match it is added to the set.
/// basic data items are to be overridden by newer information.
/// Excempt from this is the `api_id`, since this is a permanent document identifier.
use super::MatchState;
use super::candidates::{
    dokument_merge_candidates, station_merge_candidates, vorgang_merge_candidates,
};
use crate::api::RoundTimestamp;
use crate::db::KeyIndex;
use crate::db::insert::{self, insert_or_retrieve_autor, update_scraper_touched_vorgang};
#[cfg(feature = "track-validation")]
use crate::db::validate;
use crate::error::{DataValidationError, PazufaError};
use crate::models::enumerations::{HashStrategy, Mime};
use crate::utils::notify::notify_ambiguous_match;
use crate::{PazufaServer, Result, models};
use tracing::{debug, info, warn};
use uuid::Uuid;

async fn merge_hashes_of_dok(
    db_id: i32,
    hash: &models::HashWrapper,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<()> {
    let mut hash_values: Vec<String> = vec![];
    let mut hash_types: Vec<String> = vec![];
    let mut hash_mimes: Vec<String> = vec![];
    match hash {
        models::dokument::HashWrapper::Legacy(h) => {
            hash_values.push(h.clone());
            hash_types.push(HashStrategy::Sha256Bytes.to_string());
            hash_mimes.push(Mime::ApplicationPdf.to_string());
        }
        models::dokument::HashWrapper::Future(hashes) => {
            for ht in hashes {
                hash_values.push(ht.value.clone());
                hash_types.push(ht.strategy.to_string());
                hash_mimes.push(ht.mime.to_string());
            }
        }
    }
    sqlx::query!(
        "INSERT INTO dok_hashes(dok_id, hash_strat_id, value, mime_id)
        SELECT $1, hs.id, i.value, m.id
            FROM 
                UNNEST($2::text[], $3::text[], $4::text[]) as i(strat, value, mime), 
                hash_strategy hs,
                mime m
            WHERE 
                hs.value = i.strat AND
                m.value = i.mime
        ON CONFLICT (dok_id, hash_strat_id) 
        DO UPDATE SET 
            value=EXCLUDED.value,
            mime_id=EXCLUDED.mime_id
            ",
        db_id,
        &hash_types[..],
        &hash_values[..],
        &hash_mimes[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn merge_zusammenfassung_of_dok(
    db_id: i32,
    zusammenfassung: &models::ZusammenfassungWrapper,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<()> {
    let mut zf_values: Vec<String> = vec![];
    let mut zf_types: Vec<String> = vec![];
    match &zusammenfassung {
        models::dokument::ZusammenfassungWrapper::Legacy(zf) => {
            zf_values.push(zf.clone());
            zf_types.push("full".to_string());
        }
        models::dokument::ZusammenfassungWrapper::Future(zfs) => {
            for zf in zfs {
                zf_values.push(zf.inhalt.clone());
                zf_types.push(zf.typ.clone());
            }
        }
    }
    sqlx::query!(
        "INSERT INTO zusammenf_typ (value) 
            SELECT * FROM UNNEST($1::text[]) ON CONFLICT (value) DO NOTHING",
        &zf_types[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO dok_zusammenfassungen(dok_id, zf_typ, value)
        SELECT $1, zft.id, i.value
            FROM UNNEST($2::text[], $3::text[]) as i(tp, value), zusammenf_typ as zft
            WHERE zft.value = i.tp
        ON CONFLICT (dok_id, zf_typ) DO UPDATE SET value=EXCLUDED.value",
        db_id,
        &zf_types[..],
        &zf_values[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn execute_merge_dokument(
    model: &models::Dokument,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<()> {
    let db_id = candidate;
    let original_state = crate::db::retrieve::dokument_by_id(db_id, tx).await?;
    // master update
    sqlx::query!(
        "UPDATE dokument SET
        drucksnr = $2, titel =$3,
        kurztitel = COALESCE($4, kurztitel), vorwort=COALESCE($5, vorwort),
        volltext=COALESCE($6, volltext),
        zp_lastmod=$7, link=$8, meinung=$9
        WHERE dokument.id = $1
        ",
        db_id,
        model.drucksnr,
        model.titel,
        model.kurztitel,
        model.vorwort,
        model.volltext,
        model.zp_modifiziert,
        model.link,
        model.meinung.map(i32::from)
    )
    .execute(&mut **tx)
    .await?;
    // hashes: replace if strategy bin conflicts
    merge_hashes_of_dok(db_id, &model.hash, tx).await?;

    // zusammenfassung: replace by type
    if let Some(zusf) = &model.zusammenfassung {
        merge_zusammenfassung_of_dok(db_id, zusf, tx).await?;
    }

    // schlagworte::UNION
    insert::insert_schlagworte_of_dok(db_id, &model.schlagworte, tx).await?;
    // autoren::UNION
    let mut aids = vec![];
    for a in &model.autoren {
        aids.push(insert_or_retrieve_autor(a, tx).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_dok_autor(dok_id, aut_id)
    SELECT $1, blub FROM UNNEST($2::int4[]) as blub 
    ON CONFLICT DO NOTHING",
        db_id,
        &aids[..]
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "INSERT INTO scraper_touched_dokument(dok_id, collector_key, scraper) 
    VALUES ($1, $2, $3) 
    ON CONFLICT(dok_id, scraper) DO UPDATE SET time_stamp=NOW()",
        db_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT dok_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY dok_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_dokument
        )
        DELETE FROM scraper_touched_dokument st
        USING ranked_objects ro
        WHERE st.dok_id=ro.dok_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(srv.config.per_object_scraper_log_size)
    )
    .execute(&mut **tx)
    .await?;
    let merged_state = crate::db::retrieve::dokument_by_id(db_id, tx).await?;
    if original_state.with_round_timestamps() != merged_state.with_round_timestamps() {
        sqlx::query!(
            "UPDATE dokument SET meta_last_modified = NOW() WHERE id = $1",
            db_id
        )
        .execute(&mut **tx)
        .await?;
    }
    info!("Merging Dokument into Database successful");
    Ok(())
}
pub(crate) async fn insert_or_merge_dok(
    dok: &models::DokumentOrApiId,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<Option<i32>> {
    match dok {
        models::DokumentOrApiId::ApiId(uuid) => {
            let id = sqlx::query!("SELECT id FROM dokument d WHERE d.api_id = $1", uuid)
                .map(|r| r.id)
                .fetch_optional(&mut **tx)
                .await?;
            if let Some(id) = id {
                Ok(Some(id))
            } else {
                Err(DataValidationError::IncompleteDataSupplied {
                    input: format!("Supplied uuid `{uuid}` as document id without a body, but no such ID is in the database.") }.into())
            }
        }
        models::DokumentOrApiId::Dokument(dok) => {
            let matches = dokument_merge_candidates(dok, &mut **tx, srv).await?;
            match matches {
                MatchState::NoMatch => {
                    let did =
                        crate::db::insert::insert_dokument(dok, scraper_id, collector_key, tx, srv)
                            .await?;
                    Ok(Some(did))
                }
                MatchState::ExactlyOne(matchmod) => {
                    debug!(
                        "Found exactly one Dokument with db id: {}. Merging...",
                        matchmod
                    );
                    execute_merge_dokument(dok, matchmod, scraper_id, collector_key, tx, srv)
                        .await?;
                    Ok(None)
                }
                MatchState::Ambiguous(matches) => {
                    let api_ids = sqlx::query!(
                        "SELECT api_id FROM dokument WHERE id = ANY($1::int4[])",
                        &matches[..]
                    )
                    .map(|r| r.api_id)
                    .fetch_all(&mut **tx)
                    .await?;
                    notify_ambiguous_match(&api_ids, &dok, "execute merge station.dokumente", srv)?;
                    Err(DataValidationError::AmbiguousMatch {
                        message: "Ambiguous document match(station), see notification".to_string(),
                    }
                    .into())
                }
            }
        }
    }
}

pub(crate) async fn execute_merge_station(
    model: &models::Station,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<()> {
    let db_id = candidate;
    let obj = "merge station";
    let sapi: uuid::Uuid = sqlx::query_scalar!("SELECT api_id FROM station WHERE id = $1", db_id)
        .fetch_one(&mut **tx)
        .await?;
    let vg_id: i32 = sqlx::query_scalar!("SELECT vg_id FROM station WHERE id = $1", db_id)
        .fetch_one(&mut **tx)
        .await?;
    // pre-master updates
    let gr_id = insert::insert_or_retrieve_gremium(&model.gremium, tx).await?;
    // master update
    sqlx::query!(
        "UPDATE station SET 
        gr_id = COALESCE($2, gr_id),
        typ = (SELECT id FROM stationstyp WHERE value = $3),
        titel = COALESCE($4, titel),
        zp_start = $5, zp_modifiziert = COALESCE($6, NOW()),
        trojanergefahr = COALESCE($7, trojanergefahr),
        link = COALESCE($8, link),
        gremium_isff = $9
        WHERE station.id = $1",
        db_id,
        gr_id,
        srv.guard_ts(&model.typ, sapi, obj),
        model.titel,
        model.zp_start,
        model.zp_modifiziert,
        model.trojanergefahr.map(|x| i32::from(x)),
        model.link,
        model.gremium_federf
    )
    .execute(&mut **tx)
    .await?;

    // links::UNION
    sqlx::query!(
        "INSERT INTO rel_station_link(stat_id, link)
        SELECT $1, blub FROM UNNEST($2::text[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        &model.additional_links[..]
    )
    .execute(&mut **tx)
    .await?;

    // schlagworte::UNION
    insert::insert_schlagworte_of_vorgang(vg_id, &model.schlagworte, tx).await?;

    // dokumente::UNION
    let mut insert_ids = vec![];

    for dok in &model.dokumente {
        // if id & not in database: fail.
        // if id & in database: add to list of associated documents
        // if document: match & integrate or insert.
        if let Some(id) = insert_or_merge_dok(dok, scraper_id, collector_key, tx, srv).await? {
            insert_ids.push(id);
        }
    }
    sqlx::query!(
        "INSERT INTO rel_station_dokument(stat_id, dok_id) 
        SELECT $1, did FROM UNNEST($2::int4[]) as did
        ON CONFLICT DO NOTHING",
        db_id,
        &insert_ids[..]
    )
    .execute(&mut **tx)
    .await?;

    // stellungnahmen
    let mut insert_ids = vec![];
    for stln in &model.stellungnahmen {
        if let Some(id) = insert_or_merge_dok(stln, scraper_id, collector_key, tx, srv).await? {
            insert_ids.push(id);
        }
    }

    sqlx::query!(
        "INSERT INTO rel_station_stln(stat_id, dok_id) 
            SELECT $1, did FROM UNNEST($2::int4[]) as did
            ON CONFLICT DO NOTHING",
        db_id,
        &insert_ids[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO scraper_touched_station(stat_id, collector_key, scraper) 
        VALUES ($1, $2, $3) ON CONFLICT(stat_id, scraper) DO UPDATE SET time_stamp=NOW()",
        db_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT stat_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY stat_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_station
        )
        DELETE FROM scraper_touched_station st
        USING ranked_objects ro
        WHERE st.stat_id=ro.stat_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(srv.config.per_object_scraper_log_size)
    )
    .execute(&mut **tx)
    .await?;
    info!("Merging Station into Database successful");
    Ok(())
}

pub(crate) async fn execute_merge_vorgang(
    model: &models::Vorgang,
    candidate: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<()> {
    let db_id = candidate;
    let obj = "Vorgang";
    let vapi = model.api_id;
    let original_state = crate::db::retrieve::vorgang_by_id(candidate, true, &mut *tx).await?;
    // master insert
    sqlx::query!(
        "UPDATE vorgang SET
            titel = $1, kurztitel = $2,
            verfaend = $3, wahlperiode = $4,
            typ = (SELECT id FROM vorgangstyp WHERE value = $5),
            ressort = (SELECT id FROM ressort WHERE value = $6)
        WHERE vorgang.id = $7",
        model.titel,
        model.kurztitel,
        model.verfassungsaendernd,
        i32::try_from(model.wahlperiode).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert wahlperiode as it exceeds i32::MAX: {e}")
        })?,
        srv.guard_ts(&model.typ, vapi, obj),
        model.ressort.map(|x| x.to_string()),
        db_id
    )
    .execute(&mut **tx)
    .await?;
    // sachgebiete
    let sg: Vec<_> = model.sachgebiete.iter().map(|x| *x as i32).collect();
    sqlx::query!(
        "INSERT INTO vg_sachgebiete (vg_id, sg_id) 
        SELECT $1, id FROM sachgebiet
        WHERE value = ANY($2::int4[])
        ON CONFLICT DO NOTHING
        ",
        db_id,
        &sg[..]
    )
    .execute(&mut **tx)
    .await?;
    // initiatoren / initpersonen::UNION
    let mut aids = vec![];
    for a in &model.initiatoren {
        aids.push(insert_or_retrieve_autor(a, tx).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_vorgang_init (vg_id, in_id)
        SELECT $1, blub FROM UNNEST($2::int4[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        &aids[..]
    )
    .execute(&mut **tx)
    .await?;
    // links
    sqlx::query!(
        "INSERT INTO rel_vorgang_links (vg_id, link)
        SELECT $1, blub FROM UNNEST($2::text[]) as blub
        ON CONFLICT DO NOTHING",
        db_id,
        &model.links[..]
    )
    .execute(&mut **tx)
    .await?;
    // identifikatoren
    let (mut ident_list, mut identtyp_list) = (vec![], vec![]);
    for id in &model.ids {
        ident_list.push(id.id.clone());
        identtyp_list.push(id.typ.clone());
    }

    sqlx::query!(
        "INSERT INTO rel_vorgang_ident (vg_id, typ, identifikator)
        SELECT $1, vit.id, ident FROM 
        UNNEST($2::text[], $3::text[]) blub(typ_value, ident)
        INNER JOIN vg_ident_typ vit ON vit.value = typ_value
        ON CONFLICT DO NOTHING
        ",
        db_id,
        &identtyp_list[..],
        &ident_list[..]
    )
    .execute(&mut **tx)
    .await?;

    for stat in &model.stationen {
        match station_merge_candidates(stat, db_id, tx, srv).await? {
            MatchState::NoMatch => {
                insert::insert_station(stat, db_id, scraper_id, collector_key, tx, srv).await?;
            }
            MatchState::ExactlyOne(statid) => {
                debug!("Found exactly one Station with db id: {statid}. Merging...");
                execute_merge_station(stat, statid, scraper_id, collector_key, tx, srv).await?;
            }
            MatchState::Ambiguous(matches) => {
                let mids = sqlx::query!(
                    "SELECT api_id FROM station WHERE id = ANY($1::int4[]);",
                    &matches[..]
                )
                .map(|r| r.api_id)
                .fetch_all(&mut **tx)
                .await?;
                notify_ambiguous_match(&mids, stat, "exec_merge_vorgang: station matching", srv)?;
            }
        }
    }
    // merge Zusammenfassungen as insert
    crate::db::insert::vorgang::insert_zusammenf_for_vorgang(db_id, &model.zusammenfassung, tx)
        .await?;

    // lobbyregistereinträge are just replaced as-is, no merging
    sqlx::query!("DELETE FROM lobbyregistereintrag WHERE vg_id = $1", db_id)
        .execute(&mut **tx)
        .await?;

    for l in &model.lobbyregister {
        let aid = insert_or_retrieve_autor(&l.organisation, tx).await?;
        let lrid: i32 = sqlx::query_scalar!(
            "INSERT INTO lobbyregistereintrag(intention, interne_id, organisation, vg_id, link)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id",
            &l.intention,
            &l.interne_id,
            &aid,
            db_id,
            &l.link
        )
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query!(
            "INSERT INTO rel_lobbyreg_drucksnr(drucksnr, lob_id) 
            SELECT x, $1 FROM UNNEST($2::text[]) as x(x)",
            lrid,
            &l.betroffene_drucksachen
        )
        .execute(&mut **tx)
        .await?;
    }
    update_scraper_touched_vorgang(
        db_id,
        srv.config.per_object_scraper_log_size,
        collector_key,
        scraper_id,
        tx,
    )
    .await?;

    // validation and updating
    let merged_state = crate::db::retrieve::vorgang_by_id(candidate, true, &mut *tx).await?;
    #[cfg(feature = "track-validation")]
    validate::validate_vorgang(&merged_state, srv).await?;
    if original_state.with_round_timestamps() != merged_state.with_round_timestamps() {
        sqlx::query!(
            "UPDATE vorgang
                SET meta_last_modified = NOW() WHERE id = $1",
            db_id
        )
        .execute(&mut **tx)
        .await?;
    }

    info!(
        "Merging of Vg Successful: Merged `{}`(ext) with  `{}`(db)",
        model.api_id,
        sqlx::query!("SELECT api_id FROM vorgang WHERE id = $1", candidate)
            .map(|r| r.api_id)
            .fetch_one(&mut **tx)
            .await?
    );
    Ok(())
}

pub(crate) async fn run_integration(
    model: &models::Vorgang,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    server: &PazufaServer,
) -> Result<()> {
    let mut tx = server.sqlx_db.begin().await?;
    debug!(
        "Looking for Merge Candidates for Vorgang with api_id: {:?}",
        model.api_id
    );
    let candidates = vorgang_merge_candidates(model, &mut *tx, server).await?;
    match candidates {
        MatchState::NoMatch => {
            info!(
                "No Merge Candidate found, Inserting Complete Vorgang with api_id: {:?}",
                model.api_id
            );
            let model = model.clone();
            info!(target: "obj", "Merge(Insert New) Vorgang {}", model.api_id);
            insert::insert_vorgang(&model, scraper_id, collector_key, &mut tx, server).await?;
        }
        MatchState::ExactlyOne(one) => {
            let api_id = sqlx::query!("SELECT api_id FROM vorgang WHERE id = $1", one)
                .map(|r| r.api_id)
                .fetch_one(&mut *tx)
                .await?;
            info!(
                "Matching Vorgang in the DB has api_id: {}, Updating with data from: {}",
                api_id, model.api_id
            );
            info!(target: "obj", "Merge(merge) new Vorgang {} into Vorgang {}", model.api_id, api_id);
            let model = model.clone();
            execute_merge_vorgang(&model, one, scraper_id, collector_key, &mut tx, server).await?;
        }
        MatchState::Ambiguous(many) => {
            warn!(
                "Ambiguous matches for Vorgang with api_id: {:?}",
                model.api_id
            );
            warn!("Transaction not committed, administrators notified");
            debug!("Details:  {:?} \n\n {:?}", model, many);
            let api_ids = sqlx::query!(
                "SELECT api_id FROM vorgang WHERE id=ANY($1::int4[])",
                &many[..]
            )
            .map(|r| r.api_id)
            .fetch_all(&mut *tx)
            .await?;
            notify_ambiguous_match(&api_ids, model, "merging vorgang", server)?;
            tx.rollback().await?;
            return Err(DataValidationError::AmbiguousMatch {
                message: format!(
                    "Tried to merge object with id `{}`, found {} matching VGs.",
                    model.api_id,
                    many.len()
                ),
            }
            .into());
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod scenariotest {
    use crate::api::SortArrays;
    use crate::models::{self, DokumentOrApiId};
    use crate::utils::testing::{TestSetup, generate};
    use crate::{PazufaServer, Result, api::PaginationResponsePart, db::retrieve};
    use uuid::Uuid;

    struct Scenario {
        context: Vec<models::Vorgang>,
        object: models::Vorgang,
        expected: Vec<models::Vorgang>,
        shouldfail: bool,
        test_setup: TestSetup,
    }
    struct ScenarioBuilder {
        context: Vec<models::Vorgang>,
        object: Option<models::Vorgang>,
        expected: Vec<models::Vorgang>,
        shouldfail: bool,
        name: &'static str,
    }
    impl ScenarioBuilder {
        fn with_context(mut self, context: Vec<models::Vorgang>) -> Self {
            self.context = context;
            self
        }
        fn with_expectation(mut self, expectation: Vec<models::Vorgang>) -> Self {
            self.expected = expectation;
            self.expected
                .iter_mut()
                .for_each(crate::api::SortArrays::sort_arrays);
            self.expected.sort_by_key(|a| a.api_id);
            self
        }
        fn with_test_object(mut self, obj: models::Vorgang) -> Self {
            self.object = Some(obj);
            self
        }

        #[allow(unused, reason = "Future proofing for negative test.")]
        fn with_should_fail(mut self, shouldfail: bool) -> Self {
            self.shouldfail = shouldfail;
            self
        }

        async fn build(self) -> Scenario {
            Scenario {
                context: self.context,
                expected: self.expected,
                object: self.object.expect("Expected object to be set"),
                shouldfail: self.shouldfail,
                test_setup: TestSetup::new(self.name).await,
            }
        }
    }
    impl Scenario {
        fn builder(name: &'static str) -> ScenarioBuilder {
            ScenarioBuilder {
                context: vec![],
                object: None,
                expected: vec![],
                shouldfail: false,
                name,
            }
        }

        async fn run(&self) -> Result<()> {
            self.build_context(&self.test_setup.server).await?;
            self.place_object(&self.test_setup.server).await?;
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            self.check_result(&self.test_setup.server).await?;
            self.test_setup.teardown().await;
            Ok(())
        }

        async fn build_context(&self, server: &PazufaServer) -> Result<()> {
            for obj in &self.context {
                super::run_integration(obj, Uuid::nil(), 1, server).await?;
            }
            Ok(())
        }
        async fn place_object(&self, server: &PazufaServer) -> Result<()> {
            super::run_integration(&self.object, Uuid::nil(), 1, server).await?;
            Ok(())
        }
        async fn check_result(&self, server: &PazufaServer) -> Result<()> {
            let paramock = retrieve::VGGetParameters::default();
            let mut tx = server
                .sqlx_db
                .begin()
                .await
                .expect("Failed to create database transaction.");
            let mut db_vorgangs = retrieve::vorgang_by_parameter(
                paramock,
                None,
                Some(PaginationResponsePart::MAX_PER_PAGE),
                false,
                &mut tx,
            )
            .await
            .expect("Failed to get valid server response when trying to get vorgang by id.");
            db_vorgangs
                .1
                .iter_mut()
                .for_each(crate::api::SortArrays::sort_arrays);
            db_vorgangs.1.sort_by_key(|a| a.api_id);

            tx.commit().await?;
            let equality = self
                .expected
                .iter()
                .map(crate::api::RoundTimestamp::with_round_timestamps)
                .collect::<Vec<_>>()
                == db_vorgangs
                    .1
                    .iter()
                    .map(crate::api::RoundTimestamp::with_round_timestamps)
                    .collect::<Vec<_>>();
            if !equality && !self.shouldfail {
                let exp_content = self
                    .expected
                    .iter()
                    .map(|x| {
                        serde_json::to_string_pretty(x)
                            .expect("Failed to serialize Vorgang to JSON.")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let dbv_content = db_vorgangs
                    .1
                    .iter()
                    .map(|x| {
                        serde_json::to_string_pretty(x)
                            .expect("Failed to serialize Vorgang to JSON.")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                std::fs::write(
                    format!("tests/{}_dump.json", self.test_setup.name),
                    format!("{{\n\"expected\": [{exp_content}],\n\"actual\": [{dbv_content}]}}"),
                )
                .expect("Failed to write to file.");
                return Err(crate::error::PazufaError::Other {
                    message: format!(
                        "Expected and Actual Contents were not equal. Dump: tests/{}_dump.json",
                        self.test_setup.name
                    ),
                });
            }
            if equality && self.shouldfail {
                return Err(crate::error::PazufaError::Other {
                    message: "Expected Case to fail, but actual output was equal to expectation"
                        .to_owned(),
                });
            }
            Ok(())
        }
    }
    fn vg_to_expected_shape(vg: &models::Vorgang) -> models::Vorgang {
        let mut vg = vg.clone();
        for s in &mut vg.stationen {
            for d in &mut s.dokumente {
                if let DokumentOrApiId::Dokument(dok) = d {
                    *d = DokumentOrApiId::ApiId(
                        dok.api_id.expect("Station Dokument is missing API id."),
                    );
                }
            }
            for d in &mut s.stellungnahmen {
                if let DokumentOrApiId::Dokument(dok) = d {
                    *d = DokumentOrApiId::ApiId(
                        dok.api_id
                            .expect("Station Stellungnahme is missing API id."),
                    );
                }
            }
        }
        vg.sort_arrays();
        vg
    }
    // one in, again one in, one out
    #[tokio::test]
    #[tracing_test::traced_test]
    async fn test_idempotenz() {
        let vg = generate::default_vorgang();
        let scenario = Scenario::builder("idempotenz")
            .with_context(vec![vg.clone()])
            .with_test_object(vg.clone())
            .with_expectation(vec![vg_to_expected_shape(&vg)])
            .build()
            .await;
        if let Err(e) = scenario.run().await {
            panic!("Scenario \"idempotenz\" failed with the following error: {e}")
        }
    }
    #[tokio::test]
    async fn test_merge_matching_identifier() {
        let vg = generate::default_vorgang();

        let mut vg2 = generate::default_vorgang();
        vg2.api_id = Uuid::nil(); // take out api id matching
        vg2.titel = "Anderer Titel".to_string();
        vg2.stationen = vec![]; // take out vorwort matching

        let mut vg_exp = vg.clone();
        vg_exp.titel = vg2.titel.clone();
        vg_exp.stationen = vg.stationen.clone();

        let scenario = Scenario::builder("merge_matching_ids")
            .with_context(vec![vg])
            .with_test_object(vg2)
            .with_expectation(vec![vg_to_expected_shape(&vg_exp)])
            .build()
            .await;
        if let Err(e) = scenario.run().await {
            panic!("Scenatio \"merge_matching_ids\" failed with the following error: {e}")
        }
    }
    #[allow(unsafe_code, reason = "Repeated acces of manually set `Option`s.")]
    #[tokio::test]
    async fn test_link_ini_ids_merging() {
        let vg = generate::default_vorgang();
        let mut vg_mod = vg.clone();
        vg_mod.links = vec!["https://example.com".to_string()];
        vg_mod.ids = vec![models::VgIdent {
            id: "einzigartig und anders".to_string(),
            typ: "initdrucks".to_owned(),
        }];
        vg_mod.initiatoren = vec![generate::random::autor(0)];

        let mut vg_exp = vg.clone();
        vg_exp.links = []
            .iter()
            .chain(vg.links.iter())
            .chain(vg_mod.links.iter())
            .cloned()
            .collect(); //merged
        vg_exp.links.sort();
        vg_exp.ids = []
            .iter()
            .chain(vg.ids.iter())
            .chain(vg_mod.ids.iter())
            .cloned()
            .collect(); //merged
        vg_exp.ids.sort_by(|a, b| a.id.cmp(&b.id));
        vg_exp.initiatoren = []
            .iter()
            .chain(vg.initiatoren.iter())
            .chain(vg_mod.initiatoren.iter())
            .cloned()
            .collect(); //merged
        vg_exp
            .initiatoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        let scenario = Scenario::builder("link_ini_ids_merging")
            .with_context(vec![vg])
            .with_test_object(vg_mod)
            .with_expectation(vec![vg_to_expected_shape(&vg_exp)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!("Scenatio \"link_ini_ids_merging\" failed with the following error: {e}")
        }
    }
    #[tokio::test]
    async fn test_vorgang_weak_property_change_override() {
        let vg = generate::default_vorgang();
        let mut vg_mod = vg.clone();
        vg_mod.titel = "Testtitel".to_string();
        vg_mod.kurztitel = Some("Testkurztitel".to_string());
        vg_mod.wahlperiode = 20;
        vg_mod.verfassungsaendernd = true;
        let scenario = Scenario::builder("weak_prop_change_override")
            .with_context(vec![vg.clone()])
            .with_test_object(vg_mod.clone())
            .with_expectation(vec![vg_to_expected_shape(&vg_mod)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!("Scenatio \"weak_prop_change_override\" failed with the following error: {e}")
        }
    }
    #[tokio::test]
    async fn test_vorgang_ressort_sachgebiete_merging() {
        let mut vg = generate::default_vorgang();
        vg.ressort = Some(models::enumerations::Ressort::Arbeit);
        vg.sachgebiete = vec![models::enumerations::Sachgebiet::Wahlen];

        let mut vg_mod = vg.clone();
        vg_mod.ressort = Some(models::enumerations::Ressort::Bildung);
        vg_mod.sachgebiete = vec![models::enumerations::Sachgebiet::Parlament];

        let mut vg_exp = vg.clone();
        vg_exp.ressort = vg_mod.ressort; // ressort is overridden by the incoming change
        vg_exp.sachgebiete = []
            .iter()
            .chain(vg.sachgebiete.iter())
            .chain(vg_mod.sachgebiete.iter())
            .copied()
            .collect(); // sachgebiete are merged via union

        let scenario = Scenario::builder("vorgang_ressort_sachgebiete_merging")
            .with_context(vec![vg])
            .with_test_object(vg_mod)
            .with_expectation(vec![vg_to_expected_shape(&vg_exp)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!(
                "Scenatio \"vorgang_ressort_sachgebiete_merging\" failed with the following error: {e}"
            )
        }
    }
    #[tokio::test]
    async fn test_not_merged_but_separate() {
        let vg = generate::default_vorgang();
        let mut vg2 = vg.clone();
        vg2.api_id = uuid::uuid!("b18bee64-c0ff-eeee-ff1c-deadbeef3452");
        vg2.ids = vec![];

        let mut stat = generate::default_station();
        stat.api_id = Some(uuid::uuid!("b18bee64-c0ff-eeee-ff1c-deadbeef4732"));
        stat.typ = models::Stationstyp::PreparlRegent;
        stat.dokumente = vec![];
        vg2.stationen = vec![stat];

        vg2.titel = "Ich Mag Moneten und deshalb ist das ein anderes Gesetz".to_string();
        let scenario = Scenario::builder("not_merged_but_separate")
            .with_context(vec![vg.clone()])
            .with_test_object(vg2.clone())
            .with_expectation(vec![vg_to_expected_shape(&vg), vg_to_expected_shape(&vg2)])
            .build()
            .await;
        if let Err(e) = scenario.run().await {
            panic!("Scenatio \"not_merged_but_separate\" failed with the following error: {e}")
        }
    }
    #[tokio::test]
    async fn test_schlagwort_duplicate_elimination_and_formatting() {
        let mut vg = generate::default_vorgang();
        vg.schlagworte = vec!["AiNz".to_string(), "ainz".to_string(), "AINZ".to_string()];
        let vg2 = vg.clone();

        let mut vg_exp = vg.clone();
        vg_exp.schlagworte = vec!["ainz".to_string()];
        let scenario = Scenario::builder("schlagwort_duplicate_elimination_and_formatting")
            .with_context(vec![vg])
            .with_test_object(vg2)
            .with_expectation(vec![vg_to_expected_shape(&vg_exp)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!(
                "Scenatio \"schlagwort_duplicate_elimination_and_formatting\" failed with the following error: {e}"
            )
        }
    }
    #[tokio::test]
    async fn test_station_merging_on_weak_property_changes() {
        let vg = generate::default_vorgang();
        let mut vg2 = vg.clone();
        vg2.stationen[0].link = Some("https://other.link".to_string());
        vg2.stationen[0].titel = Some("Weirder anderer Titel".to_string());
        vg2.stationen[0].zp_modifiziert = Some(chrono::Utc::now());
        vg2.stationen[0].gremium_federf = Some(true);
        vg2.stationen[0].trojanergefahr = Some(4u8);
        vg2.stationen[0].zp_start = chrono::Utc::now();

        let scenario = Scenario::builder("station_weak_props_change")
            .with_context(vec![vg])
            .with_test_object(vg2.clone())
            .with_expectation(vec![vg_to_expected_shape(&vg2)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!("Scenatio \"station_weak_props_change\" failed with the following error: {e}")
        }
    }
    #[tokio::test]
    async fn test_dokument_merging_on_weak_property_changes() {
        let modified_dokument = models::Dokument {
            api_id: Some(uuid::uuid!("b18bee64-c0ff-ff0c-ff1c-deadbeef4732")),
            titel: "Anderer Titel".to_string(),
            ..generate::default_dokument()
        };
        let modified_stellungnahme = models::Dokument {
            api_id: Some(uuid::uuid!("b18bee64-c0ff-ff1c-ff1c-deadbeef4732")),
            titel: "Anderer Titel für ne Stellungnahme".to_string(),
            ..generate::default_stellungnahme()
        };
        let modified_docs_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                    modified_dokument.clone(),
                ))],
                stellungnahmen: vec![models::DokumentOrApiId::Dokument(Box::new(
                    modified_stellungnahme.clone(),
                ))],
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let expected_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                    models::Dokument {
                        api_id: generate::default_dokument().api_id,
                        ..modified_dokument.clone()
                    },
                ))],
                stellungnahmen: vec![models::DokumentOrApiId::Dokument(Box::new(
                    models::Dokument {
                        api_id: generate::default_stellungnahme().api_id,
                        ..modified_stellungnahme.clone()
                    },
                ))],
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let scenario = Scenario::builder("dokument_merging_on_weak_property_changes")
            .with_context(vec![generate::default_vorgang()])
            .with_test_object(modified_docs_vorgang)
            .with_expectation(vec![vg_to_expected_shape(&expected_vorgang)])
            .build()
            .await;

        if let Err(e) = scenario.run().await {
            panic!(
                "Scenatio \"dokument_merging_on_weak_property_changes\" failed with the following error: {e}"
            )
        }
    }
}
