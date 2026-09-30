use crate::models;
use crate::utils::header_params::DeserializeFromHeaderName;
use axum::http::HeaderValue;
use chrono;
use serde::{Deserialize, Serialize, de, ser};
use utoipa::{IntoParams, ToSchema};
use uuid;

#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema, IntoParams)]
#[into_params(names("X-API-Key"), parameter_in = Header)]
pub struct XAPIKey(pub String);
impl DeserializeFromHeaderName for XAPIKey {
    fn header_name() -> &'static str {
        "x-api-key"
    }
}
impl TryFrom<HeaderValue> for XAPIKey {
    type Error = String;
    fn try_from(value: HeaderValue) -> Result<Self, Self::Error> {
        Ok(Self(value.to_str().map_err(|e| e.to_string())?.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(names("x-scraper-id"), parameter_in = Header)]
pub struct XScraperId(pub uuid::Uuid);
impl DeserializeFromHeaderName for XScraperId {
    fn header_name() -> &'static str {
        "x-scraper-id"
    }
}
impl TryFrom<HeaderValue> for XScraperId {
    type Error = String;
    fn try_from(value: HeaderValue) -> Result<Self, Self::Error> {
        Ok(Self(
            uuid::Uuid::parse_str(value.to_str().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        ))
    }
}

/// The Keytag of the key that you are trying to delete
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(names("api-key-delete"), parameter_in = Header)]
pub struct ApiKeyDelete(pub String);

impl DeserializeFromHeaderName for ApiKeyDelete {
    fn header_name() -> &'static str {
        "api-key-delete"
    }
}
impl TryFrom<HeaderValue> for ApiKeyDelete {
    type Error = String;
    fn try_from(value: HeaderValue) -> Result<Self, Self::Error> {
        Ok(Self(value.to_str().map_err(|e| e.to_string())?.to_owned()))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Header)]
pub struct IfModSince {
    #[serde(rename = "if-modified-since")]
    pub if_modified_since: Option<chrono::DateTime<chrono::Utc>>,
}
impl TryFrom<Option<&HeaderValue>> for IfModSince {
    type Error = String;
    fn try_from(value: Option<&HeaderValue>) -> Result<Self, Self::Error> {
        let Some(Ok(dt_str)) = value.map(|x| x.to_str()) else {
            return Ok(Self {
                if_modified_since: None,
            });
        };

        match chrono::DateTime::parse_from_rfc3339(dt_str) {
            Ok(if_modified_since) => Ok(Self {
                if_modified_since: Some(if_modified_since.to_utc()),
            }),
            Err(e3339) => match chrono::DateTime::parse_from_rfc2822(dt_str) {
                Ok(ims) => Ok(Self {
                    if_modified_since: Some(ims.to_utc()),
                }),
                Err(e2282) => Err(format!(
                    "Parsing of If Modified Since header failed as rfc3339/rfc2822: `{e3339}`/`{e2282}`"
                )),
            },
        }
    }
}
#[cfg(test)]
mod test {
    use axum::http::HeaderValue;

    #[test]
    fn ims_from_header_value() {
        let rfc2822_datestring = HeaderValue::from_static("Wed, 18 Feb 2015 23:16:09 GMT");
        let rfc3339_datestring = HeaderValue::from_static("2015-02-18T23:16:09Z");
        let ims_2822 = super::IfModSince::try_from(Some(&rfc2822_datestring));
        let ims_3339 = super::IfModSince::try_from(Some(&rfc3339_datestring));
        assert_eq!(
            ims_2822.expect("successful parsing").if_modified_since,
            ims_3339.expect("successful parsing").if_modified_since
        );
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct AuthListingKeytagPathParams {
    pub keytag: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoParams, ToSchema, Default)]
#[into_params(parameter_in = Query)]
pub struct ListingQueryParams {
    /// Filter results to include only items updated on or after this timestamp (inclusive lower bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter results to include only items updated on or before this timestamp (inclusive upper bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoParams, ToSchema, Default)]
#[into_params(parameter_in = Query)]
pub struct AutorenDeleteByParamQueryParams {
    /// Filter by author name substring. Returns items where an author's name contains this string (case-insensitive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Filter by author's professional field. Returns items where an author has the specified professional expertise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fach: Option<String>,
    /// Filter by author organization. Returns items where an author belongs to the specified organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoParams, ToSchema, Default)]
#[into_params(parameter_in = Query)]
pub struct GremienDeleteByParamQueryParams {
    /// Filter by committee name substring. Returns committees whose names contain this string (case-insensitive). Used in conjunction with parlament parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gr: Option<String>,
    /// Filter by parliament code. Returns only items related to the specified parliament (e.g., 'BT' for Bundestag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<models::Parlament>,
    /// Filter by electoral period number. Returns only items from the specified electoral period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Path)]
pub struct EnumDeletePathParams {
    /// The exact value to remove from the enumeration (case-sensitive)
    pub item: String,
    /// Name of the enumeration type to remove a value from
    pub name: models::EnumerationNames,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct AuthListingQueryParams {
    /// Filter results to include only items updated on or after this timestamp (inclusive lower bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter results to include only items updated on or before this timestamp (inclusive upper bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct KalDatePutPathParams {
    /// Code of the parliament to add sessions for (e.g., 'BT' for Bundestag)
    pub parlament: models::Parlament,
    /// Date to add sessions for, in ISO 8601 format (YYYY-MM-DD)
    pub datum: chrono::naive::NaiveDate,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ApiIdPathParam {
    /// Unique identifier (UUID) of the object to identify
    pub api_id: uuid::Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct AutorenGetQueryParams {
    /// Filter by author name substring. Returns items where an author's name contains this string (case-insensitive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Filter by author's professional field. Returns items where an author has the specified professional expertise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fach: Option<String>,
    /// Filter by author organization. Returns items where an author belongs to the specified organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct EnumPathParams {
    /// Name of the enumeration type to update values for
    pub name: models::EnumerationNames,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct EnumGetQueryParams {
    /// Filter by substring match. Returns items containing this string (case-insensitive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct GremienGetQueryParams {
    /// Filter by committee name substring. Returns committees whose names contain this string (case-insensitive). Used in conjunction with parlament parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gr: Option<String>,
    /// Filter by parliament code. Returns only items related to the specified parliament (e.g., 'BT' for Bundestag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<models::Parlament>,
    /// Filter by electoral period number. Returns only items from the specified electoral period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp: Option<u16>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct KalDateGetPathParams {
    /// Code of the parliament to query sessions for (e.g., 'BT' for Bundestag)
    pub parlament: models::Parlament,
    /// Date to retrieve sessions for, in ISO 8601 format (YYYY-MM-DD)
    pub datum: chrono::naive::NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct KalDateGetQueryParams {
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
#[serde(remote = "Self")]
pub struct KalGetQueryParams {
    /// Filter results to include only items updated on or after this timestamp (inclusive lower bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter results to include only items updated on or before this timestamp (inclusive upper bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter by parliament code. Returns only items related to the specified parliament (e.g., 'BT' for Bundestag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<models::Parlament>,
    /// Filter by electoral period number. Returns only items from the specified electoral period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp: Option<u16>,
    /// Filter by committee name substring. Returns committees whose names contain this string (case-insensitive). Used in conjunction with parlament parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gr: Option<String>,
    /// The year to filter results by (1945 or later)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1945)]
    pub y: Option<i32>,
    /// The month to filter results by (1-12)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1, maximum = 12)]
    pub m: Option<u32>,
    /// The day of month to filter results by (1-31)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1, maximum = 31)]
    pub dom: Option<u32>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}
impl<'de> Deserialize<'de> for KalGetQueryParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // constraint y(>1944)
        if let Some(y) = unchecked.y
            && y < 1945
        {
            return Err(de::Error::custom("Error: Year must be >= 1945".to_owned()));
        }
        // constraint dom(<31)
        if let Some(m) = unchecked.m
            && !(1..=12).contains(&m)
        {
            return Err(de::Error::custom(
                "Error: Month must be between 1 and 12".to_owned(),
            ));
        }
        // constraint m(1<=m<=12)
        if let Some(dom) = unchecked.dom
            && !(1..=31).contains(&dom)
        {
            return Err(de::Error::custom(
                "Error: DoM must be in valid value range".to_owned(),
            ));
        }
        Ok(unchecked)
    }
}
impl Serialize for KalGetQueryParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct SGetQueryParams {
    /// Filter results to include only items updated on or after this timestamp (inclusive lower bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter results to include only items updated on or before this timestamp (inclusive upper bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
    /// Filter by parliament code. Returns only items related to the specified parliament (e.g., 'BT' for Bundestag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<models::Parlament>,
    /// Filter by electoral period number. Returns only items from the specified electoral period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp: Option<u16>,
    /// Filter by committee name substring. Returns committees whose names contain this string (case-insensitive). Used in conjunction with parlament parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gr: Option<String>,
    /// Filter by associated legislative process ID. Returns only items related to the specified process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vgid: Option<uuid::Uuid>,
    /// Filter by legislative process type. Returns only items of the specified process type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vgtyp: Option<models::Vorgangstyp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct ExpandQueryParam {
    /// if `true`, deliver documents as full, expanded objects instead of just their api id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct VorgangGetQueryParams {
    /// Filter results to include only items updated on or after this timestamp (inclusive lower bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter results to include only items updated on or before this timestamp (inclusive upper bound)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter by parliament code. Returns only items related to the specified parliament (e.g., 'BT' for Bundestag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<models::Parlament>,
    /// Filter by electoral period number. Returns only items from the specified electoral period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp: Option<u16>,
    /// Filter by author name substring. Returns items where an author's name contains this string (case-insensitive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Filter by author's professional field. Returns items where an author has the specified professional expertise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fach: Option<String>,
    /// Filter by author organization. Returns items where an author belongs to the specified organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// Filter by legislative process type. Returns only items of the specified process type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vgtyp: Option<models::Vorgangstyp>,
    /// Page number for paginated results. Starts at 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of items to return per page. Maximum value is capped at server-defined limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

/// Format of return of the documents. Defaults to pazufa
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, IntoParams, Default)]
#[into_params(parameter_in = Query)]
pub struct DocumentFormatParams {
    pub format: Option<models::enumerations::DokumentFormat>,
}
