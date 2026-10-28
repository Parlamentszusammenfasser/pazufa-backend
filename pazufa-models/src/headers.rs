use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Maximum number of requests allowed per second")]
pub struct XRateLimitLimit(#[serde(rename = "x-ratelimit-limit")] u32);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Number of requests remaining in the current rate limit period")]
pub struct XRateLimitRemaining(#[serde(rename = "x-ratelimit-remaining")] u32);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Timestamp when the rate limit will be reset.")]
pub struct XRateLimitReset(#[serde(rename = "x-ratelimit-reset")] chrono::DateTime<chrono::Utc>);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(
    description = "Used for pagination. 
        Contains links to the first, previous, next, and last pages of results.
        Format follows RFC 5988 Web Linking standard.",
    example = r#"<https://api.example.com/resources?page=3&per_page=20>; rel="next", <https://api.example.com/resources?page=5&per_page=20>; rel="last", <https://api.example.com/resources?page=1&per_page=20>; rel="first", <https://api.example.com/resources?page=1&per_page=20>; rel="prev""#
)]
pub struct Link(#[serde(rename = "link")] String);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Total number of items available.")]
pub struct XTotalCount(#[serde(rename = "x-total-count")] u32);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Total number of pages available")]
pub struct XTotalPages(#[serde(rename = "x-total-pages")] u32);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Current page number")]
pub struct XPage(#[serde(rename = "x-page")] u32);

#[derive(ToSchema, Serialize, Deserialize)]
#[schema(description = "Number of items per page")]
pub struct XPerPage(#[serde(rename = "x-per-page")] u32);
