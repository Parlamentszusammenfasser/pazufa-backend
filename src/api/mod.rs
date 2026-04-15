use chrono::{DateTime, Datelike, Utc};
use chrono::{DurationRound, NaiveDate};
use std::cmp::Ordering;
use std::fmt::{Debug, Display};
use std::sync::Arc;

use async_trait::async_trait;
use axum::response::IntoResponse;
use axum_extra::extract::CookieJar;
use headers::Host;
use openapi::models;
use tracing::{debug, instrument};

use crate::Configuration;
use crate::Result;
#[cfg(feature = "track-validation")]
use crate::db::validate::Tracks;
use crate::error::LTZFError;
use crate::utils::notify;
use crate::utils::status::Statistics;
use crate::utils::tracing::Logging;
use openapi::apis::unauthorisiert::{PingResponse, StatusResponse, Unauthorisiert};

pub(crate) mod auth;
pub(crate) mod misc;
pub(crate) mod misc_auth;
pub(crate) mod sitzung;
pub(crate) mod vorgang;

pub type Claims = (auth::APIScope, i32);

#[derive(Clone)]
pub struct LTZFServer {
    pub sqlx_db: sqlx::PgPool,
    pub mailbundle: Option<Arc<notify::MailBundle>>,
    pub config: Configuration,
    pub logging: Logging,
    pub statistics: Arc<Statistics>,
    #[cfg(feature = "track-validation")]
    pub tracks: Arc<Tracks>,
}
pub type LTZFArc = std::sync::Arc<LTZFServer>;
impl LTZFServer {
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

#[async_trait]
impl openapi::apis::ErrorHandler<LTZFError> for LTZFServer {
    #[instrument(skip_all, fields(%method))]
    async fn handle_error(
        &self,
        method: &axum::http::Method,
        _host: &Host,
        _cookies: &CookieJar,
        error: LTZFError,
    ) -> std::result::Result<axum::response::Response, axum::http::StatusCode> {
        tracing::error!("An error occurred during {method} that was not expected: {error}\n");
        match error {
            LTZFError::Validation { source } => match *source {
                crate::error::DataValidationError::MalformedVorgang { reason } => {
                    Ok((axum::http::StatusCode::BAD_REQUEST, reason).into_response())
                }
                res => Ok((
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    res.to_string(),
                )
                    .into_response()),
            },
            res => Ok((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                res.to_string(),
            )
                .into_response()),
        }
    }
}

#[async_trait]
impl Unauthorisiert<LTZFError> for LTZFServer {
    #[instrument(skip_all, fields(t=?query_params.t))]
    async fn ping(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &CookieJar,
        query_params: &models::PingQueryParams,
    ) -> Result<PingResponse> {
        if let Some(t) = query_params.t {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| LTZFError::Other {
                    message: "System time before unix epoch.".to_owned(),
                })?
                .as_secs_f64();
            debug!("Ping with one-way time: {} s", current_time - t);
        }
        Ok(PingResponse::Status200_Pong)
    }

    #[instrument(skip_all)]
    async fn status(
        &self,
        _method: &axum::http::Method,
        _host: &Host,
        _cookies: &CookieJar,
    ) -> Result<StatusResponse> {
        debug!("Status Requested");
        // TODO: implement "API is not running for some reason" markers
        let body = self.statistics.output().await;
        Ok(StatusResponse::Status200_APIIsRunning {
            body,
            x_rate_limit_limit: None,
            x_rate_limit_remaining: None,
            x_rate_limit_reset: None,
        })
    }
}
#[derive(Debug)]
pub struct PaginationResponsePart {
    pub x_total_count: i32,
    pub x_total_pages: i32,
    pub x_page: i32,
    pub x_per_page: i32,
}

fn div_ceil(dividend: i32, divisor: i32) -> Option<i32> {
    let quotient = dividend.checked_div(divisor)?;
    let remainder = dividend.checked_rem(divisor)?;
    if remainder != 0 && dividend.is_positive() == divisor.is_positive() {
        quotient.checked_add(1)
    } else {
        Some(quotient)
    }
}

impl PaginationResponsePart {
    pub const DEFAULT_PER_PAGE: i32 = 32;
    pub const MAX_PER_PAGE: i32 = 256;
    pub fn new(x_total_count: i32, x_page: Option<i32>, x_per_page: Option<i32>) -> Self {
        let x_per_page =
            x_per_page.map_or(Self::DEFAULT_PER_PAGE, |x| x.clamp(1, Self::MAX_PER_PAGE));
        let Some(x_total_pages) = div_ceil(x_total_count, x_per_page) else {
            unreachable!("x_per_page is clamped between 1 and 256.")
        };
        let x_page = x_page.map_or(1, |x| x.clamp(1, x_total_pages));

        Self {
            x_total_count,
            x_total_pages,
            x_page,
            x_per_page,
        }
    }
    pub fn limit(&self) -> i64 {
        i64::from(self.x_per_page)
    }
    pub fn offset(&self) -> i64 {
        let page_offset = i64::from(self.x_page.saturating_sub(1));
        let per_page = i64::from(self.x_per_page);

        #[allow(clippy::arithmetic_side_effects, reason = "Wide multiplication.")]
        {
            page_offset * per_page
        }
    }
    pub fn start(&self) -> usize {
        let Ok(page) = usize::try_from(self.x_page.max(0)).map(|page| page.saturating_sub(1))
        else {
            unreachable!(
                "i32 to usize only fails if the value is negative. `max(0)` forces non-negativity."
            )
        };
        let Ok(per_page) = usize::try_from(self.x_per_page.max(0)) else {
            unreachable!(
                "i32 to usize only fails if the value is negative. `max(0)` forces non-negativity."
            )
        };
        page.saturating_mul(per_page)
    }

    pub fn end(&self) -> usize {
        let Ok(offset) = usize::try_from(self.offset().max(0)) else {
            #[cfg(target_pointer_width = "64")]
            unreachable!(
                "i32 to usize only fails if the value is negative. `max(0)` forces non-negativity."
            );
            #[cfg(not(target_pointer_width = "64"))]
            panic!("Offset exceeded usize limit.");
        };
        let Ok(limit) = usize::try_from(self.limit().max(0)) else {
            #[cfg(target_pointer_width = "64")]
            unreachable!(
                "i32 to usize only fails if the value is negative. `max(0)` forces non-negativity."
            );
            #[cfg(not(target_pointer_width = "64"))]
            panic!("Offset exceeded usize limit.");
        };
        let Ok(total_count) = usize::try_from(self.x_total_count.max(0)) else {
            unreachable!(
                "i32 to usize only fails if the value is negative. `max(0)` forces non-negativity."
            )
        };
        total_count.min(offset.saturating_add(limit))
    }
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

#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
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

    #[allow(unused, reason = "Temp. example")]
    {
        let since2 = [since, ifmodsince, ymd_since].into_iter().flatten().max();
        let until2 = [until, ymd_until].into_iter().flatten().min();
    }

    #[rustfmt::skip]
    let since = match (since, ifmodsince, ymd_since) {
        (Some(since),    Some(ifmodsince), Some(ymd_since) ) => Some(since.max(ifmodsince).max(ymd_since)),
        (Some(since),    Some(ifmodsince), None                          ) => Some(since.max(ifmodsince)),
        (Some(since),    None,                            Some(ymd_since)) => Some(since.max(ymd_since)),
        (None,                          Some(ifmodsince), Some(ymd_since)) => Some(ifmodsince.max(ymd_since)),
        (Some(since),    None,                            None                          ) => Some(since),
        (None,                          Some(ifmodsince), None                          ) => Some(ifmodsince),
        (None,                          None,                            Some(ymd_since)) => Some(ymd_since),  
        (None,                          None,                            None                          ) => None,
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

#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
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
            assert!(result.since == expected_since);
            assert!(result.until == expected_until);
        } else {
            panic!("Failed to get applicable date range based on combined parameters.")
        }
    }
}

/// this is here to implement a `PartialEq`, Eq, Ord, ...
/// for hashing, since we need t
#[derive(Debug, Clone)]
pub(crate) struct WrappedAutor<'wrapped> {
    pub autor: &'wrapped models::Autor,
}
impl PartialEq for WrappedAutor<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.autor.organisation == other.autor.organisation
            && self.autor.person == other.autor.person
    }
}
impl Eq for WrappedAutor<'_> {}
impl PartialOrd for WrappedAutor<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for WrappedAutor<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.autor
            .organisation
            .cmp(&other.autor.organisation)
            .then(self.autor.person.cmp(&other.autor.person))
    }
}

/// This trait enables sorting all arrays contained in an object
/// to be able to compare them afterwards without caring for ordering
#[cfg(test)]
#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
pub(crate) trait SortArrays: Clone {
    fn sort_arrays(&mut self);
}

#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
#[cfg(test)]
impl SortArrays for models::Dokument {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        if let Some(x) = self.schlagworte.as_mut() {
            x.sort();
        }
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            });
        }
        self.autoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
    }
}
#[cfg(test)]
#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
impl SortArrays for models::Station {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        if let Some(x) = self.schlagworte.as_mut() {
            x.sort();
        }
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            });
        }
        if let Some(x) = self.additional_links.as_mut() {
            x.sort();
        }
        self.dokumente.sort_by(|a, b| match (a, b) {
            (
                models::StationDokumenteInner::String(x),
                models::StationDokumenteInner::String(y),
            ) => x.cmp(y),
            (
                models::StationDokumenteInner::String(x),
                models::StationDokumenteInner::Dokument(y),
            ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
            (
                models::StationDokumenteInner::Dokument(x),
                models::StationDokumenteInner::String(y),
            ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
            (
                models::StationDokumenteInner::Dokument(x),
                models::StationDokumenteInner::Dokument(y),
            ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
        });
        self.dokumente.iter_mut().for_each(|x| {
            if let models::StationDokumenteInner::Dokument(x) = x {
                x.sort_arrays();
            }
        });
        if let Some(x) = self.stellungnahmen.as_mut() {
            x.sort_by(|a, b| match (a, b) {
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::String(y),
                ) => x.cmp(y),
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::String(y),
                ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
            });
            for x in x {
                if let models::StationDokumenteInner::Dokument(x) = x {
                    x.sort_arrays();
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
impl SortArrays for models::Vorgang {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            });
        }
        if let Some(x) = self.lobbyregister.as_mut() {
            x.sort_by(|a, b| a.link.cmp(&b.link));
        }
        self.stationen
            .sort_by(|a, b| (a.zp_start, a.api_id).cmp(&(b.zp_start, b.api_id)));
        self.stationen.iter_mut().for_each(SortArrays::sort_arrays);
        self.initiatoren
            .sort_by(|a, b| a.organisation.cmp(&b.organisation));
        if let Some(x) = self.ids.as_mut() {
            x.sort_by(|a, b| (&a.typ, &a.id).cmp(&(&b.typ, &b.id)));
        }
        if let Some(x) = self.links.as_mut() {
            x.sort();
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "Panic is allowed in test code.")]
impl SortArrays for models::Sitzung {
    fn sort_arrays(&mut self) {
        let nil = uuid::Uuid::nil();
        let emp = String::new();
        if let Some(x) = self.touched_by.as_mut() {
            x.sort_by(|a, b| {
                (a.key.as_ref().unwrap_or(&emp), a.scraper_id.unwrap_or(nil))
                    .cmp(&(b.key.as_ref().unwrap_or(&emp), b.scraper_id.unwrap_or(nil)))
            });
        }
        if let Some(x) = self.experten.as_mut() {
            x.sort_by(|a, b| a.organisation.cmp(&b.organisation));
        }

        if let Some(x) = self.dokumente.as_mut() {
            x.sort_by(|a, b| match (a, b) {
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::String(y),
                ) => x.cmp(y),
                (
                    models::StationDokumenteInner::String(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => (**x).cmp(&y.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::String(y),
                ) => (**y).cmp(&x.api_id.unwrap_or(nil).to_string()),
                (
                    models::StationDokumenteInner::Dokument(x),
                    models::StationDokumenteInner::Dokument(y),
                ) => x.api_id.unwrap_or(nil).cmp(&y.api_id.unwrap_or(nil)),
            });

            for x in x {
                if let models::StationDokumenteInner::Dokument(x) = x {
                    x.sort_arrays();
                }
            }
        }
        self.tops.sort_by(|a, b| a.nummer.cmp(&b.nummer));
    }
}
/// Helper Trait that allows me to compare objects (vorgang, dokument, ...)
/// that are stored and re-fetched with whatever precision where only the
/// very very margins differ by a few nanosecs.
/// Using this trait the thing can round its dates to a precision of 1 second
/// We do not really need more
pub(crate) trait RoundTimestamp: Clone {
    fn with_round_timestamps(&self) -> Self;
}

fn round_to_second(datetime: DateTime<Utc>) -> DateTime<Utc> {
    if let Ok(datetime) = datetime.duration_round(const { chrono::Duration::seconds(1) }) {
        datetime
    } else {
        let mut unix_time = datetime.timestamp();
        if datetime.timestamp_subsec_millis() >= 500 {
            unix_time = unix_time.saturating_add(1);
        }
        let Some(datetime) = DateTime::from_timestamp(unix_time, 0) else {
            unreachable!("Only returns None, if `nsecs` is greater then 999,999,999.");
        };
        datetime
    }
}

impl RoundTimestamp for models::Dokument {
    fn with_round_timestamps(&self) -> Self {
        Self {
            zp_referenz: round_to_second(self.zp_referenz),
            zp_erstellt: self.zp_erstellt.map(round_to_second),
            zp_modifiziert: round_to_second(self.zp_modifiziert),
            ..self.clone()
        }
    }
}

impl RoundTimestamp for models::Station {
    fn with_round_timestamps(&self) -> Self {
        Self {
            zp_modifiziert: self.zp_modifiziert.map(round_to_second),
            zp_start: round_to_second(self.zp_start),
            stellungnahmen: self.stellungnahmen.as_ref().map(|v| {
                v.iter()
                    .map(|sn| match sn {
                        models::StationDokumenteInner::Dokument(d) => {
                            models::StationDokumenteInner::Dokument(d.with_round_timestamps())
                        }
                        models::StationDokumenteInner::String(x) => {
                            models::StationDokumenteInner::String(x.clone())
                        }
                    })
                    .collect()
            }),
            dokumente: self
                .dokumente
                .iter()
                .map(|sn| match sn {
                    models::StationDokumenteInner::Dokument(d) => {
                        models::StationDokumenteInner::Dokument(d.with_round_timestamps())
                    }
                    models::StationDokumenteInner::String(x) => {
                        models::StationDokumenteInner::String(x.clone())
                    }
                })
                .collect(),

            ..self.clone()
        }
    }
}

impl RoundTimestamp for models::Vorgang {
    fn with_round_timestamps(&self) -> Self {
        Self {
            stationen: self
                .stationen
                .iter()
                .map(RoundTimestamp::with_round_timestamps)
                .collect(),
            ..self.clone()
        }
    }
}
impl RoundTimestamp for models::Sitzung {
    fn with_round_timestamps(&self) -> Self {
        Self {
            dokumente: self.dokumente.as_ref().map(|v| {
                v.iter()
                    .map(|sn| match sn {
                        models::StationDokumenteInner::Dokument(d) => {
                            models::StationDokumenteInner::Dokument(d.with_round_timestamps())
                        }
                        models::StationDokumenteInner::String(x) => {
                            models::StationDokumenteInner::String(x.clone())
                        }
                    })
                    .collect()
            }),
            termin: round_to_second(self.termin),
            ..self.clone()
        }
    }
}
