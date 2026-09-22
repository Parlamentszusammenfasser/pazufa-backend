use axum::http::{HeaderValue, StatusCode, request::Parts};
use axum::{
    extract::FromRequestParts,
    response::{IntoResponse as _, Response},
};
use serde::de::DeserializeOwned;

pub trait DeserializeFromHeaderName {
    fn header_name() -> &'static str;
}

#[derive(Debug)]
pub struct HeaderParams<T: DeserializeFromHeaderName + DeserializeOwned + TryFrom<HeaderValue>>(
    pub T,
);

impl<S, T> FromRequestParts<S> for HeaderParams<T>
where
    S: Send + Sync,
    T: DeserializeOwned + DeserializeFromHeaderName + TryFrom<HeaderValue>,
    <T as TryFrom<headers::HeaderValue>>::Error: axum::response::IntoResponse,
{
    type Rejection = Response;

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "would make the code unnecessarily unreadable"
    )]
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let inner = parts.headers.remove(T::header_name());
        let inner = inner.ok_or(
            (
                StatusCode::BAD_REQUEST,
                format!("Header Not found: {}", T::header_name()),
            )
                .into_response(),
        )?;
        Ok(HeaderParams(inner.try_into().map_err(|s| {
            (StatusCode::BAD_REQUEST, s).into_response()
        })?))
    }
}

#[cfg(test)]
mod test {
    use super::FromRequestParts;
    use crate::{
        models::params::{XAPIKey, XScraperId},
        utils::header_params::HeaderParams,
    };
    use axum::extract::Request;
    use uuid::uuid;

    #[tokio::test]
    async fn test_xapikey() {
        let possible_values = [
            "x-api-key",
            "x-aPi-Key",
            "X-API-KEY",
            "x-API-KeY",
            "X-aPi-KeY",
        ];
        let rejected_values = ["X_API_KEY", "x-scraper-id"];
        for v in possible_values {
            let request: Request<()> = Request::builder()
                .uri("https://bughunt.lol")
                .header(v, "pjotr kaminski")
                .method(axum::http::method::Method::PUT)
                .body(())
                .expect("Expected no problems building a request");

            let mut parts = request.into_parts().0;
            let result = <HeaderParams<XAPIKey>>::from_request_parts(&mut parts, &()).await;
            assert!(
                matches!(&result, Ok(HeaderParams(XAPIKey(content))) if content == "pjotr kaminski"),
                "got: {result:?} with header: {v}"
            );
        }
        for v in rejected_values {
            let request: Request<()> = Request::builder()
                .uri("https://bughunt.lol")
                .header(v, "pjotr kaminski")
                .method(axum::http::method::Method::PUT)
                .body(())
                .expect("Expected no problems building a request");

            let mut parts = request.into_parts().0;
            let result = <HeaderParams<XAPIKey>>::from_request_parts(&mut parts, &()).await;
            assert!(result.is_err());
        }
    }
    #[tokio::test]
    async fn test_xscraperid() {
        let possible_values = [
            "x-scraper-id",
            "x-scRaPeR-iD",
            "X-SCRAPER-ID",
            "x-sCrApEr-Id",
            "X-sCRAPer-id",
        ];
        let rejected_values = ["X_SCRAPER_ID", "x-api-key"];
        for v in possible_values {
            let request: Request<()> = Request::builder()
                .uri("https://bughunt.lol")
                .header(v, "64475b2e-7442-4bd9-a477-27ce1baba21c")
                .method(axum::http::method::Method::PUT)
                .body(())
                .expect("Expected no problems building a request");

            let mut parts = request.into_parts().0;
            let result = <HeaderParams<XScraperId>>::from_request_parts(&mut parts, &()).await;
            assert!(
                matches!(&result, Ok(HeaderParams(XScraperId(content))) if content == &uuid!("64475b2e-7442-4bd9-a477-27ce1baba21c")),
                "got: {result:?} with header: {v}"
            );
        }
        for v in rejected_values {
            let request: Request<()> = Request::builder()
                .uri("https://bughunt.lol")
                .header(v, "pjotr kaminski")
                .method(axum::http::method::Method::PUT)
                .body(())
                .expect("Expected no problems building a request");

            let mut parts = request.into_parts().0;
            let result = <HeaderParams<XScraperId>>::from_request_parts(&mut parts, &()).await;
            assert!(result.is_err());
        }
    }
}
