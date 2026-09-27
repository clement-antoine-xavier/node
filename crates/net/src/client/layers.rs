//! Custom `tower` layers for the client-facing HTTP stack.

use std::convert::Infallible;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::future::Either;
use http::{Method, Request, Response};
use tower::{Layer, Service};

use crate::auth::{self, ClientIdentity};
use crate::client::handlers::{self, Body};
use crate::error::AuthError;

/// Requests to this path bypass signature verification (for probes/LBs).
pub const HEALTH_PATH: &str = "/health";

/// Rejects requests whose method is not in the allowlist with `405`.
///
/// A request filter implemented as a `tower` layer so the router itself stays
/// free of policy.
#[derive(Clone)]
pub struct MethodFilterLayer {
    allowed: Arc<[Method]>,
}

impl MethodFilterLayer {
    pub fn new(allowed: impl IntoIterator<Item = Method>) -> Self {
        Self {
            allowed: allowed.into_iter().collect(),
        }
    }
}

impl<S> Layer<S> for MethodFilterLayer {
    type Service = MethodFilter<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MethodFilter {
            inner,
            allowed: self.allowed.clone(),
        }
    }
}

/// See [`MethodFilterLayer`].
#[derive(Clone)]
pub struct MethodFilter<S> {
    inner: S,
    allowed: Arc<[Method]>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for MethodFilter<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>, Error = Infallible>,
{
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Either<S::Future, std::future::Ready<Result<Response<Body>, Infallible>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        if self.allowed.contains(request.method()) {
            Either::Left(self.inner.call(request))
        } else {
            Either::Right(std::future::ready(Ok(handlers::method_not_allowed())))
        }
    }
}

/// Verifies the Ed25519 signature on each request (except [`HEALTH_PATH`]).
///
/// On success the verified client key is inserted into the request extensions;
/// on failure the request is rejected with `401`.
#[derive(Clone)]
pub struct SignatureLayer {
    required: bool,
    max_skew: Duration,
}

impl SignatureLayer {
    pub fn new(required: bool, max_skew: Duration) -> Self {
        Self { required, max_skew }
    }
}

impl<S> Layer<S> for SignatureLayer {
    type Service = SignatureVerifier<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SignatureVerifier {
            inner,
            required: self.required,
            max_skew: self.max_skew,
        }
    }
}

/// See [`SignatureLayer`].
#[derive(Clone)]
pub struct SignatureVerifier<S> {
    inner: S,
    required: bool,
    max_skew: Duration,
}

impl<S, ReqBody> Service<Request<ReqBody>> for SignatureVerifier<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>, Error = Infallible>,
{
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Either<S::Future, std::future::Ready<Result<Response<Body>, Infallible>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        if !self.required || request.uri().path() == HEALTH_PATH {
            return Either::Left(self.inner.call(request));
        }

        let method = request.method().as_str().to_owned();
        let path_and_query = request
            .uri()
            .path_and_query()
            .map(|path| path.as_str().to_owned())
            .unwrap_or_else(|| request.uri().path().to_owned());
        let public_key = header(&request, auth::CLIENT_PUBLIC_KEY);
        let timestamp = header(&request, auth::CLIENT_TIMESTAMP);
        let signature = header(&request, auth::CLIENT_SIGNATURE);

        let verified = match (public_key, timestamp, signature) {
            (Some(public_key), Some(timestamp), Some(signature)) => timestamp
                .parse::<u64>()
                .map_err(|_| AuthError::Malformed(auth::CLIENT_TIMESTAMP))
                .and_then(|timestamp_ms| {
                    auth::verify_http_request(
                        &method,
                        &path_and_query,
                        timestamp_ms,
                        public_key,
                        signature,
                        self.max_skew,
                        auth::now_ms(),
                    )
                }),
            _ => Err(AuthError::Missing("client signature headers")),
        };

        match verified {
            Ok(public_key) => {
                request.extensions_mut().insert(ClientIdentity(public_key));
                Either::Left(self.inner.call(request))
            }
            Err(error) => {
                tracing::warn!(%error, method = %method, path = %path_and_query, "rejecting unsigned client request");
                Either::Right(std::future::ready(Ok(handlers::unauthorized())))
            }
        }
    }
}

fn header<'a, B>(request: &'a Request<B>, name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}
