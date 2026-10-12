//! Strong `ETag`s derived from the response body, and `If-None-Match` handling based on them.
//!
//! A strong `ETag` has to differ between the content codings of a representation
//! (RFC 9110 §8.8.3.3), yet it should not change just because the compressor did.
//! The work is therefore split into two layers that have to enclose the compression layer:
//!
//! ```text
//! ConditionalLayer -> CompressionLayer -> HashLayer -> handler
//! ```
//!
//! [`HashLayer`] hashes the uncompressed body into `"<sha256>"`. [`ConditionalLayer`] appends the
//! content coding the compression chose (`"<sha256>-br"`) and answers a matching `If-None-Match`
//! with `304 Not Modified`. As the compressed body is produced lazily, no compression takes place
//! for a `304`.
//!
//! [`Docs`] adds the resulting headers to the `OpenAPI` specification.
//!
//! The tags are strong validators, which leaves the door open to `If-Match` preconditions.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Method, Request, Response, StatusCode, header};
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use utoipa::PartialSchema;
use utoipa::openapi::path::{Operation, ParameterBuilder, ParameterIn};
use utoipa::openapi::{HeaderBuilder, RefOr, Required};

type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

/// Adds an `ETag` to every `200` response of a `GET` request that does not carry one yet.
/// Buffers the response body to do so.
#[derive(Debug, Clone, Copy, Default)]
pub struct HashLayer;

impl<S> Layer<S> for HashLayer {
    type Service = Hash<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Hash { inner }
    }
}

#[derive(Debug, Clone)]
pub struct Hash<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for Hash<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // keep the instance that was polled ready, leave a fresh clone behind
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let is_get = req.method() == Method::GET;
        Box::pin(async move {
            let res = inner.call(req).await?;
            if !is_get || res.status() != StatusCode::OK || res.headers().contains_key(header::ETAG)
            {
                return Ok(res);
            }
            let (mut parts, body) = res.into_parts();
            let bytes = match to_bytes(body, usize::MAX).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::error!("Failed to read the response body for hashing: {e}");
                    let mut res = Response::new(Body::empty());
                    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    return Ok(res);
                }
            };
            let etag = format!("\"{}\"", sha256::digest(bytes.as_ref()));
            if let Ok(etag) = HeaderValue::from_str(&etag) {
                parts.headers.insert(header::ETAG, etag);
            }
            Ok(Response::from_parts(parts, Body::from(bytes)))
        })
    }
}

/// Makes the `ETag` of a response specific to its content coding and answers `GET` requests whose
/// `If-None-Match` matches it with `304 Not Modified`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConditionalLayer;

impl<S> Layer<S> for ConditionalLayer {
    type Service = Conditional<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Conditional { inner }
    }
}

#[derive(Debug, Clone)]
pub struct Conditional<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for Conditional<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let if_none_match: Vec<HeaderValue> = if req.method() == Method::GET {
            req.headers()
                .get_all(header::IF_NONE_MATCH)
                .iter()
                .cloned()
                .collect()
        } else {
            vec![]
        };
        Box::pin(async move {
            let mut res = inner.call(req).await?;
            if res.status() != StatusCode::OK {
                return Ok(res);
            }
            let Some(mut etag) = res.headers().get(header::ETAG).cloned() else {
                return Ok(res);
            };
            if let Some(specific) =
                with_content_coding(&etag, res.headers().get(header::CONTENT_ENCODING))
            {
                res.headers_mut().insert(header::ETAG, specific.clone());
                etag = specific;
            }
            if !matches_any(&if_none_match, &etag) {
                return Ok(res);
            }
            // dropping the body here means that it is never compressed
            let (mut parts, _) = res.into_parts();
            parts.status = StatusCode::NOT_MODIFIED;
            for name in [
                header::CONTENT_LENGTH,
                header::CONTENT_ENCODING,
                header::CONTENT_TYPE,
            ] {
                parts.headers.remove(name);
            }
            Ok(Response::from_parts(parts, Body::empty()))
        })
    }
}

/// Appends the content coding to the opaque part of `etag`: `"abc"` + `br` -> `"abc-br"`.
/// Returns `None` if there is nothing to append.
fn with_content_coding(etag: &HeaderValue, coding: Option<&HeaderValue>) -> Option<HeaderValue> {
    let coding: String = coding?
        .to_str()
        .ok()?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if coding.is_empty() || coding.eq_ignore_ascii_case("identity") {
        return None;
    }
    let opaque = etag.to_str().ok()?.strip_suffix('"')?;
    HeaderValue::from_str(&format!("{opaque}-{coding}\"")).ok()
}

/// Whether one of the `If-None-Match` header values contains `etag` (or `*`).
/// This is the weak comparison of RFC 9110 §8.8.3.2, as mandated for `If-None-Match`.
fn matches_any(if_none_match: &[HeaderValue], etag: &HeaderValue) -> bool {
    fn opaque(tag: &str) -> &str {
        tag.strip_prefix("W/").unwrap_or(tag)
    }
    let Ok(etag) = etag.to_str() else {
        return false;
    };
    if_none_match
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|candidate| candidate == "*" || opaque(candidate) == opaque(etag))
}

/// Documents what the layers of this module do for every `GET` operation with a `200` response:
/// the `if-none-match` request header, the `etag` response header and the `304` response.
pub struct Docs;

impl utoipa::Modify for Docs {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        for item in openapi.paths.paths.values_mut() {
            if let Some(operation) = item.get.as_mut() {
                document(operation);
            }
        }
    }
}

fn document(operation: &mut Operation) {
    const IF_NONE_MATCH: &str = "if-none-match";
    const ETAG: &str = "etag";

    let responses = &mut operation.responses.responses;
    if !responses.contains_key("200") {
        return;
    }
    responses
        .entry("304".to_owned())
        .or_insert_with(|| RefOr::T(utoipa::openapi::Response::new("Not Modified")));
    for status in ["200", "304"] {
        if let Some(RefOr::T(response)) = responses.get_mut(status) {
            response.headers.entry(ETAG.to_owned()).or_insert_with(|| {
                utoipa::openapi::RefOr::T(
                    HeaderBuilder::new()
                        .schema(Some(String::schema()))
                        .description(Some(
                            "Strong entity tag of the returned representation. \
                        It is specific to the content coding of the response.",
                        ))
                        .build(),
                )
            });
        }
    }

    let parameters = operation.parameters.get_or_insert_default();
    let documented = parameters.iter().any(|p| {
        matches!(p, RefOr::T(p) if p.parameter_in == ParameterIn::Header
            && p.name.eq_ignore_ascii_case(IF_NONE_MATCH))
    });
    if !documented {
        parameters.push(utoipa::openapi::RefOr::T(
            ParameterBuilder::new()
                .name(IF_NONE_MATCH)
                .parameter_in(ParameterIn::Header)
                .required(Required::False)
                .description(Some(
                    "One or more entity tags from the `etag` header of earlier responses. \
                    If one of them matches the current representation, \
                    the server responds with `304 Not Modified` and an empty body.",
                ))
                .schema(Some(String::schema()))
                .build(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;
    use tower_http::compression::CompressionLayer;
    use utoipa::Modify;

    const BODY: &str = "a body that is long enough for the compression layer to care about";

    fn app() -> Router {
        Router::new()
            .route("/", get(|| async { BODY }).post(|| async { BODY }))
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }))
            .layer(HashLayer)
            .layer(CompressionLayer::new())
            .layer(ConditionalLayer)
    }

    async fn call(method: Method, uri: &str, headers: &[(&str, &str)]) -> (Response<Body>, String) {
        let mut req = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let req = req.body(Body::empty()).expect("Failed to build request");
        let Ok(res) = app().call(req).await;
        let (parts, body) = res.into_parts();
        let bytes = to_bytes(body, usize::MAX)
            .await
            .expect("Failed to read body");
        (
            Response::from_parts(parts, Body::empty()),
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    fn etag(res: &Response<Body>) -> Option<&str> {
        res.headers()
            .get(header::ETAG)
            .map(|v| v.to_str().expect("etag is not a string"))
    }

    #[tokio::test]
    async fn test_etag_is_hash_of_body() {
        let (res, body) = call(Method::GET, "/", &[]).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body, BODY);
        assert_eq!(
            etag(&res),
            Some(format!("\"{}\"", sha256::digest(BODY)).as_str())
        );
    }

    #[tokio::test]
    async fn test_etag_is_specific_to_content_coding() {
        let plain = format!("\"{}\"", sha256::digest(BODY));
        for coding in ["gzip", "br", "zstd", "deflate"] {
            let (res, _) = call(Method::GET, "/", &[("accept-encoding", coding)]).await;
            assert_eq!(
                res.headers()
                    .get(header::CONTENT_ENCODING)
                    .expect("response is not compressed"),
                coding
            );
            assert_eq!(
                etag(&res),
                Some(format!("\"{}-{coding}\"", sha256::digest(BODY)).as_str())
            );

            // the tag of another coding is not a match
            let (res, _) = call(
                Method::GET,
                "/",
                &[("accept-encoding", coding), ("if-none-match", &plain)],
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_if_none_match() {
        let (res, _) = call(Method::GET, "/", &[("accept-encoding", "gzip")]).await;
        let tag = etag(&res).expect("no etag").to_owned();

        for candidate in [
            tag.clone(),
            format!("W/{tag}"),
            format!("\"something-else\", {tag}"),
            "*".to_owned(),
        ] {
            let (res, body) = call(
                Method::GET,
                "/",
                &[("accept-encoding", "gzip"), ("if-none-match", &candidate)],
            )
            .await;
            assert_eq!(res.status(), StatusCode::NOT_MODIFIED, "for `{candidate}`");
            assert_eq!(body, "");
            assert_eq!(etag(&res), Some(tag.as_str()));
            assert!(!res.headers().contains_key(header::CONTENT_ENCODING));
            assert!(res.headers().contains_key(header::VARY));
        }

        let (res, body) = call(Method::GET, "/", &[("if-none-match", "\"something-else\"")]).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body, BODY);
    }

    #[tokio::test]
    async fn test_only_successful_get_is_tagged() {
        let (res, body) = call(Method::POST, "/", &[("if-none-match", "*")]).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body, BODY);
        assert_eq!(etag(&res), None);

        let (res, _) = call(Method::GET, "/missing", &[("if-none-match", "*")]).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(etag(&res), None);
    }

    fn assert_documented(api: &utoipa::openapi::OpenApi) {
        let operations: Vec<_> = api
            .paths
            .paths
            .iter()
            .filter_map(|(path, item)| Some((path, item.get.as_ref()?)))
            .filter(|(_, op)| op.responses.responses.contains_key("200"))
            .collect();
        assert!(!operations.is_empty(), "no GET operation found");
        for (path, op) in operations {
            let if_none_match = op
                .parameters
                .iter()
                .flatten()
                .filter(|p| matches!(p, RefOr::T(p) if p.name == "if-none-match"))
                .count();
            assert_eq!(if_none_match, 1, "if-none-match of GET {path}");
            for status in ["200", "304"] {
                let Some(RefOr::T(response)) = op.responses.responses.get(status) else {
                    panic!("GET {path} has no inline {status} response");
                };
                assert!(
                    response.headers.contains_key("etag"),
                    "etag of GET {path} {status}"
                );
            }
        }
    }

    #[test]
    fn test_spec_documents_etags() {
        use utoipa::OpenApi;
        // what `--dump-spec` prints
        let mut dumped = crate::openapi::PazufAPI::openapi();
        assert_documented(&dumped);
        // what is served
        assert_documented(&crate::api_routes().1);

        // applying it twice does not duplicate anything
        Docs.modify(&mut dumped);
        assert_documented(&dumped);
    }
}
