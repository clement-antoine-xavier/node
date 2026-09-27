//! Custom `tower` layers for the client-facing HTTP stack.

use std::convert::Infallible;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::future::Either;
use http::{Method, Request, Response};
use tower::{Layer, Service};

use crate::client::handlers::{self, Body};

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
