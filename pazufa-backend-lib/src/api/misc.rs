use super::PaginationResponsePart;
use crate::api::{GetSingleResponse, ListingGetResponse};
use crate::db::retrieve;
use crate::models::enumerations::DokumentFormat;
use crate::models::params::{self, ListingQueryParams};
use crate::{PazufaError, PazufaServer, Result, models};
use sqlx::Row;
use sqlx::SqlSafeStr;
use std::collections::BTreeMap;
use std::str::FromStr;
use tracing::{info, instrument};

/// GETs all Dokument Objects given the filtering parameters
/// # Errors
/// Can fail due to database errors or Pagination errors
pub async fn dokumente_get_by_params(
    server: &PazufaServer,
    _format: DokumentFormat,
    lp: ListingQueryParams,
    ims: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ListingGetResponse<models::Dokument>> {
    let mut tx = server.sqlx_db.begin().await?;

    let (prp, doks) = retrieve::get_dokument_by_parameter(
        retrieve::DokumentRetrivalParameters {
            page: lp.page,
            per_page: lp.per_page,
            since: lp.since,
            until: lp.until,
            if_mod_since: ims,
        },
        &mut tx,
    )
    .await?;

    if doks.is_empty() {
        return Ok(ListingGetResponse::NoContent);
    }
    if let Some(p) = lp.page
        && p > prp.x_page
    {
        return Ok(ListingGetResponse::NoContent);
    }

    Ok(ListingGetResponse::Success {
        body: doks,
        prp,
        path_prefix: "/api/v2/dokumente",
    })
}

/// GETs all Authors given the filtering parameters
/// # Errors
/// Can fail due to database errors or Pagination errors
#[instrument(skip_all, fields(query=?query_params))]
pub async fn autoren_get(
    server: &PazufaServer,
    query_params: &params::AutorenGetQueryParams,
) -> Result<ListingGetResponse<models::Autor>> {
    let mut tx = server.sqlx_db.begin().await?;
    let result = sqlx::query!(
        "SELECT a.id FROM autor a WHERE
            ($1::text IS NULL AND person IS NULL OR person ILIKE CONCAT('%',$1,'%')) AND
            organisation ILIKE CONCAT('%',$2::text,'%') AND
            ($3::text IS NULL AND fachgebiet IS NULL OR fachgebiet ILIKE CONCAT('%', $3, '%'))",
        query_params.person,
        query_params.org,
        query_params.fach,
    )
    .map(|r| r.id)
    .fetch_all(&mut *tx)
    .await?;

    let prp = PaginationResponsePart::new(
        u32::try_from(result.len()).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert result length: {e}"),
        })?,
        query_params.page,
        query_params.per_page,
    );
    if let Some(p) = query_params.page
        && p > prp.x_page
    {
        return Ok(ListingGetResponse::NoContent);
    }
    if result.is_empty() {
        return Ok(ListingGetResponse::NoContent);
    }
    let result = &result[prp.start()..prp.end()];
    let output = sqlx::query!(
        "SELECT * FROM autor WHERE id = ANY($1::int4[])",
        &result[..]
    )
    .map(|r| models::Autor {
        fachgebiet: r.fachgebiet,
        lobbyregister: r.lobbyregister,
        organisation: r.organisation,
        person: r.person,
    })
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    if output.is_empty() {
        info!("No matching Authors found");
        return Ok(ListingGetResponse::NoContent);
    }
    info!("{} Authors found and returned", output.len());
    return Ok(ListingGetResponse::Success {
        body: output,
        prp,
        path_prefix: "/api/v2/autoren",
    });
}

/// GETs all Gremien given the filtering parameters
/// # Errors
/// Can fail due to database errors or Pagination errors
#[instrument(skip_all, fields(query=?query_params))]
pub async fn gremien_get(
    server: &PazufaServer,
    query_params: &params::GremienGetQueryParams,
) -> Result<ListingGetResponse<models::Gremium>> {
    let mut tx = server.sqlx_db.begin().await?;
    let mut result = sqlx::query!(
        "SELECT g.id FROM gremium g
            INNER JOIN parlament p ON p.id = g.parl 
            WHERE p.value = COALESCE($1, p.value) AND
            g.wp = COALESCE($2, g.wp) AND
            ($3::text IS NULL OR g.name ILIKE CONCAT('%',$3,'%'))",
        query_params.p.map(|x| x.to_string()),
        query_params.wp.map(i32::from),
        query_params.gr
    )
    .map(|r| r.id)
    .fetch_all(&mut *tx)
    .await?;
    if result.is_empty() {
        info!("No matching Gremium found");
        return Ok(ListingGetResponse::NoContent);
    }
    let prp = PaginationResponsePart::new(
        u32::try_from(result.len()).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert result length: {e}"),
        })?,
        query_params.page,
        query_params.per_page,
    );
    if let Some(p) = query_params.page
        && p > prp.x_page
    {
        return Ok(ListingGetResponse::NoContent);
    }
    if result.is_empty() {
        return Ok(ListingGetResponse::NoContent);
    }
    let selected_ids: Vec<i32> = result.drain(prp.start()..prp.end()).collect();
    let result = sqlx::query!(
        "SELECT g.link, g.name, g.wp, p.value as parl FROM gremium g
            INNER JOIN parlament p ON p.id = g.parl
            WHERE g.id = ANY($1::int4[])",
        &selected_ids[..]
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| {
        Ok(models::Gremium {
            link: r.link,
            name: r.name,
            parlament: models::Parlament::from_str(&r.parl)
                .map_err(|e| PazufaError::Other { message: e })?,
            wahlperiode: u16::try_from(r.wp).map_err(|e| PazufaError::Other {
                message: format!("Failed to convert wahlperiode: {e}"),
            })?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    info!("{} matching Gremien found and returned", result.len());
    Ok(ListingGetResponse::Success {
        body: result,
        prp,
        path_prefix: "/api/v2/gremien",
    })
}

/// GETs all Enumeration entries given the filtering parameters and the name of the enumeration
/// # Errors
/// Can fail due to database errors or Pagination errors
#[instrument(skip_all, fields(enum=%path_params.name, contains=?query_params.contains, page=?query_params.page, per_page=?query_params.per_page))]
pub async fn enum_get(
    server: &PazufaServer,
    path_params: &params::EnumPathParams,
    query_params: &params::EnumGetQueryParams,
) -> Result<ListingGetResponse<String>> {
    let contains = query_params
        .contains
        .as_ref()
        .map_or_else(String::new, |x| {
            if path_params.name == models::EnumerationNames::Parlamente {
                x.to_uppercase()
            } else {
                x.to_lowercase()
            }
        });
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
    #[allow(
        clippy::disallowed_methods,
        reason = "Sqlx does not support dynamic table names for compile time checks."
    )]
    let mut filtered_ids = sqlx::query(
        sqlx::AssertSqlSafe(format!(
            "SELECT v.id FROM {} v WHERE v.value ILIKE CONCAT('%',$1::text,'%')",
            enum_tables[&path_params.name]
        ))
        .into_sql_str(),
    )
    .bind::<_>(contains)
    .map(|r| r.get(0))
    .fetch_all(&mut *tx)
    .await?;

    let prp = PaginationResponsePart::new(
        u32::try_from(filtered_ids.len()).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert result length: {e}"),
        })?,
        query_params.page,
        query_params.per_page,
    );
    if let Some(p) = query_params.page
        && p > prp.x_page
    {
        return Ok(ListingGetResponse::NoContent);
    }
    if filtered_ids.is_empty() {
        info!("No matching Enumeration entry found");
        return Ok(ListingGetResponse::NoContent);
    }
    let select_few: Vec<i32> = filtered_ids.drain(prp.start()..prp.end()).collect();
    #[allow(
        clippy::disallowed_methods,
        reason = "Sqlx does not support dynamic table names for compile time checks."
    )]
    let values: Vec<String> = sqlx::query(
        sqlx::AssertSqlSafe(format!(
            "SELECT v.value FROM {} v WHERE v.id = ANY($1::int4[])",
            enum_tables[&path_params.name]
        ))
        .into_sql_str(),
    )
    .bind::<_>(select_few)
    .map(|r| r.get(0))
    .fetch_all(&mut *tx)
    .await?;
    info!("{} matching entries found", values.len());
    return Ok(ListingGetResponse::Success {
        body: values,
        prp,
        path_prefix: match path_params.name {
            models::EnumerationNames::Schlagworte => "/api/v2/enumeration/schlagwort",
            models::EnumerationNames::Stationstypen => "/api/v2/enumeration/stationstyp",
            models::EnumerationNames::Parlamente => "/api/v2/enumeration/parlament",
            models::EnumerationNames::Vorgangstypen => "/api/v2/enumeration/vorgangstyp",
            models::EnumerationNames::Dokumententypen => "/api/v2/enumeration/dokumententyp",
            models::EnumerationNames::Vgidtypen => "/api/v2/enumeration/vg_ident_typ",
        },
    });
}

/// Retrieves a specific Document by it's api id
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(dok=%path_params.api_id))]
pub async fn dokument_get_by_id(
    server: &PazufaServer,
    header_params: &params::IfModSince,
    path_params: &params::ApiIdPathParam,
) -> Result<GetSingleResponse<models::Dokument>> {
    let mut tx = server.sqlx_db.begin().await?;
    let Some(dbid) = retrieve::dokument_dbid(path_params.api_id, &mut tx).await? else {
        info!("Not Found");
        return Ok(GetSingleResponse::NotFound);
    };
    let Some(result) =
        retrieve::dokument_by_id_ims(dbid, header_params.if_modified_since, &mut tx).await?
    else {
        info!("Not Modified");
        return Ok(GetSingleResponse::NotMod);
    };
    tx.commit().await?;
    info!("Document found");
    return Ok(GetSingleResponse::Success(Box::new(result)));
}

#[cfg(test)]
mod test_unauthorisiert {
    use crate::api::ListingGetResponse;
    use crate::api::vorgang as libvg;
    use crate::models;
    use crate::models::dokument::DokumentHash;
    use crate::models::dokument::HashWrapper;
    use crate::models::dokument::ZusammenfassungWrapper;
    use crate::models::dokument::Zusammenfassungstupel;
    use crate::models::enumerations::HashStrategy;
    use crate::models::enumerations::Mime;
    use crate::models::params;
    use crate::utils::testing::{TestSetup, generate};
    use uuid::uuid;

    #[tokio::test]
    async fn autoren_get_nocontent_on_page_overflow() {
        let scenario = TestSetup::new("autoren_get_nocontent_page_overflow").await;
        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let result = super::autoren_get(
            &scenario.server,
            &params::AutorenGetQueryParams {
                page: Some(500),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to get Data");
        assert!(
            matches!(result, ListingGetResponse::NoContent),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn autor_get_nocontent() {
        let scenario = TestSetup::new("autor_get_nocontent").await;
        let r = super::autoren_get(
            &scenario.server,
            &params::AutorenGetQueryParams {
                fach: None,
                org: None,
                person: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get Autor.");
        assert!(matches!(r, ListingGetResponse::NoContent));
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn autor_get_success() {
        let scenario = TestSetup::new("autor_get_success").await;

        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Autor.");

        let r = super::autoren_get(
            &scenario.server,
            &params::AutorenGetQueryParams {
                fach: None,
                org: None,
                person: None,
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get Autor.");
        assert!(
            matches!( &r,
                ListingGetResponse::Success { body, .. } if !body.is_empty()),
            "Body is empty, expected some object"
        );
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn gremien_get_nocontent_on_page_overflow() {
        let scenario = TestSetup::new("gremien_get_nocontent_page_overflow").await;
        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                page: Some(500),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to get Data");
        assert!(
            matches!(result, ListingGetResponse::NoContent),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn gremien_get_nocontent() {
        let scenario = TestSetup::new("test_gremien_get_nocontent").await;
        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: None,
                p: None,
                page: None,
                per_page: None,
                wp: None,
            },
        )
        .await
        .expect("Failed to get Gremium.");
        assert!(
            matches!(result, ListingGetResponse::NoContent),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn gremium_get_success() {
        let scenario = TestSetup::new("test_gremium_get_success").await;
        let vorgang = generate::default_vorgang();
        let dft_gremium = generate::default_gremium();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: None,
                p: None,
                page: None,
                per_page: None,
                wp: None,
            },
        )
        .await;

        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. })
                if body.len() == 1 && body.contains(&generate::default_gremium())
                    && prp.x_page == 1 && prp.x_total_count == 1 && prp.x_total_pages == 1)
        );

        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: Some("Inneres".to_string()),
                p: None,
                page: None,
                per_page: None,
                wp: None,
            },
        )
        .await;
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. })
                if body.len() == 1 && body.contains(&generate::default_gremium())
                    && prp.x_page == 1 && prp.x_total_count == 1 && prp.x_total_pages == 1)
        );

        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: None,
                p: Some(dft_gremium.parlament),
                page: None,
                per_page: None,
                wp: None,
            },
        )
        .await;
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. })
                if body.len() == 1 && body.contains(&generate::default_gremium())
                    && prp.x_page == 1 && prp.x_total_count == 1 && prp.x_total_pages == 1)
        );

        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: None,
                p: None,
                page: None,
                per_page: None,
                wp: Some(20),
            },
        )
        .await;
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. })
                if body.len() == 1 && body.contains(&generate::default_gremium())
                    && prp.x_page == 1 && prp.x_total_count == 1 && prp.x_total_pages == 1)
        );

        let result = super::gremien_get(
            &scenario.server,
            &params::GremienGetQueryParams {
                gr: None,
                p: Some(models::Parlament::Be),
                page: None,
                per_page: None,
                wp: None,
            },
        )
        .await;
        assert!(
            matches!(&result, Ok(ListingGetResponse::NoContent)),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn enums_get_nocontent_on_page_overflow() {
        let scenario = TestSetup::new("enums_get_nocontent_page_overflow").await;
        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let result = super::enum_get(
            &scenario.server,
            &params::EnumPathParams {
                name: models::EnumerationNames::Parlamente,
            },
            &params::EnumGetQueryParams {
                page: Some(500),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to get Data");
        assert!(
            matches!(result, ListingGetResponse::NoContent),
            "Expected to find no entries"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn enum_get_nocontent() {
        let scenario = TestSetup::new("test_enum_get_nocontent").await;
        let test_parameters = vec![
            models::EnumerationNames::Parlamente,
            models::EnumerationNames::Dokumententypen,
            models::EnumerationNames::Vgidtypen,
            models::EnumerationNames::Vorgangstypen,
            models::EnumerationNames::Stationstypen,
            models::EnumerationNames::Schlagworte,
        ];
        for tp in test_parameters {
            let result = super::enum_get(
                &scenario.server,
                &params::EnumPathParams { name: tp },
                &params::EnumGetQueryParams {
                    contains: Some("apfelsaftcocktail".to_string()), // hoffentlich kommt niemand auf die depperte idee apfelsaftcocktail-Gesetzgebung zu machen
                    page: None,
                    per_page: None,
                },
            )
            .await;
            assert!(
                matches!(&result, Ok(ListingGetResponse::NoContent)),
                "Expected to find no entries, got {result:?} instead"
            );
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn enum_get_success() {
        let scenario = TestSetup::new("test_enum_get_success").await;
        let test_cases_one = vec![
            (models::EnumerationNames::Vorgangstypen, "einspruch"),
            (models::EnumerationNames::Vgidtypen, "initdrucks"),
            (models::EnumerationNames::Dokumententypen, "entwurf"),
            (models::EnumerationNames::Stationstypen, "blt"),
            (models::EnumerationNames::Schlagworte, "schuppe"),
            (models::EnumerationNames::Parlamente, "B"),
        ];
        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        for (name, mtch) in test_cases_one {
            let result = super::enum_get(
                &scenario.server,
                &params::EnumPathParams { name },
                &params::EnumGetQueryParams {
                    contains: Some(mtch.to_string()),
                    page: None,
                    per_page: None,
                },
            )
            .await;
            assert!(
                matches!(&result, Ok(ListingGetResponse::Success { body, .. }) if !body.is_empty() ),
                "Expected to get success, got {result:?} instead with test case `{name}` / `{mtch}`"
            );
        }
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn dokumente_get_success_new_members() {
        let scenario = TestSetup::new("test_dokumente_get_success_new_members").await;
        let api_id = uuid!("06836a6d-30e6-4200-b968-ca362af92d2e");
        let vorgang = models::Vorgang {
            stationen: vec![models::Station {
                dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                    models::Dokument {
                        api_id: Some(api_id),
                        hash: HashWrapper::Future(vec![
                            DokumentHash {
                                mime: Mime::ApplicationPdf,
                                value: "469527345692734698276459726439587234857284756".to_owned(),
                                strategy: HashStrategy::Sha1Bytes,
                            },
                            DokumentHash {
                                mime: Mime::TextHtml,
                                value: "4695273456927346982764597264395872348572847ffe".to_owned(),
                                strategy: HashStrategy::Sha256Text,
                            },
                            DokumentHash {
                                mime: Mime::ApplicationPdf,
                                value: "4695273456927346912764597264395872348572847ffe".to_owned(),
                                strategy: HashStrategy::Sha256Bytes,
                            },
                        ]),
                        zusammenfassung: Some(ZusammenfassungWrapper::Future(vec![
                            Zusammenfassungstupel {
                                typ: "full".to_owned(),
                                inhalt: "something something content creation".to_owned(),
                            },
                        ])),
                        ..generate::default_dokument()
                    },
                ))],
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let _insert = libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let dokument = super::dokument_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
        )
        .await;

        let dokument2 = super::dokument_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
        )
        .await;
        assert!(
            matches!(dokument,
            Ok(crate::api::GetSingleResponse::Success(d))
                if
                matches!(d.hash, HashWrapper::Future(ref hv)
                    if hv.len() == 3 && hv.contains(&DokumentHash {
                            mime: Mime::ApplicationPdf,
                            value: "469527345692734698276459726439587234857284756".to_owned(),
                            strategy: HashStrategy::Sha1Bytes,
                        }) &&
                    hv.contains(&DokumentHash {
                                    mime: Mime::TextHtml,
                                    value: "4695273456927346982764597264395872348572847ffe".to_owned(),
                                    strategy: HashStrategy::Sha256Text,
                                }) &&
                    hv.contains(&DokumentHash {
                                    mime: Mime::ApplicationPdf,
                                    value: "4695273456927346912764597264395872348572847ffe".to_owned(),
                                    strategy: HashStrategy::Sha256Bytes,
                                })
                    ) &&
                matches!(d.zusammenfassung, Some(ZusammenfassungWrapper::Legacy(_)))
            ),
            "{dokument2:?}"
        );
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn dokument_get_success() {
        let scenario = TestSetup::new("test_dokument_get_success").await;
        let vorgang = generate::default_vorgang();
        libvg::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        let result = super::dokumente_get_by_params(
            &scenario.server,
            models::enumerations::DokumentFormat::Pazufa,
            params::ListingQueryParams {
                since: None,
                until: None,
                page: None,
                per_page: None,
            },
            None,
        )
        .await;

        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if body.len() == 2),
            "{result:?}"
        );
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if body.contains(&generate::default_stellungnahme()))
        );
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if body.contains(&generate::default_dokument()))
        );
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if prp.x_page == 1)
        );
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if prp.x_total_count == 2)
        );
        assert!(
            matches!(&result, Ok(ListingGetResponse::Success { body, prp,  .. }) if prp.x_total_pages == 1)
        );

        let result = super::dokumente_get_by_params(
            &scenario.server,
            models::enumerations::DokumentFormat::Pazufa,
            params::ListingQueryParams {
                since: Some(
                    chrono::DateTime::parse_from_rfc3339("2200-01-01T01:01:01Z")
                        .expect("static str")
                        .to_utc(),
                ),
                until: None,
                page: None,
                per_page: None,
            },
            None,
        )
        .await;

        assert!(matches!(&result, Ok(ListingGetResponse::NoContent)));
        scenario.teardown().await;
    }
}
