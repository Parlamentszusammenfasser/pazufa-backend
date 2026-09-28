use super::{PutResponse, RoundTimestamp};
use crate::models::params;
use crate::{PazufaError, PazufaServer, Result, models};
use sqlx::PgTransaction;
use sqlx::SqlSafeStr;
use std::collections::{BTreeMap, BTreeSet};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// this query tries to resolve all potential unique constraint conflicts
// on tables where the enumeration entry are part of a shared unique constraint.
//
// this would mean, if there is a n:m relation table for dokument to autor and values x and y for field autor
// which are to be merged (x is to be made y) this would violate a unique constraint in the table
// thus this query tries to find these and delete entries that are to be the same after the whole transaction
macro_rules! conflict_resolve_query(
    ($table_name:expr, $shorthand:expr, $ident_col:expr, $element_col:expr) => {
        concat!("
WITH lookup(new,old) AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS iv(new, old)) -- this is the vector of all authors to be replaced
-- assumes
-- (1) no circular replacements (to be detected in server code)
-- (2) uniqueness of entries
,
potential_conflicts AS (
-- select from rda rows together with their target aut_id value (either already new or new where aut_id=old) that 
SELECT 
	",$ident_col," as identifier, 
	",$element_col," as original_id, 
	lu.old as old_id,
	lu.new as target_id 
FROM ",$table_name, " ", $shorthand,"
INNER JOIN lookup lu ON 
-- (a) are to be replaced (contain an entry aut_id = old)
lu.old = ",$shorthand,".",$element_col," OR
-- (b) are already a new value (contain an entry aut_id=new)
lu.new = ",$shorthand,".",$element_col,"
),

actual_conflicts AS (
-- select from potential conflicts rows rows are classified by the tuple (other_identifiers, target_aut_id)
SELECT pc.identifier, pc.original_id, pc.target_id FROM potential_conflicts pc
-- and an entry in pc with the same target value and identifying rows and a differing current aut_id exists
WHERE 
EXISTS (
	SELECT 1 FROM potential_conflicts pc2 
	WHERE 
	pc.identifier = pc2.identifier    AND
	pc.target_id = pc2.target_id      AND
	pc.original_id <> pc2.original_id
	)
),

deletion_select AS(-- select all but one from each class denoted by the same identifier / target id
	SELECT * FROM actual_conflicts ac
	WHERE 
	ac.original_id <> (SELECT MIN(original_id) FROM actual_conflicts ac2
	WHERE ac2.identifier = ac.identifier AND ac2.target_id = ac.target_id
	GROUP BY (identifier, target_id))
)

DELETE FROM ",$table_name," ",$shorthand," WHERE 
EXISTS (SELECT FROM deletion_select ds WHERE ds.identifier = ",$shorthand,".",$ident_col," AND ds.original_id = ",$shorthand,".",$element_col,")"
        ) // concat
    } // match arm of macro
); // macro_rules

/// Deletes Autoren from the database given some filtering parameters
/// Does not delete anything with an empty filter to prevent accidents
///
/// # Errors
/// Can fail if database operations fail.
#[instrument(skip_all, fields(query=?query_params))]
pub async fn autoren_delete_by_param(
    server: &PazufaServer,
    query_params: &params::AutorenDeleteByParamQueryParams,
) -> Result<()> {
    let mut tx = server.sqlx_db.begin().await?;
    let n_deleted = sqlx::query!(
        "DELETE FROM autor a WHERE 
            (a.person IS NULL OR a.person = COALESCE($1, a.person)) AND
            a.organisation = COALESCE($2, a.organisation) AND
            (a.fachgebiet IS NULL OR a.fachgebiet = COALESCE($3, a.fachgebiet))",
        query_params.person,
        query_params.org,
        query_params.fach
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    info!(target: "obj", "Successfully deleted {} authors matching psn:{:?} org:{:?} fch:{:?}", 
            n_deleted, query_params.person, query_params.org, query_params.fach);

    info!("Successfully deleted matching authors");
    return Ok(());
}

/// Deletes Gremien from the database given some filtering parameters
/// Does not delete anything with an empty filter to prevent accidents
///
/// # Errors
/// Can fail if database operations fail.
#[instrument(skip_all, fields(query=?query_params))]
pub async fn gremien_delete_by_param(
    server: &PazufaServer,
    query_params: &params::GremienDeleteByParamQueryParams,
) -> Result<()> {
    let mut tx = server.sqlx_db.begin().await?;
    let n_del = sqlx::query!(
        "DELETE FROM gremium g WHERE 
            g.name = COALESCE($1, g.name) AND
            g.wp = COALESCE($2, g.wp) AND
            g.parl = COALESCE((SELECT id FROM parlament p WHERE p.value = $3), g.parl)",
        query_params.gr,
        query_params.wp.map(i32::from),
        query_params.p.as_ref().map(ToString::to_string)
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    info!(target: "obj", "Deleted {} Gremien matching gr:{:?} wp:{:?} pa:{:?}",
        n_del, query_params.gr, query_params.wp, query_params.p.as_ref().map(ToString::to_string)
    );
    info!("Deleted the requested Gremien");
    return Ok(());
}

/// Deletes Enumeration Entries from the database given some filtering parameters
/// Does not delete anything with an empty filter to prevent accidents
///
/// # Errors
/// Can fail if database operations fail.
#[instrument(skip_all, fields(name=?path_params.name))]
pub async fn enum_delete(
    server: &PazufaServer,
    path_params: &params::EnumDeletePathParams,
) -> Result<()> {
    use models::EnumerationNames;

    let mut tx = server.sqlx_db.begin().await?;
    let n_del = match path_params.name {
        EnumerationNames::Schlagworte => {
            sqlx::query!(
                "DELETE FROM schlagwort x WHERE x.value = $1",
                path_params.item
            )
        }
        EnumerationNames::Stationstypen => {
            sqlx::query!(
                "DELETE FROM stationstyp x WHERE x.value = $1",
                path_params.item
            )
        }
        EnumerationNames::Parlamente => {
            sqlx::query!(
                "DELETE FROM parlament x WHERE x.value = $1",
                path_params.item
            )
        }
        EnumerationNames::Vorgangstypen => {
            sqlx::query!(
                "DELETE FROM vorgangstyp x WHERE x.value = $1",
                path_params.item
            )
        }
        EnumerationNames::Dokumententypen => {
            sqlx::query!(
                "DELETE FROM dokumententyp x WHERE x.value = $1",
                path_params.item
            )
        }
        EnumerationNames::Vgidtypen => {
            sqlx::query!(
                "DELETE FROM vg_ident_typ x WHERE x.value = $1",
                path_params.item
            )
        }
    }
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    info!(target: "obj", "Deleted {} Enumeration Entries from {}", 
            n_del, path_params.name);
    info!("Deleted the requested Entries");
    Ok(())
}
async fn count_existing_authors(
    tx: &mut sqlx::PgTransaction<'_>,
    autoren: &[models::Autor],
) -> Result<usize> {
    let (mut person, mut organisation) = (vec![], vec![]);
    for a in autoren {
        person.push(a.person.clone());
        organisation.push(a.organisation.clone());
    }

    let existing_obj_cnt = sqlx::query!(
        "SELECT COUNT(1) as cnt FROM 
        UNNEST($1::text[], $2::text[]) as iv(person, orga)
        WHERE EXISTS (
            SELECT 1 FROM autor a WHERE 
            (a.person IS NULL AND  iv.person IS NULL OR a.person=iv.person) 
            AND a.organisation = iv.orga)",
        &person[..] as &[Option<String>],
        &organisation[..]
    )
    .map(|r| r.cnt)
    .fetch_one(&mut **tx)
    .await?
    .unwrap_or_default();
    usize::try_from(existing_obj_cnt).map_err(|e| PazufaError::Other {
        message: format!("Failed to convert object count to usize: {e}"),
    })
}
async fn count_existing_gremien(
    tx: &mut sqlx::PgTransaction<'_>,
    gremien: &[models::Gremium],
) -> Result<usize> {
    let (mut names, mut pvalues, mut wps, mut links) = (vec![], vec![], vec![], vec![]);
    for gr in gremien {
        names.push(gr.name.clone());
        pvalues.push(gr.parlament.to_string());
        wps.push(i32::from(gr.wahlperiode));
        links.push(gr.link.clone());
    }

    let existing_obj_cnt = sqlx::query_scalar!("SELECT COUNT(1) as cnt FROM 
        UNNEST($1::text[], $2::text[], $3::int4[], $4::text[]) as input_vector(name, pvalue, wp, link)
        WHERE EXISTS (
            SELECT 1 FROM gremium g 
        INNER JOIN parlament p ON g.parl = p.id
        WHERE 
            g.name = input_vector.name AND
            p.value = input_vector.pvalue AND
            g.wp = input_vector.wp AND
            (g.link IS NULL AND input_vector.link IS NULL OR g.link = input_vector.link)
        )
        ", &names[..], &pvalues[..], &wps[..], &links[..] as &[Option<String>])
        .fetch_one(&mut **tx).await?.unwrap_or_default();

    usize::try_from(existing_obj_cnt).map_err(|e| PazufaError::Other {
        message: format!("Failed to convert object count to usize: {e}"),
    })
}
enum PutReplaceValidation {
    NotMod,
    BadReq(String),
    Fine,
}
async fn putreplacing_validation<F, T: utoipa::ToSchema + Clone + PartialEq + Ord>(
    tx: &mut PgTransaction<'_>,
    body: &models::ReplacementPutRequest<T>,
    count_existing: F,
) -> Result<PutReplaceValidation>
where
    F: AsyncFn(&mut PgTransaction<'_>, &[T]) -> Result<usize>,
{
    // if replacing contains an index larger than the object list: Bad Request
    // if replacing contains circular references (meaning a replacing object is identifiable with an object in the object list): Bad Request
    let seen: BTreeSet<&T> = body.objects.iter().collect();
    for rpl in &body.replacing {
        if rpl.replaced_by as usize >= body.objects.len()
            || rpl.values.iter().any(|x| seen.contains(&x))
        {
            let w = "Semantically bad request: Either a circular replacement was detected or 
                        there were more replacement rules than new entries. 
                        An entry must be bound to at most one replacement rule."
                .to_owned();
            warn!("{w}");
            return Ok(PutReplaceValidation::BadReq(w));
        }
    }
    // check if all authors are existent in the database
    // check if none of the replacing authors are in the database
    // if both: NotModified
    if count_existing(tx, &body.objects).await? == body.objects.len() {
        // flatten the replacement objects and check for existence
        let flattened: Vec<T> = body
            .replacing
            .iter()
            .flat_map(|o| o.values.iter())
            .cloned()
            .collect();
        if count_existing(tx, &flattened).await? == 0 {
            info!("All Entries already exist in the database and no replacement entry was found");
            return Ok(PutReplaceValidation::NotMod);
        }
    }
    Ok(PutReplaceValidation::Fine)
}
async fn resolve_conflicts(
    rep_old: &[i32],
    rep_new: &[i32],
    table: &str,
    column: &str,
    resol_query: Option<&str>,
    tx: &mut PgTransaction<'_>,
) -> Result<()> {
    // first, delete potentially conflicting entries
    if let Some(conflict_res_query) = resol_query {
        #[allow(
            clippy::disallowed_methods,
            reason = "Query is huge and sqlx expects a string literal. For reasons of readability,
                this is handled in a macro, which cannot be fed into sqlx::query!"
        )]
        sqlx::query(sqlx::AssertSqlSafe(conflict_res_query).into_sql_str())
            .bind(rep_new)
            .bind(rep_old)
            .execute(&mut **tx)
            .await?;
    }

    // then insert like this:
    let query = format!(
        "WITH lookup AS (SELECT * FROM UNNEST($1::int4[], $2::int4[]) AS la(new, old))
                UPDATE {table} 
                SET {column} = (SELECT new FROM lookup WHERE old={column})
                WHERE {column} = ANY($2::int4[])
            "
    );
    #[allow(
        clippy::disallowed_methods,
        reason = "Query is inherently dynamic and therefore not supported by sqlx compile time checks."
    )]
    sqlx::query(sqlx::AssertSqlSafe(query).into_sql_str())
        .bind(rep_new)
        .bind(rep_old)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
/// Inserts new Autor Entries into the database.
/// Optionally does a PUT-REPLACE, replacing some authors by others and keeping all references in
/// check.
/// # Errors
/// Can fail due to
/// - Database Errors
/// - Validation Errors of the replacement logic
#[instrument(skip_all)]
pub async fn autoren_put(
    server: &PazufaServer,
    body: &models::ReplacementPutRequest<models::Autor>,
) -> Result<PutResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    match putreplacing_validation(&mut tx, body, count_existing_authors).await? {
        PutReplaceValidation::Fine => {}
        PutReplaceValidation::NotMod => return Ok(PutResponse::NotMod),
        PutReplaceValidation::BadReq(s) => return Ok(PutResponse::BadReq(s)),
    }
    debug!("Request was valid");
    // insert all authors, fetch their IDs
    let (mut person, mut organisation, mut fach, mut lobby) = (vec![], vec![], vec![], vec![]);
    for a in &body.objects {
        person.push(&a.person);
        organisation.push(&a.organisation);
        fach.push(&a.fachgebiet);
        lobby.push(&a.lobbyregister);
    }
    let new_ids = sqlx::query!("
        INSERT INTO autor(person, organisation, fachgebiet, lobbyregister) 

        SELECT ps, og, fc, lb FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[]) AS iv(ps, og, fc, lb)

        ON CONFLICT ON CONSTRAINT unq_data 
        DO UPDATE SET 
        fachgebiet = EXCLUDED.fachgebiet,
        lobbyregister = EXCLUDED.lobbyregister

        RETURNING autor.id
        ", &person[..] as &[&Option<String>], &organisation[..] as &[&String], &fach[..] as &[&Option<String>], &lobby[..] as &[&Option<String>])
        .map(|r| r.id)
        .fetch_all(&mut *tx).await?;

    if body.replacing.is_empty() {
        tx.commit().await?;
        warn!(target: "obj", "Inserted Authors into the database with no replacements: {:?}",body.objects );
        // if there is nothing to replace, we are done here
        info!("New authors were introduced into the database");
        warn!("CAREFUL: HEREBY DANGLING AUTHOR ENTRIES CAN BE CREATED");
        return Ok(PutResponse::Success);
    }
    // for each replacing autor:
    // for each table referencing it: Update those tables with the new id
    let mut rep_new: Vec<_> = vec![];
    let mut rep_old: Vec<_> = vec![];
    for entry in &body.replacing {
        let (mut vperson, mut vorga) = (vec![], vec![]);
        for value in &entry.values {
            vperson.push(value.person.clone());
            vorga.push(value.organisation.clone());
        }
        let repl_with = new_ids[entry.replaced_by as usize];
        let value_ids: Vec<_> = sqlx::query_scalar!(
            "SELECT a.id FROM
                    UNNEST($1::text[], $2::text[]) as iv(ps, og)
                    INNER JOIN autor a ON 
                    (a.person IS NULL AND iv.ps IS NULL OR a.person=iv.ps) AND 
                    a.organisation = iv.og",
            &vperson[..] as &[Option<String>],
            &vorga[..],
        )
        .fetch_all(&mut *tx)
        .await?;
        for gid in value_ids {
            rep_new.push(repl_with);
            rep_old.push(gid);
        }
    }

    // tables referencing authors:
    // table in question, column that references the author, query to delete conflicts _if_ the author is part of a unique identifier. can be empty if not applicable
    #[rustfmt::skip]
    let tables = vec![
        ("rel_dok_autor", "aut_id", 
         Some(conflict_resolve_query!("rel_dok_autor", "rda","dok_id","aut_id"))),
        ("rel_vorgang_init", "in_id",
         Some(conflict_resolve_query!("rel_vorgang_init", "rvi", "vg_id", "in_id"))),
        ("rel_sitzung_experten", "eid",
         Some(conflict_resolve_query!("rel_sitzung_experten", "rse", "sid","eid"))),
        ("lobbyregistereintrag", "organisation",
         Some(conflict_resolve_query!( "lobbyregistereintrag", "lre", "vg_id", "organisation"))),
    ];
    for (table, column, conflict_res_query) in tables {
        resolve_conflicts(
            &rep_old,
            &rep_new,
            table,
            column,
            conflict_res_query,
            &mut tx,
        )
        .await?;
    }
    sqlx::query!(
        "DELETE FROM autor a WHERE a.id = ANY($1::int4[])",
        &rep_old[..]
    )
    .execute(&mut *tx)
    .await?;

    // return 201Created
    tx.commit().await?;
    info!("Successful PUT-and-replace was executed");
    info!(target: "obj", "Inserted Authors into the database with: {:?}, replacing: {:?}", body.objects, body.replacing );
    Ok(PutResponse::Success)
}
/// Inserts new Gremium Entries into the database.
/// Optionally does a PUT-REPLACE, replacing some authors by others and keeping all references in
/// check.
/// # Errors
/// Can fail due to
/// - Database Errors
/// - Validation Errors of the replacement logic
#[instrument(skip_all)]
pub async fn gremien_put(
    server: &PazufaServer,
    body: &models::ReplacementPutRequest<models::Gremium>,
) -> Result<PutResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    match putreplacing_validation(&mut tx, body, count_existing_gremien).await? {
        PutReplaceValidation::Fine => {}
        PutReplaceValidation::NotMod => return Ok(PutResponse::NotMod),
        PutReplaceValidation::BadReq(s) => return Ok(PutResponse::BadReq(s)),
    }
    debug!("Request was valid");
    let (mut names, mut pvalues, mut wps, mut links) = (vec![], vec![], vec![], vec![]);
    for gr in &body.objects {
        names.push(&gr.name);
        pvalues.push(gr.parlament.to_string());
        wps.push(i64::from(gr.wahlperiode));
        links.push(&gr.link);
    }
    // insert all gremien, fetch their IDs
    let new_ids = sqlx::query_scalar!("
        INSERT INTO gremium(name, parl, wp, link) 
        
        SELECT nm, p.id, wp, ln FROM UNNEST($1::text[], $2::text[], $3::int8[], $4::text[]) AS iv(nm, pname, wp, ln)
        INNER JOIN parlament p ON p.value = iv.pname

        ON CONFLICT ON CONSTRAINT unique_combo 
        DO UPDATE SET link = EXCLUDED.link

        RETURNING gremium.id
        ", &names[..] as &[&String], &pvalues[..], &wps[..], &links[..] as &[&Option<String>])
        .fetch_all(&mut *tx).await?;

    if body.replacing.is_empty() {
        tx.commit().await?;
        // if there is nothing to replace, we are done here
        warn!(target: "obj", "Inserted Gremien into the database with no replacements: {:?}", body.objects);
        info!("New gremien were introduced into the database");
        warn!("CAREFUL: HEREBY DANGLING GREMIUM ENTRIES CAN BE CREATED");
        return Ok(PutResponse::Success);
    }
    // for each replacing gremium:
    // for each table referencing it: Update those tables with the new id
    let mut replacement_tuples = vec![];
    for entry in &body.replacing {
        let (mut vnames, mut vwps, mut vpvals) = (vec![], vec![], vec![]);
        for value in &entry.values {
            vnames.push(value.name.clone());
            vwps.push(i32::from(value.wahlperiode));
            vpvals.push(value.parlament.to_string());
        }
        let repl_with = new_ids[entry.replaced_by as usize];
        let value_ids: Vec<i32> = sqlx::query_scalar!(
            "SELECT g.id FROM
                    UNNEST($1::text[], $2::text[], $3::int4[]) as iv(nm, pv, wp)
                    INNER JOIN parlament p ON p.value = iv.pv
                    INNER JOIN gremium g ON 
                    g.name=iv.nm AND g.parl = p.id AND g.wp=iv.wp",
            &vnames[..],
            &vpvals[..],
            &vwps[..],
        )
        .fetch_all(&mut *tx)
        .await?;
        replacement_tuples.extend(value_ids.into_iter().map(|gid| (repl_with, gid)));
    }
    let rep_new: Vec<_> = replacement_tuples.iter().map(|x| x.0).collect();
    let rep_old: Vec<_> = replacement_tuples.iter().map(|x| x.1).collect();
    // tables that reference a gremium:
    // - station(gr_id)
    // - sitzung(gr_id)
    let tables = vec![("station", "gr_id", None), ("sitzung", "gr_id", None)];
    for (table, column, conflict_resolution_query) in tables {
        resolve_conflicts(
            &rep_old,
            &rep_new,
            table,
            column,
            conflict_resolution_query,
            &mut tx,
        )
        .await?;
    }
    sqlx::query!(
        "DELETE FROM gremium g WHERE g.id = ANY($1::int4[])",
        &rep_old[..]
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    info!("Successful PUT-and-replace was executed");
    info!(target: "obj", "Inserted Gremien into the database with: {:?}, replacing: {:?}", body.objects, body.replacing );
    Ok(PutResponse::Success)
}
async fn enum_put_preliminary_checks(
    path_params: &params::EnumPathParams,
    body: &models::ReplacementPutRequest<String>,
    enum_tables: &BTreeMap<models::EnumerationNames, &str>,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<PutResponse> {
    // check if replacing contain an index larger than the object list
    // if so: Bad Request
    for rpl in &body.replacing {
        if rpl.replaced_by as usize >= body.objects.len() {
            return Ok(PutResponse::BadReq(
                "Length of .replaced_by > objects that can be replaced".to_owned(),
            ));
        }
    }

    // check if all gremien are existent in the database
    // check if none of the replacing gremien are in the database or replacing is None
    // if both: NotModified
    #[allow(
        clippy::disallowed_methods,
        reason = "Sqlx does not support dynamic table names for compile time checks."
    )]
    let present: i64 = sqlx::query_scalar(
        sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(1) as cnt FROM UNNEST($1::text[]) as item 
                    WHERE EXISTS(SELECT 1 FROM {} x WHERE item=x.value)",
            enum_tables[&path_params.name]
        ))
        .into_sql_str(),
    )
    .bind(&body.objects[..])
    .fetch_one(&mut **tx)
    .await?;
    let present = usize::try_from(present).map_err(|e| PazufaError::Other {
        message: format!("Error converting expected object count: {e}"),
    })?;

    if present == body.objects.len() {
        // flatten the replacement objects and check for existence
        let flattened: Vec<String> = body
            .replacing
            .iter()
            .flat_map(|o| o.values.iter())
            .cloned()
            .collect();

        #[allow(
            clippy::disallowed_methods,
            reason = "Sqlx does not support dynamic table names for compile time checks."
        )]
        let present: i64 = sqlx::query_scalar(
            sqlx::AssertSqlSafe(format!(
                "SELECT COUNT(1) FROM UNNEST($1::text[]) as item 
                WHERE EXISTS(SELECT 1 FROM {} x WHERE item=x.value)",
                enum_tables[&path_params.name]
            ))
            .into_sql_str(),
        )
        .bind(&flattened[..])
        .fetch_one(&mut **tx)
        .await?;

        if present == 0 {
            return Ok(PutResponse::NotMod);
        }
    }
    Ok(PutResponse::Success)
}
async fn enum_put_conflict_cleanup(
    new_enum_ids: Vec<i32>,
    path_params: &params::EnumPathParams,
    body: &models::ReplacementPutRequest<String>,
    enum_tables: &BTreeMap<models::EnumerationNames, &str>,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<PutResponse> {
    // in that order:
    // - table to replace in,
    // - column name to replace,
    // - conflict resolution query if necessary
    // if No conflict resolution query is passed alongside, the values in `column` are just
    // replaced 1:1 with the new ones.
    // If there is a conflict resolution query to be run, it is run before the replacements
    //
    //
    // Conflict resolution queries are there to swap out values that are part of a database key.
    type ReplacementTuple = (&'static str, &'static str, Option<&'static str>);

    // for each replacing:
    // for each table referencing it: Update those tables with the new id
    let mut replacement_tuples = vec![];
    for entry in &body.replacing {
        // first, delete potentially conflicting entries
        // !!this is a TODO!! (but its like, ten years later and i have no idea what i meant

        // then insert like this:
        let vitems: Vec<String> = entry.values.clone();
        let repl_with = new_enum_ids[entry.replaced_by as usize];
        #[allow(
            clippy::disallowed_methods,
            reason = "Sqlx does not support dynamic table names for compile time checks."
        )]
        let value_ids: Vec<i32> = sqlx::query_scalar(
            sqlx::AssertSqlSafe(format!(
                "SELECT x.id  FROM
                    UNNEST($1::text[]) as item
                    INNER JOIN {} x ON x.value = item",
                enum_tables[&path_params.name]
            ))
            .into_sql_str(),
        )
        .bind(&vitems[..])
        .fetch_all(&mut **tx)
        .await?;
        replacement_tuples.extend(value_ids.into_iter().map(|gid| (repl_with, gid)));
    }
    let rep_new: Vec<_> = replacement_tuples.iter().map(|x| x.0).collect();
    let rep_old: Vec<_> = replacement_tuples.iter().map(|x| x.1).collect();

    // referencing tables:
    // parlament: gremium(parl)
    // dokumententyp: dokument(typ)
    // stationstyp: station(typ)
    // vg_ident_typ: rel_vorgang_ident(typ)
    // vorgangstyp: vorgang(typ)
    // schlagwort: rel_vorgang_schlagwort(sw_id) / rel_dok_schlagwort(sw_id)
    #[rustfmt::skip]
    let mut enum_table_refs: BTreeMap<models::EnumerationNames, BTreeSet<ReplacementTuple>> = BTreeMap::new();
    enum_table_refs.insert(
        models::EnumerationNames::Parlamente,
        [("gremium", "parl", None)].into_iter().collect(), // not a key component
    );
    enum_table_refs.insert(
        models::EnumerationNames::Dokumententypen,
        [("dokument", "typ", None)].into_iter().collect(), // not a key component
    );
    enum_table_refs.insert(
        models::EnumerationNames::Stationstypen,
        [("station", "typ", None)].into_iter().collect(), // not a key component
    );
    #[rustfmt::skip]
    enum_table_refs.insert(
        models::EnumerationNames::Vgidtypen,
        [(
            "rel_vorgang_ident", "typ",
            Some(conflict_resolve_query!("rel_vorgang_ident", "rvi", "vg_id", "typ")),
        )]
        .into_iter()
        .collect(),
    );
    enum_table_refs.insert(
        models::EnumerationNames::Vorgangstypen,
        [("vorgang", "typ", Some(""))].into_iter().collect(), // not a key component
    );
    #[rustfmt::skip]
    enum_table_refs.insert(
        models::EnumerationNames::Schlagworte,
        // a key component, a key component
        [("rel_dok_schlagwort", "sw_id",
          Some(conflict_resolve_query!("rel_dok_schlagwort","rds","dok_id", "sw_id")),
        ),
        ("rel_vorgang_schlagwort", "sw_id",
         Some(conflict_resolve_query!( "rel_vorgang_schlagwort", "rvs", "vg_id", "sw_id")),
        ),
        ]
        .into_iter()
        .collect(),
    );
    for (table, column, conflict_resolution_query) in &enum_table_refs[&path_params.name] {
        resolve_conflicts(
            &rep_old,
            &rep_new,
            table,
            column,
            *conflict_resolution_query,
            tx,
        )
        .await?;
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "Sqlx does not support dynamic table names for compile time checks."
    )]
    sqlx::query(
        sqlx::AssertSqlSafe(format!(
            "DELETE FROM {} x WHERE x.id = ANY($1::int4[])",
            enum_tables[&path_params.name]
        ))
        .into_sql_str(),
    )
    .bind(&rep_old[..])
    .execute(&mut **tx)
    .await?;
    Ok(PutResponse::Success)
}
/// Inserts new Enumeration Entries into the database.
/// Optionally does a PUT-REPLACE, replacing some enumeration entries by others and keeping all references in
/// check.
/// # Errors
/// Can fail due to
/// - Database Errors
/// - Validation Errors of the replacement logic
#[instrument(skip_all, fields(name=%path_params.name))]
pub async fn enum_put(
    server: &PazufaServer,
    path_params: &params::EnumPathParams,
    body: &models::ReplacementPutRequest<String>,
) -> Result<PutResponse> {
    tracing::info!("{:?}", path_params);

    let mut tx = server.sqlx_db.begin().await?;

    let enum_tables: BTreeMap<models::EnumerationNames, &str> = FromIterator::from_iter(
        vec![
            (models::EnumerationNames::Schlagworte, "schlagwort"),
            (models::EnumerationNames::Stationstypen, "stationstyp"),
            (models::EnumerationNames::Parlamente, "parlament"),
            (models::EnumerationNames::Vorgangstypen, "vorgangstyp"),
            (models::EnumerationNames::Dokumententypen, "dokumententyp"),
            (models::EnumerationNames::Vgidtypen, "vg_ident_typ"),
        ]
        .drain(..),
    );
    match enum_put_preliminary_checks(path_params, body, &enum_tables, &mut tx).await? {
        PutResponse::Success => {}
        x => return Ok(x),
    }

    // insert all enum entries, fetch their IDs
    #[allow(
        clippy::disallowed_methods,
        reason = "Sqlx does not support dynamic table names for compile time checks."
    )]
    let new_ids: Vec<i32> = sqlx::query_scalar(
        sqlx::AssertSqlSafe(format!(
            "INSERT INTO {} (value)
            SELECT item FROM UNNEST($1::text[]) as item 
            ON CONFLICT(value) DO UPDATE SET value=EXCLUDED.value
            RETURNING id",
            enum_tables[&path_params.name]
        ))
        .into_sql_str(),
    )
    .bind(&body.objects[..])
    .fetch_all(&mut *tx)
    .await?;

    if body.replacing.is_empty() {
        tx.commit().await?;
        // if there is nothing to replace, we are done here
        // CAREFUL: HERE DANGLING ENUM ENTRIES ARE CREATED
        warn!(target: "obj", "Inserted Enumeration Entries into the database with no replacements: {:?}", body.objects);
        warn!(
            "Inserted Enumeration Entries into db without replacements, these are dangling as of now"
        );
        return Ok(PutResponse::Success);
    }
    let thingy =
        enum_put_conflict_cleanup(new_ids, path_params, body, &enum_tables, &mut tx).await?;

    // return 201Created
    tx.commit().await?;
    info!(target: "obj", "Inserted Enum into the database with: {:?}, replacing: {:?}", body.objects, body.replacing );
    Ok(thingy)
}

/// Inserts a document by its supplied `api_id`
/// only for administrative purposes, can create dangling references
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(dok=%path_params.api_id))]
pub async fn dokument_put_id(
    server: &PazufaServer,
    creator_keyidx: i32,
    path_params: &params::ApiIdPathParam,
    body: &models::Dokument,
) -> Result<PutResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    let did = sqlx::query!(
        "SELECT id FROM dokument WHERE api_id = $1",
        path_params.api_id
    )
    .map(|r| r.id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(did) = did {
        let dok = crate::db::retrieve::dokument_by_id(did, &mut tx).await?;
        if dok.with_round_timestamps() == body.with_round_timestamps() {
            info!("Dokument was not modified");
            return Ok(PutResponse::NotMod);
        }
        sqlx::query!("DELETE FROM dokument WHERE api_id = $1", path_params.api_id)
            .execute(&mut *tx)
            .await?;
    }
    let id = crate::db::insert::insert_dokument(body, Uuid::nil(), creator_keyidx, &mut tx, server)
        .await?;
    let api_id = sqlx::query!("SELECT api_id FROM dokument WHERE id= $1", id)
        .map(|r| r.api_id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;
    info!(target: "obj", "PUT Dokument {}", api_id);
    info!("Created or updated successfully");
    return Ok(PutResponse::Success);
}

#[cfg(test)]
mod test_authorisierte_sonstige {
    use super::super::{PazufaServer, PutResponse};
    use super::{autoren_delete_by_param, enum_delete, gremien_delete_by_param};
    use crate::api::vorgang;
    use crate::api::{ListingGetResponse, misc};
    use crate::db::merge::execute::run_integration;
    use crate::models::{self, DokumentOrApiId, EnumerationNames, params};
    use crate::utils::testing::{TestSetup, generate};
    use tracing_test::traced_test;

    async fn insert_default_vorgang(server: &PazufaServer) {
        let vorgang = generate::default_vorgang();
        vorgang::vorgang_id_put(
            server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
    }
    async fn fetch_all_authors(server: &PazufaServer) -> Vec<models::Autor> {
        let autoren = misc::autoren_get(
            server,
            &params::AutorenGetQueryParams {
                page: None,
                per_page: None,
                fach: None,
                org: None,
                person: None,
            },
        )
        .await
        .expect("Failed to get Autoren.");
        match autoren {
            ListingGetResponse::Success { body, .. } => body,
            ListingGetResponse::NoContent => vec![],
            _ => panic!("Error"),
        }
    }
    async fn fetch_all_gremien(server: &PazufaServer) -> Vec<models::Gremium> {
        let gremien = misc::gremien_get(
            server,
            &params::GremienGetQueryParams {
                page: None,
                per_page: None,
                gr: None,
                p: None,
                wp: None,
            },
        )
        .await
        .expect("Failed to get Gremien.");
        match gremien {
            ListingGetResponse::Success { body, .. } => body,
            ListingGetResponse::NoContent => vec![],
            _ => panic!("Error"),
        }
    }
    async fn fetch_all_enumvars(server: &PazufaServer, name: EnumerationNames) -> Vec<String> {
        let entries = misc::enum_get(
            server,
            &params::EnumPathParams { name },
            &params::EnumGetQueryParams {
                page: None,
                per_page: None,
                contains: None,
            },
        )
        .await
        .expect("Failed to get Enum.");
        match entries {
            ListingGetResponse::Success { body, .. } => body,
            _ => vec![],
        }
    }
    #[tokio::test]
    async fn test_autor_delete() {
        let scenario = TestSetup::new("test_autor_delete").await;
        insert_default_vorgang(&scenario.server).await;
        let autoren = fetch_all_authors(&scenario.server).await;
        autoren_delete_by_param(
            &scenario.server,
            &params::AutorenDeleteByParamQueryParams {
                fach: None,
                org: Some("Mysterium der Ministerien".to_string()),
                person: None,
            },
        )
        .await
        .expect("Failed to delete Autor.");
        let autoren_now = fetch_all_authors(&scenario.server).await;
        assert!(
            autoren.len() > autoren_now.len(),
            "Expected: {autoren:?}, Got {autoren_now:?}"
        );
        let autoren = autoren_now;
        autoren_delete_by_param(
            &scenario.server,
            &params::AutorenDeleteByParamQueryParams {
                fach: None,
                org: None,
                person: Some("Harald Maria Töpfer".to_string()),
            },
        )
        .await
        .expect("Failed to delete Autor.");
        let autoren_now = fetch_all_authors(&scenario.server).await;
        assert!(autoren.len() > autoren_now.len());

        scenario.teardown().await;
    }

    async fn enum_delete_with(
        server: &PazufaServer,
        pp: &params::EnumDeletePathParams,
    ) -> crate::Result<()> {
        enum_delete(server, pp).await
    }
    #[tokio::test]
    async fn test_enum_delete() {
        let scenario = TestSetup::new("test_enum_delete").await;
        insert_default_vorgang(&scenario.server).await;

        enum_delete_with(
            &scenario.server,
            &params::EnumDeletePathParams {
                item: "preparl-entwurf".to_string(),
                name: models::EnumerationNames::Dokumententypen,
            },
        )
        .await
        .expect("Failed to delete Enum.");

        scenario.teardown().await;
    }

    #[traced_test]
    #[tokio::test]
    async fn test_gremien_delete() {
        let scenario = TestSetup::new("test_gremien_delete").await;
        let mut vorgang = generate::default_vorgang();
        let std_station = generate::default_station();
        vorgang.stationen.push(models::Station {
            api_id: Some(uuid::uuid!("b18bde64-c0ff-eeee-bbbb-deadbeef106e")),
            gremium: models::Gremium {
                link: None,
                name: "rrrrrr".to_string(),
                parlament: models::Parlament::Bt,
                wahlperiode: 12,
            },
            typ: models::Stationstyp::ParlVollvlsgn,
            ..std_station.clone()
        });

        vorgang::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        // and now retrival and checks
        let gremien = fetch_all_gremien(&scenario.server).await;
        gremien_delete_by_param(
            &scenario.server,
            &params::GremienDeleteByParamQueryParams {
                gr: Some("rrrrrr".to_string()),
                p: None,
                wp: None,
            },
        )
        .await
        .expect("Failed to delete Gremium.");
        let gremien_after_delete = fetch_all_gremien(&scenario.server).await;
        assert!(
            gremien.len() == 2 && gremien_after_delete.len() == 1,
            "vorher {gremien:?} nachher {gremien_after_delete:?}"
        );
        let gremien = gremien_after_delete;

        gremien_delete_by_param(
            &scenario.server,
            &params::GremienDeleteByParamQueryParams {
                gr: None,
                p: Some(models::Parlament::Bt),
                wp: None,
            },
        )
        .await
        .expect("Failed to delete Gremium.");

        let gremien_after_delete = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() == 1 && gremien_after_delete.is_empty());
        gremien_delete_by_param(
            &scenario.server,
            &params::GremienDeleteByParamQueryParams {
                gr: None,
                p: None,
                wp: Some(20),
            },
        )
        .await
        .expect("Failed to delete Gremium.");

        let new_gremien = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() > new_gremien.len());
        scenario.teardown().await;
    }

    #[tokio::test]
    pub async fn test_gremium_put() {
        let scenario = TestSetup::new("test_gremium_put").await;
        insert_default_vorgang(&scenario.server).await;

        let other_gremium = models::Gremium {
            link: None,
            name: "Ausschuss für Ware Diggah".to_string(),
            parlament: models::Parlament::Bv,
            wahlperiode: 42,
        };
        // check insert without conflict
        let gremien = fetch_all_gremien(&scenario.server).await;
        let response = super::gremien_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![other_gremium.clone()],
                replacing: vec![],
            },
        )
        .await
        .expect("Failed to create Gremium.");
        assert!(matches!(response, PutResponse::Success));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert!(gremien.len() < gremien_new.len());
        assert!(gremien_new.contains(&other_gremium));
        let gremien = gremien_new;

        // check insert with conflict
        let response = super::gremien_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![other_gremium.clone()],
                replacing: vec![],
            },
        )
        .await
        .expect("Failed to create Gremium.");
        assert!(matches!(response, PutResponse::NotMod));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());
        let gremien = gremien_new;

        // check replace
        let repl_grm = models::Gremium {
            link: None,
            name: "Ausschuss für Ware Diggah2".to_string(),
            parlament: models::Parlament::Bv,
            wahlperiode: 42,
        };
        let response = super::gremien_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![repl_grm.clone()],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 0,
                    values: vec![other_gremium.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Gremium.");
        assert!(matches!(response, PutResponse::Success));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());
        assert!(gremien_new.contains(&repl_grm));

        // malformed request
        let response = super::gremien_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![models::Gremium {
                    link: None,
                    name: "Ausschuss für Ware Diggah2".to_string(),
                    parlament: models::Parlament::Bv,
                    wahlperiode: 42,
                }],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 1,
                    values: vec![other_gremium.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Gremium.");
        assert!(matches!(response, PutResponse::BadReq(_)));
        let gremien_new = fetch_all_gremien(&scenario.server).await;
        assert_eq!(gremien.len(), gremien_new.len());

        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_autor_put_replace() {
        let scenario = TestSetup::new("test_autor_put_replace").await;
        insert_default_vorgang(&scenario.server).await;
        let other_autor = models::Autor {
            fachgebiet: Some("Blattzerfetzung".to_string()),
            lobbyregister: Some("https://example.com/einzigartig".to_string()),
            person: Some("Thorbjörn Alman".to_string()),
            organisation: "Schmiedeversammlung Süd".to_string(),
        };
        let _ = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![other_autor.clone()],
                replacing: vec![],
            },
        )
        .await
        .expect("Failed to create Autor.");
        let autoren = fetch_all_authors(&scenario.server).await;
        // check replace
        let repl_aut = models::Autor {
            fachgebiet: Some("Blattzusammensetzung".to_string()),
            lobbyregister: Some("https://example.com/einzigartig/hahadochnicht".to_string()),
            person: Some("Karla Kolumna".to_string()),
            organisation: "Wasserstoffwirtschaftsverband der Ostgoten".to_string(),
        };
        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![repl_aut.clone()],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 0,
                    values: vec![other_autor.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(response, PutResponse::Success));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), autoren_new.len());
        assert!(autoren_new.contains(&repl_aut));
        assert!(!autoren_new.contains(&other_autor));

        // malformed request in replace
        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![repl_aut.clone()],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 1,
                    values: vec![other_autor.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(response, PutResponse::BadReq(_)));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), autoren_new.len());

        // circular reference
        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![repl_aut.clone()],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 0,
                    values: vec![repl_aut.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(response, PutResponse::BadReq(_)));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), autoren_new.len());

        // test case of merging two foreign keys: currently disabled !!THIS IS A TODO!!
        let mod_autor = models::Autor {
            person: Some("Heribert Schnakenwurst IV".to_string()),
            ..generate::default_autor_person()
        };
        let mut mod_stln = generate::default_dokument();
        mod_stln.autoren.push(mod_autor.clone());

        let mut modified_default = generate::default_vorgang();
        modified_default.stationen[0].stellungnahmen[0] =
            DokumentOrApiId::Dokument(Box::new(mod_stln));

        run_integration(&modified_default, uuid::Uuid::nil(), 1, &scenario.server)
            .await
            .expect("Integration failed."); // insert one that can be merged
        let all_authors = fetch_all_authors(&scenario.server).await;
        assert!(all_authors.contains(&mod_autor));

        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![generate::default_autor_person()],
                replacing: vec![models::ReplacementPutRequestInner {
                    replaced_by: 0,
                    values: vec![mod_autor.clone()],
                }],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(&response, PutResponse::Success), "{response:?}");
        assert!(fetch_all_authors(&scenario.server).await.len() < all_authors.len());
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_autor_put_insert() {
        let scenario = TestSetup::new("test_autor_put_insert").await;
        insert_default_vorgang(&scenario.server).await;

        let other_autor = models::Autor {
            fachgebiet: Some("Blattzerfetzung".to_string()),
            lobbyregister: Some("https://example.com/einzigartig".to_string()),
            person: Some("Thorbjörn Alman".to_string()),
            organisation: "Schmiedeversammlung Süd".to_string(),
        };
        // check insert without conflict
        let autoren = fetch_all_authors(&scenario.server).await;
        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![other_autor.clone()],
                replacing: vec![],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(response, PutResponse::Success));
        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert!(autoren.len() < autoren_new.len());
        assert!(autoren_new.contains(&other_autor));
        let autoren = autoren_new;

        // check insert with conflict
        let response = super::autoren_put(
            &scenario.server,
            &models::ReplacementPutRequest {
                objects: vec![other_autor.clone()],
                replacing: vec![],
            },
        )
        .await
        .expect("Failed to create Autor.");
        assert!(matches!(response, PutResponse::NotMod));

        let autoren_new = fetch_all_authors(&scenario.server).await;
        assert_eq!(autoren.len(), autoren_new.len());

        scenario.teardown().await;
    }

    async fn ep_with(
        server: &PazufaServer,
        tp: models::EnumerationNames,
        body: &models::ReplacementPutRequest<String>,
    ) -> crate::Result<PutResponse> {
        super::enum_put(server, &params::EnumPathParams { name: tp }, body).await
    }

    #[tokio::test]
    async fn test_enum_put() {
        let scenario = TestSetup::new("test_enum_put").await;
        insert_default_vorgang(&scenario.server).await;
        #[rustfmt::skip]
        let testcases = [
            (models::EnumerationNames::Parlamente,
                "EP".to_string(),
                "ER".to_string()),
            (models::EnumerationNames::Dokumententypen,
                "traktat".to_string(),
                "encyclika".to_string()),
            (models::EnumerationNames::Vorgangstypen,
                "Verdauung".to_string(),
                "Rohrreinigung".to_string()),
            (models::EnumerationNames::Schlagworte,
                "flüssiggasterminal".to_string(),
                "rühreihöchstmenge".to_string()),
            (models::EnumerationNames::Vgidtypen,
                "anschrift".to_string(),
                "hausnummer".to_string()),
            (models::EnumerationNames::Stationstypen,
                "hauptbahnhof".to_string(),
                "haltestelle".to_string()),
        ];
        for (tp, new_entry, other_new_entry) in &testcases {
            let entries = fetch_all_enumvars(&scenario.server, *tp).await;
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::ReplacementPutRequest {
                    objects: vec![new_entry.clone()],
                    replacing: vec![],
                },
            )
            .await
            .expect("Failed to create Enum.");
            assert!(matches!(response, PutResponse::Success));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert!(entries.len() < entries_new.len());
            assert!(entries_new.contains(new_entry));
            let entries = entries_new;

            // with conflict
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::ReplacementPutRequest {
                    objects: vec![new_entry.clone()],
                    replacing: vec![],
                },
            )
            .await
            .expect("Failed to create Enum.");
            assert!(matches!(response, PutResponse::NotMod));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
            let entries = entries_new;

            // check replace
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::ReplacementPutRequest {
                    objects: vec![other_new_entry.clone()],
                    replacing: vec![models::ReplacementPutRequestInner {
                        replaced_by: 0,
                        values: vec![new_entry.clone()],
                    }],
                },
            )
            .await
            .unwrap_or_else(|x| {
                panic!("On test case {tp} // {new_entry} // {other_new_entry} with error {x}")
            });
            assert!(matches!(response, PutResponse::Success));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
            assert!(entries_new.contains(other_new_entry));
            assert!(!entries_new.contains(new_entry));

            // malformed request
            let response = ep_with(
                &scenario.server,
                *tp,
                &models::ReplacementPutRequest {
                    objects: vec![other_new_entry.clone()],
                    replacing: vec![models::ReplacementPutRequestInner {
                        replaced_by: 1,
                        values: vec![new_entry.clone()],
                    }],
                },
            )
            .await
            .expect("Failed to create Enum.");
            assert!(matches!(response, PutResponse::BadReq(_)));
            let entries_new = fetch_all_enumvars(&scenario.server, *tp).await;
            assert_eq!(entries.len(), entries_new.len());
        }

        scenario.teardown().await;
    }
}
