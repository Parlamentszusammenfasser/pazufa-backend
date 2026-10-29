use axum::extract::{Json, Path, Query, State};
use axum::http::HeaderMap;
use pazufa_backend_lib::PazufaServer;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, ListingGetResponse, PutResponse, vorgang as libvg,
};
use pazufa_models::params::HeaderParams;
use pazufa_models::params::{IfModSince, VorgangGetQueryParams, XScraperId};
use pazufa_models::{self as models, APIScope, params};
use std::sync::Arc;
use tracing::{info, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(
    vorgang_put,
    vorgang_get,
    vorgang_get_by_id,
    vorgang_delete,
    vorgang_bulk_delete,
    vorgang_id_put
))]
pub(super) struct VorgangApi;

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new()
        .routes(routes!(vorgang_put, vorgang_get))
        .routes(routes!(vorgang_bulk_delete))
        .routes(routes!(vorgang_get_by_id, vorgang_id_put, vorgang_delete))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(put),
    path = "",
    tags = ["Vorgang", "Collector-Schnittstellen"],
    description = "Collector interface for inserting a new legislative process. Processes are automatically deduplicated against existing data and merged if necessary. Used by data collection services to add new legislative processes to the system.",
    security(("apiKey" = [ "collector", "admin", "keyadder"])),
    params(params::XScraperId),
    request_body = models::Vorgang,
    responses(PutResponse)
)]
async fn vorgang_put(
    claims: Claims,
    HeaderParams(scraper_id): HeaderParams<XScraperId>,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<models::Vorgang>,
) -> PutResponse {
    // Authentication
    if !matches!(
        claims.0,
        APIScope::Collector | APIScope::KeyAdder | APIScope::Admin
    ) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libvg::vorgang_put(api_impl.as_ref(), claims.1, &scraper_id, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}

#[utoipa::path(
    method(get),
    path = "",
    tags = ["Vorgang", "Unauthorisiert"],
    description = "Retrieves a filterable list of legislative processes. Returns up to 64 processes per request, which can be filtered by various criteria including time range, parliament, and electoral period.",
    params(
        params::VorgangGetQueryParams,
        params::IfModSince,
        params::ExpandQueryParam
        ),
    responses(ListingGetResponse<models::Vorgang>)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_get(
    headers: HeaderMap,
    Query(query_params): Query<params::VorgangGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> ListingGetResponse<models::Vorgang> {
    // Header parameters
    let ims = match IfModSince::try_from(headers.get(axum::http::header::IF_MODIFIED_SINCE)) {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return ListingGetResponse::BadReq;
        }
    };

    libvg::vorgang_get(
        api_impl.as_ref(),
        &ims,
        &query_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("{e}");
        ListingGetResponse::Error(e.to_string())
    })
}

#[utoipa::path(
    method(get),
    path = "/{api_id}",
    tags = ["Vorgang", "Unauthorisiert"],
    description = "Retrieves a specific legislative process by its unique identifier. Returns comprehensive details about the process including all its stations, associated documents, and metadata. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    params(
        params::ApiIdPathParam,
        params::IfModSince,
        params::ExpandQueryParam,
        ),
    responses(GetSingleResponse<models::Vorgang>)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_get_by_id(
    headers: HeaderMap,
    Path(path_params): Path<params::ApiIdPathParam>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> GetSingleResponse<models::Vorgang> {
    // Header parameters
    let ims = match IfModSince::try_from(headers.get(axum::http::header::IF_MODIFIED_SINCE)) {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return GetSingleResponse::BadReq;
        }
    };

    libvg::vorgang_get_by_id(
        api_impl.as_ref(),
        &ims,
        &path_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("{e}");
        GetSingleResponse::Error(e.to_string())
    })
}

#[utoipa::path(
    method(delete),
    path = "",
    tags = ["Vorgang", "Data-Administration"],
    description = "Administrative endpoint to delete Vorgang objects from the system by filter parmeters. This operation cannot be undone. With Empty Filters, this is a NOOP",
    params( params::VorgangGetQueryParams),
    security(("apiKey" = [ "admin", "keyadder"])),
    responses(DeleteResponse)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_bulk_delete(
    claims: Claims,
    Query(qparams): Query<VorgangGetQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> PutResponse {
    use pazufa_backend_lib::db;
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    if qparams == VorgangGetQueryParams::default() {
        tracing::warn!(
            "Bulk delete endpoint called without filters; This is a NO-OP to prevent accidents"
        );
        return PutResponse::Success;
    }
    let mut tx = match api_impl.sqlx_db.begin().await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("Unable to start transaction context: {e}");
            return PutResponse::Error(e.to_string());
        }
    };

    // filter for api_ids
    let delete_set = match db::retrieve::vorgang::vorgang_fetch_dbids(
        db::retrieve::VGGetParameters {
            parlament: qparams.p,
            wp: qparams.wp.map(i32::from),
            vgtyp: qparams.vgtyp,
            inipsn: qparams.person,
            iniorg: qparams.org,
            lower_date: qparams.since,
            upper_date: qparams.until,
            inifch: qparams.fach,
            if_mod_since: None,
        },
        &mut *tx,
    )
    .await
    {
        Ok(ds) => ds,
        Err(e) => {
            tracing::warn!("Unable to filter Database with given Query Parameters: {e}");
            return PutResponse::Error(e.to_string());
        }
    };
    // bulk delete these api ids
    match db::delete::bulk_delete_vorgang(&delete_set, &mut *tx).await {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!("Unable to delete previously selected Vorgang Objects by ID: {e}");
            return PutResponse::Error(e.to_string());
        }
    }
    if let Err(e) = tx.commit().await {
        tracing::warn!("Error committing transaction");
        PutResponse::Error(e.to_string())
    } else {
        PutResponse::Success
    }
}

#[utoipa::path(
    method(delete),
    path = "/{api_id}",
    tags = ["Vorgang", "Data-Administration"],
    description = "Administrative endpoint to completely remove a legislative process from the system. This operation cannot be undone.",
    params(params::ApiIdPathParam),
    security(("apiKey" = [ "admin", "keyadder"])),
    responses(DeleteResponse)
)]
#[tracing::instrument(skip_all, fields( vg_id=%path_params.api_id))]
async fn vorgang_delete(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        warn!("Not authorized");
        return DeleteResponse::Unauth;
    }

    match pazufa_backend_lib::db::delete::delete_vorgang_by_api_id(
        path_params.api_id,
        &api_impl.sqlx_db,
    )
    .await
    {
        Ok(()) => {
            info!(target: "obj", "Deleted Vorgang {}", path_params.api_id);
            DeleteResponse::Success
        }
        Err(e) => {
            warn!("{e}");
            DeleteResponse::Error(e.to_string())
        }
    }
}

#[utoipa::path(
    put,
    path = "/{api_id}",
    tags = ["Vorgang", "Data-Administration"],
    description = "Administrative endpoint to directly set or replace a legislative process without merging or matching. Replaces the entire process data if it exists, or creates a new one if it doesn't.",
    params(params::ApiIdPathParam),
    security(("apiKey" = [ "admin", "keyadder"])),
    responses(PutResponse)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_id_put(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<models::Vorgang>,
) -> PutResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libvg::vorgang_id_put(api_impl.as_ref(), claims.1, &path_params, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}
#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::PazufaServer;
    use pazufa_backend_lib::api::auth::Claims;
    use pazufa_backend_lib::api::{GetSingleResponse, PutResponse, vorgang as libvg};
    use pazufa_backend_lib::utils::header_params::HeaderParams;
    use pazufa_models;
    use pazufa_models::APIScope;
    use pazufa_models::params::{self, XScraperId};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_auth_vorgang_id_put() {
        let mut scenario = TestSetup::new("test_auth_vorgang_id_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::vorgang_id_put(
                hm,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
                axum::Json(models::Vorgang::default()),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_auth_vorgang_delete() {
        let mut scenario = TestSetup::new("test_auth_vorgang_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::vorgang_delete(
                api_key,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin],
            call_closure,
            pazufa_backend_lib::api::DeleteResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_auth_vorgang_put() {
        let mut scenario = TestSetup::new("test_auth_vorgang_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::vorgang_put(
                api_key,
                HeaderParams(XScraperId(uuid::Uuid::nil())),
                axum::extract::State(server),
                axum::Json(models::Vorgang::default()),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin, APIScope::Collector],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    /// Minimal, track-valid Vorgang: exactly one `parl-initiativ` station in the given parliament.
    fn test_vorgang(
        api_id: uuid::Uuid,
        typ: models::Vorgangstyp,
        parlament: models::Parlament,
        wahlperiode: u16,
    ) -> models::Vorgang {
        models::Vorgang {
            api_id,
            titel: format!("Testvorgang {api_id}"),
            typ,
            wahlperiode,
            initiatoren: vec![models::Autor {
                person: None,
                organisation: format!("Fraktion {parlament}"),
                fachgebiet: None,
                lobbyregister: None,
            }],
            stationen: vec![models::Station {
                api_id: Some(uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_OID,
                    api_id.as_bytes(),
                )),
                typ: models::Stationstyp::ParlInitiativ,
                zp_start: chrono::DateTime::parse_from_rfc3339("2024-01-01T12:00:00+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc(),
                zp_modifiziert: None,
                gremium: models::Gremium {
                    parlament,
                    wahlperiode,
                    link: None,
                    name: format!("Plenum {parlament}"),
                },
                gremium_federf: None,
                link: None,
                titel: None,
                trojanergefahr: None,
                schlagworte: vec![],
                touched_by: vec![],
                dokumente: vec![],
                stellungnahmen: vec![],
                additional_links: vec![],
                meta_tiebreaker: None,
            }],
            ..models::Vorgang::default()
        }
    }

    async fn put_vorgang(server: &PazufaServer, vorgang: &models::Vorgang) {
        let response = libvg::vorgang_id_put(
            server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            vorgang,
        )
        .await
        .expect("Failed to get valid server response when trying to push a test Vorgang.");
        assert_eq!(response, PutResponse::Success);
    }

    async fn vorgang_exists(server: &PazufaServer, api_id: uuid::Uuid) -> bool {
        let response = libvg::vorgang_get_by_id(
            server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
            false,
        )
        .await
        .expect("Failed to get valid server response when trying to get Vorgang by ID.");
        matches!(
            response,
            GetSingleResponse::NotMod | GetSingleResponse::Success(..)
        )
    }

    async fn bulk_delete(
        server: &Arc<PazufaServer>,
        scope: APIScope,
        qparams: params::VorgangGetQueryParams,
    ) -> PutResponse {
        super::vorgang_bulk_delete(
            Claims(scope, 1),
            axum::extract::Query(qparams),
            axum::extract::State(Arc::clone(server)),
        )
        .await
    }

    #[tokio::test]
    async fn test_auth_vorgang_bulk_delete() {
        let mut scenario = TestSetup::new("test_auth_vorgang_bulk_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::vorgang_bulk_delete(
                api_key,
                axum::extract::Query(params::VorgangGetQueryParams::default()),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    /// Without filters the endpoint must not touch anything, however much data there is.
    #[tokio::test]
    async fn test_vorgang_bulk_delete_empty_filters_are_noop() {
        let mut scenario = TestSetup::new("test_vorgang_bulk_delete_noop").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let vorgang = test_vorgang(
            uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef0001"),
            models::Vorgangstyp::BuZustimmungInibreg,
            models::Parlament::Bt,
            20,
        );
        put_vorgang(&server, &vorgang).await;

        let response = bulk_delete(
            &server,
            APIScope::Admin,
            params::VorgangGetQueryParams::default(),
        )
        .await;
        assert_eq!(response, PutResponse::Success);
        assert!(
            vorgang_exists(&server, vorgang.api_id).await,
            "Bulk delete without filters must be a NO-OP, but the Vorgang is gone."
        );

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    /// Only the Vorgang objects matching the query parameters may be deleted.
    #[tokio::test]
    async fn test_vorgang_bulk_delete_deletes_only_matches() {
        let mut scenario = TestSetup::new("test_vorgang_bulk_delete_filtered").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let bundestag_old_wp = test_vorgang(
            uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef0020"),
            models::Vorgangstyp::BuZustimmungInibreg,
            models::Parlament::Bt,
            20,
        );
        let bundestag_new_wp = test_vorgang(
            uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef0021"),
            models::Vorgangstyp::BuZustimmungInibreg,
            models::Parlament::Bt,
            21,
        );
        let bayern_landtag = test_vorgang(
            uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef0019"),
            models::Vorgangstyp::GgLandParl,
            models::Parlament::By,
            19,
        );
        for vorgang in [&bundestag_old_wp, &bundestag_new_wp, &bayern_landtag] {
            put_vorgang(&server, vorgang).await;
        }

        // 1. filters that match nothing leave everything alone
        let response = bulk_delete(
            &server,
            APIScope::KeyAdder,
            params::VorgangGetQueryParams {
                wp: Some(99),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(response, PutResponse::Success);
        for vorgang in [&bundestag_old_wp, &bundestag_new_wp, &bayern_landtag] {
            assert!(
                vorgang_exists(&server, vorgang.api_id).await,
                "A filter matching no Vorgang deleted {}.",
                vorgang.api_id
            );
        }

        // 2. delete by electoral period
        let response = bulk_delete(
            &server,
            APIScope::Admin,
            params::VorgangGetQueryParams {
                wp: Some(20),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(response, PutResponse::Success);
        assert!(
            !vorgang_exists(&server, bundestag_old_wp.api_id).await,
            "The Vorgang of the filtered electoral period was not deleted."
        );
        for vorgang in [&bundestag_new_wp, &bayern_landtag] {
            assert!(
                vorgang_exists(&server, vorgang.api_id).await,
                "Deletion by electoral period also removed the unrelated Vorgang {}.",
                vorgang.api_id
            );
        }

        // 3. delete by parliament, which is a property of the stations, not of the Vorgang itself
        let response = bulk_delete(
            &server,
            APIScope::Admin,
            params::VorgangGetQueryParams {
                p: Some(models::Parlament::By),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(response, PutResponse::Success);
        assert!(
            !vorgang_exists(&server, bayern_landtag.api_id).await,
            "The Vorgang of the filtered parliament was not deleted."
        );
        assert!(
            vorgang_exists(&server, bundestag_new_wp.api_id).await,
            "Deletion by parliament also removed the Vorgang of another parliament."
        );

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
}
