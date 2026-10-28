#[cfg(feature = "track-validation")]
use crate::db::validate::Tracks;
use crate::utils::notify;
use crate::utils::status::Statistics;
use crate::utils::tracing::Logging;
use crate::{Configuration, models};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{Datelike, NaiveDate};
use pazufa_models::{Dokument, FTMDokumentPages};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use utoipa::openapi::{
    self, ArrayBuilder, ContentBuilder, HeaderBuilder, OneOfBuilder, ResponseBuilder,
    ResponsesBuilder, Schema,
};
use utoipa::{IntoResponses, PartialSchema, ToSchema};

pub mod auth;
pub mod misc;
pub mod misc_auth;
pub mod sitzung;
pub mod vorgang;

#[derive(Debug, PartialEq)]
pub enum PazufaOrFtmListingResponse {
    Pazufa {
        body: Vec<models::Dokument>,
        prp: PaginationResponsePart,
        path_prefix: &'static str,
    },
    Ftm {
        body: Vec<models::FTMDokumentPages>,
        prp: PaginationResponsePart,
        path_prefix: &'static str,
    },
    NotMod,
    NotImpl,
    NoContent,
    BadReq,
    Error(String),
}
impl axum::response::IntoResponse for PazufaOrFtmListingResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Error(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            Self::NoContent => StatusCode::NO_CONTENT.into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::NotImpl => StatusCode::NOT_IMPLEMENTED.into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::Ftm {
                body,
                prp,
                path_prefix,
            } => {
                let mut hm = HeaderMap::new();
                let Ok(link) = HeaderValue::from_str(&prp.generate_link_header(path_prefix)) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                hm.insert("link", link);
                hm.insert("x-per-page", prp.x_per_page.into());
                hm.insert("x-page", prp.x_page.into());
                hm.insert("x-total-pages", prp.x_total_pages.into());
                hm.insert("x-total-count", prp.x_total_count.into());

                (StatusCode::OK, hm, Json(body)).into_response()
            }
            Self::Pazufa {
                body,
                prp,
                path_prefix,
            } => {
                let mut hm = HeaderMap::new();
                let Ok(link) = HeaderValue::from_str(&prp.generate_link_header(path_prefix)) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                hm.insert("link", link);
                hm.insert("x-per-page", prp.x_per_page.into());
                hm.insert("x-page", prp.x_page.into());
                hm.insert("x-total-pages", prp.x_total_pages.into());
                hm.insert("x-total-count", prp.x_total_count.into());

                (StatusCode::OK, hm, Json(body)).into_response()
            }
        }
    }
}
impl IntoResponses for PazufaOrFtmListingResponse {
    fn responses() -> BTreeMap<String, openapi::RefOr<openapi::response::Response>> {
        use models::headers as hdr;
        ResponsesBuilder::new()
            .response(
                "200",
                ResponseBuilder::new()
                    .header(
                        "link",
                        HeaderBuilder::new()
                            .schema(Some(hdr::Link::schema()))
                            .build(),
                    )
                    .header(
                        "x-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-per-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPerPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-pages",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-count",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .description("Ok")
                    .content(
                        "application/json",
                        ContentBuilder::new()
                            .schema(Some(
                                OneOfBuilder::new()
                                    .item(ArrayBuilder::new().items(Dokument::schema()))
                                    .item(ArrayBuilder::new().items(FTMDokumentPages::schema())),
                            ))
                            .build(),
                    ),
            )
            .response(
                "500",
                ResponseBuilder::new()
                    .content("text/plain", ContentBuilder::new().schema(Some("")).build())
                    .description("Error"),
            )
            .response("204", ResponseBuilder::new().description("No Content"))
            .response("304", ResponseBuilder::new().description("Not Modified"))
            .response("400", ResponseBuilder::new().description("Bad Request"))
            .build()
            .into()
    }
}

#[derive(PartialEq, Debug)]
pub enum AuthListingGetResponse<T: ToSchema + Sized + Sync + Send + Serialize + Debug + PartialEq> {
    Error(String),
    Success {
        body: Vec<T>,
        prp: PaginationResponsePart,
        path_prefix: &'static str,
    },
    Unauth,
    NoContent,
    NotMod,
    BadReq,
}

impl<T: ToSchema + Sized + Sync + Send + Serialize + Debug + PartialEq> IntoResponses
    for AuthListingGetResponse<T>
{
    fn responses() -> BTreeMap<String, openapi::RefOr<openapi::response::Response>> {
        use models::headers as hdr;
        ResponsesBuilder::new()
            .response(
                "200",
                ResponseBuilder::new()
                    .header(
                        "link",
                        HeaderBuilder::new()
                            .schema(Some(hdr::Link::schema()))
                            .build(),
                    )
                    .header(
                        "x-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-per-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPerPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-pages",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-count",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .description("Ok")
                    .content(
                        "application/json",
                        ContentBuilder::new().schema(Some(T::schema())).build(),
                    ),
            )
            .response(
                "500",
                ResponseBuilder::new()
                    .content("text/plain", ContentBuilder::new().schema(Some("")).build())
                    .description("Error"),
            )
            .response("204", ResponseBuilder::new().description("No Content"))
            .response("401", ResponseBuilder::new().description("Unauthorized"))
            .response("304", ResponseBuilder::new().description("Not Modified"))
            .response("400", ResponseBuilder::new().description("Bad Request"))
            .build()
            .into()
    }
}
impl<T: ToSchema + Sized + Sync + Send + Serialize + Debug + PartialEq> IntoResponse
    for AuthListingGetResponse<T>
{
    fn into_response(self) -> Response {
        match self {
            Self::Error(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
            Self::NoContent => StatusCode::NO_CONTENT.into_response(),
            Self::Success {
                body,
                prp,
                path_prefix,
            } => {
                let mut hm = HeaderMap::new();
                let Ok(link) = HeaderValue::from_str(&prp.generate_link_header(path_prefix)) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                hm.insert("link", link);
                hm.insert("x-per-page", prp.x_per_page.into());
                hm.insert("x-page", prp.x_page.into());
                hm.insert("x-total-pages", prp.x_total_pages.into());
                hm.insert("x-total-count", prp.x_total_count.into());

                (StatusCode::OK, hm, Json(body)).into_response()
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ListingGetResponse<T: ToSchema + Sized + Sync + Send + Serialize + PartialEq + Debug> {
    Error(String),
    Success {
        body: Vec<T>,
        prp: PaginationResponsePart,
        path_prefix: &'static str,
    },
    NoContent,
    NotMod,
    BadReq,
}

impl<T: ToSchema + Sized + Sync + Send + Serialize + PartialEq + Debug> IntoResponses
    for ListingGetResponse<T>
{
    fn responses() -> BTreeMap<String, openapi::RefOr<openapi::response::Response>> {
        use models::headers as hdr;
        ResponsesBuilder::new()
            .response(
                "200",
                ResponseBuilder::new()
                    .header(
                        "link",
                        HeaderBuilder::new()
                            .schema(Some(hdr::Link::schema()))
                            .build(),
                    )
                    .header(
                        "x-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-per-page",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XPerPage::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-pages",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .header(
                        "x-total-count",
                        HeaderBuilder::new()
                            .schema(Some(hdr::XTotalPages::schema()))
                            .build(),
                    )
                    .description("Ok")
                    .content(
                        "application/json",
                        ContentBuilder::new()
                            .schema(Some(Schema::Array(openapi::Array::new(T::schema()))))
                            .build(),
                    ),
            )
            .response(
                "500",
                ResponseBuilder::new()
                    .content("text/plain", ContentBuilder::new().schema(Some("")).build())
                    .description("Error"),
            )
            .response("204", ResponseBuilder::new().description("No Content"))
            .response("304", ResponseBuilder::new().description("Not Modified"))
            .response("400", ResponseBuilder::new().description("Bad Request"))
            .build()
            .into()
    }
}
impl<T: ToSchema + Sized + Sync + Send + Serialize + PartialEq + Debug> IntoResponse
    for ListingGetResponse<T>
{
    fn into_response(self) -> Response {
        match self {
            Self::Error(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::NoContent => StatusCode::NO_CONTENT.into_response(),
            Self::Success {
                body,
                prp,
                path_prefix,
            } => {
                let mut hm = HeaderMap::new();
                let Ok(link) = HeaderValue::from_str(&prp.generate_link_header(path_prefix)) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                hm.insert("link", link);
                hm.insert("x-per-page", prp.x_per_page.into());
                hm.insert("x-page", prp.x_page.into());
                hm.insert("x-total-pages", prp.x_total_pages.into());
                hm.insert("x-total-count", prp.x_total_count.into());

                (StatusCode::OK, hm, Json(body)).into_response()
            }
        }
    }
}

#[derive(Debug, IntoResponses, PartialEq)]
pub enum PutResponse {
    #[response(status = 201, description = "Ressource Created")]
    Success,
    #[response(status = 401, description = "Authentication or Authorization failed")]
    Unauth,
    #[response(status = 304, description = "Ressource was not modified")]
    NotMod,
    #[response(status = 409, description = "Conflict creating the ressource")]
    Conflict,
    #[response(status = 400, description = "Malformed Request")]
    BadReq(String),
    #[response(status = 500, description = "Internal Server Error")]
    Error(String),
}
impl IntoResponse for PutResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Conflict => StatusCode::CONFLICT.into_response(),
            Self::Success => StatusCode::CREATED.into_response(),
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::BadReq(s) => (StatusCode::BAD_REQUEST, s).into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
        }
    }
}

#[derive(Debug, IntoResponses, PartialEq)]
pub enum DeleteResponse {
    #[response(status = 204, description = "No Content")]
    Success,
    #[response(status = 401, description = "Authentication failed")]
    Unauth,
    #[response(status = 403, description = "Authorization failed")]
    Forbid,
    #[response(status = 400, description = "Malformed Request")]
    BadReq,
    #[response(status = 500, description = "Internal Server Error")]
    Error(String),
}
impl IntoResponse for DeleteResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Success => StatusCode::NO_CONTENT.into_response(),
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::Forbid => StatusCode::FORBIDDEN.into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
        }
    }
}
#[derive(IntoResponses, PartialEq, Debug)]
pub enum GetSingleResponse<T: ToSchema + Sized + Sync + Send + Serialize + PartialEq + Debug> {
    #[response(status = 500, description = "Internal Server Error")]
    Error(String),
    #[response(
        status = 200,
        content_type = "application/json",
        description = "Successful Response"
    )]
    Success(Box<T>),
    #[response(status = 404, description = "Ressource not found")]
    NotFound,
    #[response(status = 304, description = "Ressource was not modified")]
    NotMod,
    #[response(status = 400, description = "Malformed Request")]
    BadReq,
    #[response(status = 501, description = "Not Implemented")]
    NotImpl,
}
impl<T: ToSchema + Sized + Sync + Send + Serialize + PartialEq + Debug> IntoResponse
    for GetSingleResponse<T>
{
    fn into_response(self) -> Response {
        match self {
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::NotImpl => StatusCode::NOT_IMPLEMENTED.into_response(),
            Self::Success(body) => (StatusCode::OK, Json(*body)).into_response(),
        }
    }
}

#[derive(Clone)]
pub struct PazufaServer {
    pub sqlx_db: sqlx::PgPool,
    pub mailbundle: Option<Arc<notify::MailBundle>>,
    pub config: Configuration,
    pub logging: Logging,
    pub statistics: Arc<Statistics>,
    #[cfg(feature = "track-validation")]
    pub tracks: Arc<Tracks>,
}
pub type LTZFArc = std::sync::Arc<PazufaServer>;
impl PazufaServer {
    pub fn new(
        sqlx_db: sqlx::PgPool,
        config: Configuration,
        mailbundle: Option<notify::MailBundle>,
        logging: Logging,
    ) -> Self {
        #[cfg(feature = "track-validation")]
        let track = Tracks::new(&config.track_path);
        #[cfg(feature = "track-validation")]
        let stats = Statistics::new(&config.track_path);

        #[cfg(not(feature = "track-validation"))]
        let stats = Statistics::new();

        Self {
            config,
            sqlx_db,
            mailbundle: mailbundle.map(Arc::new),
            logging,
            statistics: Arc::new(stats),
            #[cfg(feature = "track-validation")]
            tracks: Arc::new(track),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct PaginationResponsePart {
    pub x_total_count: u32,
    pub x_total_pages: u32,
    pub x_page: u32,
    pub x_per_page: u32,
}

impl PaginationResponsePart {
    pub const DEFAULT_PER_PAGE: u32 = 32;
    pub const MAX_PER_PAGE: u32 = 256;

    #[must_use]
    pub fn new(x_total_count: u32, x_page: Option<u32>, x_per_page: Option<u32>) -> Self {
        let x_per_page =
            x_per_page.map_or(Self::DEFAULT_PER_PAGE, |x| x.clamp(1, Self::MAX_PER_PAGE));

        let x_total_pages = x_total_count.div_ceil(x_per_page).max(1);
        let x_page = x_page.map_or(1, |x| x.clamp(1, x_total_pages));

        Self {
            x_total_count,
            x_total_pages,
            x_page,
            x_per_page,
        }
    }
    #[must_use]
    pub fn limit(&self) -> i64 {
        i64::from(self.x_per_page)
    }
    #[must_use]
    pub fn offset(&self) -> i64 {
        let page_offset = i64::from(self.x_page.saturating_sub(1));
        let per_page = i64::from(self.x_per_page);

        #[allow(clippy::arithmetic_side_effects, reason = "Wide multiplication.")]
        {
            page_offset * per_page
        }
    }
    #[must_use]
    pub fn start(&self) -> usize {
        usize::try_from(
            self.x_page
                .saturating_sub(1)
                .saturating_mul(self.x_per_page),
        )
        .unwrap_or(0)
    }

    #[must_use]
    pub fn end(&self) -> usize {
        let Ok(offset) = usize::try_from(self.offset()) else {
            #[cfg(target_pointer_width = "64")]
            unreachable!("u32 forces non-negativity.");
            #[cfg(not(target_pointer_width = "64"))]
            panic!("Offset exceeded usize limit.");
        };
        let Ok(limit) = usize::try_from(self.limit()) else {
            #[cfg(target_pointer_width = "64")]
            unreachable!("u32 forces non-negativity.");
            #[cfg(not(target_pointer_width = "64"))]
            panic!("Offset exceeded usize limit.");
        };
        let Ok(total_count) = usize::try_from(self.x_total_count) else {
            unreachable!("u32 forces non-negativity.")
        };
        total_count.min(offset.saturating_add(limit))
    }
    #[must_use]
    pub fn generate_link_header(&self, link_first_part: &str) -> String {
        let mut link_string = String::new();
        if self.x_page < self.x_total_pages {
            link_string = format!(
                "<\"{}?page={}&per_page={}\">; rel=\"next\", ",
                link_first_part,
                self.x_page.saturating_add(1),
                self.x_per_page
            );
        }
        if self.x_page > 1 {
            link_string = format!(
                "{}<\"{}?page={}&per_page={}\">; rel=\"previous\", ",
                link_string,
                link_first_part,
                self.x_page.saturating_sub(1),
                self.x_per_page
            );
        }
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"first\", ",
            link_string, link_first_part, 1, self.x_per_page
        );
        link_string = format!(
            "{}<\"{}?page={}&per_page={}\">; rel=\"last\"",
            link_string,
            link_first_part,
            self.x_total_pages.max(1),
            self.x_per_page
        );
        link_string
    }
}

#[cfg(test)]
mod prp_test {
    use crate::api::PaginationResponsePart;
    #[test]
    fn test_link_header() {
        let prp = PaginationResponsePart::new(0, None, Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts.contains(&"<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=1&per_page=16\">; rel=\"last\""),
            "{link_hdr_parts:?}"
        );
        assert_eq!(link_hdr_parts.len(), 2);

        let prp = PaginationResponsePart::new(100, Some(1), Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts.contains(&"<\"/?page=2&per_page=16\">; rel=\"next\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=7&per_page=16\">; rel=\"last\""),
            "{link_hdr_parts:?}"
        );
        assert_eq!(link_hdr_parts.len(), 3);

        let prp = PaginationResponsePart::new(100, Some(2), Some(16));
        let lh = prp.generate_link_header("/");
        let link_hdr_parts: Vec<_> = lh.split(", ").collect();
        assert!(
            link_hdr_parts.contains(&"<\"/?page=3&per_page=16\">; rel=\"next\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=1&per_page=16\">; rel=\"previous\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=1&per_page=16\">; rel=\"first\""),
            "{link_hdr_parts:?}"
        );
        assert!(
            link_hdr_parts.contains(&"<\"/?page=7&per_page=16\">; rel=\"last\""),
            "{link_hdr_parts:?}"
        );
        assert_eq!(link_hdr_parts.len(), 4);
    }

    #[test]
    fn test_start_and_end() {
        let prp = PaginationResponsePart::new(0, None, None);
        assert_eq!(prp.start(), 0);
        assert_eq!(prp.end(), 0, "{prp:?}");

        let prp = PaginationResponsePart::new(1, None, None);
        assert_eq!(prp.start(), 0);
        assert_eq!(prp.end(), 1);
    }

    #[test]
    fn test_empty_result_but_page() {
        let prp = PaginationResponsePart::new(0, Some(1), Some(32));
        assert_eq!(prp.x_page, 1);
        assert_eq!(prp.x_per_page, 32);
        assert_eq!(prp.start(), 0);
        assert_eq!(prp.end(), 0, "{prp:?}");
    }
}

pub struct DateRange {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}

impl Debug for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}
impl Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}, {}]",
            self.since
                .map_or_else(|| "-∞".to_string(), |x| format!("{x}")),
            self.until
                .map_or_else(|| "∞".to_string(), |x| format!("{x}"))
        )
    }
}

impl
    From<(
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> for DateRange
{
    fn from(
        value: (
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    ) -> Self {
        Self {
            since: value.0,
            until: value.1,
        }
    }
}
#[must_use]
pub fn find_applicable_date_range(
    y: Option<i32>,
    m: Option<u32>,
    d: Option<u32>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    ifmodsince: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<DateRange> {
    let (ymd_since, ymd_until) = match (y, m, d) {
        (Some(y), Some(m), Some(d)) => Some((
            NaiveDate::from_ymd_opt(y, m, d),
            NaiveDate::from_ymd_opt(y, m, d),
        )),
        (Some(y), Some(m), None) => Some((
            NaiveDate::from_ymd_opt(y, m, 1),
            #[allow(
                clippy::arithmetic_side_effects,
                reason = "Month wrap around would result in m == 0 wich would return None anyways. Year would be caught by until being before since.")
            ]
            NaiveDate::from_ymd_opt(y, m + 1, 1)
                .or_else(|| NaiveDate::from_ymd_opt(y + 1, 1, 1))
                .and_then(|date| date.pred_opt()),
        )),
        (Some(y), None, _) => Some((
            NaiveDate::from_ymd_opt(y, 1, 1),
            NaiveDate::from_ymd_opt(y, 12, 31),
        )),
        _ => None,
    }
    .map_or((None, None), |(ymd_since, ymd_until): (Option<NaiveDate>, Option<NaiveDate>)| {
        (
            ymd_since.and_then(|date| date.and_hms_opt(0, 0, 0)).map(|dt| dt.and_utc()),
            ymd_until.and_then(|date| date.and_hms_opt(23, 59, 59)).map(|dt| dt.and_utc()),
        )
    });

    #[rustfmt::skip]
    let since = match (since, ifmodsince, ymd_since) {
        (Some(since), Some(ifmodsince), Some(ymd_since)) => Some(since.max(ifmodsince).max(ymd_since)),
        (Some(since), Some(ifmodsince), None           ) => Some(since.max(ifmodsince)),
        (Some(since), None,             Some(ymd_since)) => Some(since.max(ymd_since)),
        (None,        Some(ifmodsince), Some(ymd_since)) => Some(ifmodsince.max(ymd_since)),
        (Some(since), None,             None           ) => Some(since),
        (None,        Some(ifmodsince), None           ) => Some(ifmodsince),
        (None,        None,             Some(ymd_since)) => Some(ymd_since),  
        (None,        None,             None           ) => None,
    };

    #[rustfmt::skip]
    let until = match (until, ymd_until) {
        (Some(until), Some(ymd_until)) => Some(until.min(ymd_until)),
        (Some(until), None                          ) => Some(until),
        (None,                       Some(ymd_until)) => Some(ymd_until),
        (None,                       None                          ) => None,
    };

    if since.is_some_and(|since| since.year() < 1945) {
        return None;
    }

    if since.is_some_and(|since| until.is_some_and(|until| since >= until)) {
        return None;
    }

    Some(DateRange { since, until })
}

#[cfg(test)]
mod test_applicable_date_range {
    use super::{DateRange, find_applicable_date_range};
    use chrono::DateTime;

    #[test]
    fn test_date_range_none() {
        let result = find_applicable_date_range(None, None, None, None, None, None);
        assert!(
            matches!(
                result,
                Some(DateRange {
                    since: None,
                    until: None
                })
            ),
            "None dates should not fail but produce (None, None)"
        );
    }
    #[test]
    fn test_date_range_untilsince() {
        let since = DateTime::parse_from_rfc3339("1960-01-01T00:00:00+00:00")
            .expect("Failed to parse static datetime string")
            .to_utc();
        let until = DateTime::parse_from_rfc3339("1960-01-02T00:00:00+00:00")
            .expect("Failed to parse static datetime string")
            .to_utc();
        let result = find_applicable_date_range(None, None, None, Some(since), Some(until), None);
        assert!(
            matches!(
                result,
                Some(DateRange {
                    since: Some(inner_since),
                    until: Some(inner_until),
                })
                if inner_since == since && inner_until == until
            ),
            "Since and until should yield (since, until)"
        );
    }
    #[test]
    fn test_date_range_ymd() {
        let y = 2012;
        let m = 5;
        let d = 12;

        // ymd
        let expected_since = chrono::NaiveDate::from_ymd_opt(y, m, d)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc());
        let expected_until = chrono::NaiveDate::from_ymd_opt(y, m, d)
            .and_then(|date| date.and_hms_opt(23, 59, 59))
            .map(|datetime| datetime.and_utc());
        if let Some(result) =
            find_applicable_date_range(Some(y), Some(m), Some(d), None, None, None)
        {
            assert!(
                result.since == expected_since && result.until == expected_until,
                "ymd should start and end at the date range"
            );
        } else {
            panic!("Failed to get applicable date range based on year, month and day.");
        }
        // ym
        let expected_since = chrono::NaiveDate::from_ymd_opt(y, m, 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc());
        let expected_until = chrono::NaiveDate::from_ymd_opt(y, m, 31)
            .and_then(|date| date.and_hms_opt(23, 59, 59))
            .map(|datetime| datetime.and_utc());
        if let Some(result) = find_applicable_date_range(Some(y), Some(m), None, None, None, None) {
            assert!(
                result.since == expected_since && result.until == expected_until,
                "ymd should start and end at the date range"
            );
        } else {
            panic!("Failed to get applicable date range based on year and month.")
        }
        // y
        let expected_since = chrono::NaiveDate::from_ymd_opt(y, 1, 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc());
        let expected_until = chrono::NaiveDate::from_ymd_opt(y, 12, 31)
            .and_then(|date| date.and_hms_opt(23, 59, 59))
            .map(|datetime| datetime.and_utc());

        if let Some(result) = find_applicable_date_range(Some(y), None, None, None, None, None) {
            assert!(
                result.since == expected_since && result.until == expected_until,
                "ymd should start and end at the date range"
            );
        } else {
            panic!("Failed to get applicable date range based on year.")
        }
    }

    #[test]
    fn test_minmax() {
        let y = 2012;

        let since = chrono::NaiveDate::from_ymd_opt(2000, 3, 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc())
            .expect("Failed to build `since` datetime.");
        let until = chrono::NaiveDate::from_ymd_opt(2012, 7, 31)
            .and_then(|date| date.and_hms_opt(15, 59, 59))
            .map(|datetime| datetime.and_utc())
            .expect("Failed to build `until` datetime.");
        let expected_since = chrono::NaiveDate::from_ymd_opt(2012, 1, 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc());
        let expected_until = chrono::NaiveDate::from_ymd_opt(2012, 7, 31)
            .and_then(|date| date.and_hms_opt(15, 59, 59))
            .map(|datetime| datetime.and_utc());
        if let Some(result) =
            find_applicable_date_range(Some(y), None, None, Some(since), Some(until), None)
        {
            assert_eq!(result.since, expected_since);
            assert_eq!(result.until, expected_until);
        } else {
            panic!("Failed to get applicable date range based on combined parameters.")
        }
    }
}

/// This trait enables sorting all arrays contained in an object
/// to be able to compare them afterwards without caring for ordering
#[cfg(test)]
pub(crate) trait SortArrays: Clone {
    fn sort_arrays(&mut self);
}

#[cfg(test)]
impl SortArrays for models::Dokument {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        self.schlagworte.sort();
        self.touched_by.sort_by(|a, b| {
            (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
        });

        self.autoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
    }
}
#[cfg(test)]
impl SortArrays for models::Station {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        self.schlagworte.sort();
        self.touched_by.sort_by(|a, b| {
            (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
        });
        self.additional_links.sort();
        self.dokumente.sort_by(|a, b| match (a, b) {
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::ApiId(y)) => x.cmp(y),
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::Dokument(y)) => {
                (*x).cmp(&y.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::ApiId(y)) => {
                (*y).cmp(&x.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::Dokument(y)) => {
                x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil))
            }
        });
        self.dokumente.iter_mut().for_each(|x| {
            if let models::DokumentOrApiId::Dokument(x) = x {
                x.sort_arrays();
            }
        });
        self.stellungnahmen.sort_by(|a, b| match (a, b) {
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::ApiId(y)) => x.cmp(y),
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::Dokument(y)) => {
                (*x).cmp(&y.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::ApiId(y)) => {
                (*y).cmp(&x.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::Dokument(y)) => {
                x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil))
            }
        });
        for x in &mut self.stellungnahmen {
            if let models::DokumentOrApiId::Dokument(x) = x {
                x.sort_arrays();
            }
        }
    }
}

#[cfg(test)]
impl SortArrays for models::Vorgang {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        self.touched_by.sort_by(|a, b| {
            (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
        });
        self.lobbyregister.sort_by(|a, b| a.link.cmp(&b.link));
        self.stationen.sort_by_key(|a| (a.zp_start, a.api_id));
        self.stationen.iter_mut().for_each(SortArrays::sort_arrays);
        self.initiatoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        self.ids
            .sort_by(|a, b| (&a.typ, &a.id).cmp(&(&b.typ, &b.id)));
        self.links.sort();
        self.sachgebiete.sort();
    }
}

#[cfg(test)]
impl SortArrays for models::Sitzung {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        self.touched_by.sort_by(|a, b| {
            (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
        });
        self.experten
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        self.dokumente.sort_by(|a, b| match (a, b) {
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::ApiId(y)) => x.cmp(y),
            (models::DokumentOrApiId::ApiId(x), models::DokumentOrApiId::Dokument(y)) => {
                (*x).cmp(&y.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::ApiId(y)) => {
                (*y).cmp(&x.api_id.unwrap_or(nil))
            }
            (models::DokumentOrApiId::Dokument(x), models::DokumentOrApiId::Dokument(y)) => {
                x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil))
            }
        });

        for x in &mut self.dokumente {
            if let models::DokumentOrApiId::Dokument(x) = x {
                x.sort_arrays();
            }
        }
        self.tops.sort_by_key(|a| a.nummer);
    }
}
