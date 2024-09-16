use super::{svc_hyper::HyperService, HttpServeResult};
use crate::executor::HyperExecutor;
use hyper::server::conn::http1::Builder as Http1Builder;
use hyper::server::conn::http2::Builder as Http2Builder;
use hyper_util::{rt::TokioIo, server::conn::auto::Builder as AutoBuilder};
use rama_core::context::StateTransformer;
use rama_core::{Context, Service};
use rama_http_types::{IntoResponse, Request};
use rama_net::stream::Stream;
use rama_tcp::utils::is_connection_error;
use rama_utils::future::Fuse;
use std::convert::Infallible;
use std::error::Error;
use std::pin::pin;
use tokio::select;

/// A utility trait to allow any of the hyper server builders to be used
/// in the same way to (http) serve a connection.
pub trait HyperConnServer: Send + Sync + private::Sealed + 'static {
    fn hyper_serve_connection<IO, State, S, R, Response>(
        &self,
        ctx: Context<State>,
        io: IO,
        service: S,
        state_transformer: R,
    ) -> impl std::future::Future<Output = HttpServeResult> + Send + '_
    where
        IO: Stream,
        State: Send + Sync + 'static,
        S: Service<R::Output, Request, Response = Response, Error = Infallible>,
        R: StateTransformer<State, Output: Send + Sync + 'static, Error = Infallible>
            + Send
            + Sync
            + 'static,
        Response: IntoResponse + Send + 'static;
}

impl HyperConnServer for Http1Builder {
    #[inline]
    async fn hyper_serve_connection<IO, State, S, R, Response>(
        &self,
        ctx: Context<State>,
        io: IO,
        service: S,
        state_transformer: R,
    ) -> HttpServeResult
    where
        IO: Stream,
        State: Send + Sync + 'static,
        S: Service<R::Output, Request, Response = Response, Error = Infallible>,
        R: StateTransformer<State, Output: Send + Sync + 'static, Error = Infallible>
            + Send
            + Sync
            + 'static,
        Response: IntoResponse + Send + 'static,
    {
        let stream = TokioIo::new(Box::pin(io));
        let guard = ctx.guard().cloned();
        let service = HyperService::new(ctx, service, state_transformer);

        let mut conn = pin!(self.serve_connection(stream, service).with_upgrades());

        if let Some(guard) = guard {
            let mut cancelled_fut = pin!(Fuse::new(guard.cancelled()));

            loop {
                select! {
                    _ = cancelled_fut.as_mut() => {
                        tracing::trace!("signal received: initiate graceful shutdown");
                        conn.as_mut().graceful_shutdown();
                    }
                    result = conn.as_mut() => {
                        tracing::trace!("connection finished");
                        return map_hyper_result(result);
                    }
                }
            }
        } else {
            map_hyper_result(conn.await)
        }
    }
}

impl HyperConnServer for Http2Builder<HyperExecutor> {
    #[inline]
    async fn hyper_serve_connection<IO, State, S, R, Response>(
        &self,
        ctx: Context<State>,
        io: IO,
        service: S,
        state_transformer: R,
    ) -> HttpServeResult
    where
        IO: Stream,
        State: Send + Sync + 'static,
        S: Service<R::Output, Request, Response = Response, Error = Infallible>,
        R: StateTransformer<State, Output: Send + Sync + 'static, Error = Infallible>
            + Send
            + Sync
            + 'static,
        Response: IntoResponse + Send + 'static,
    {
        let stream = TokioIo::new(Box::pin(io));
        let guard = ctx.guard().cloned();
        let service = HyperService::new(ctx, service, state_transformer);

        let mut conn = pin!(self.serve_connection(stream, service));

        if let Some(guard) = guard {
            let mut cancelled_fut = pin!(Fuse::new(guard.cancelled()));

            loop {
                select! {
                    _ = cancelled_fut.as_mut() => {
                        tracing::trace!("signal received: initiate graceful shutdown");
                        conn.as_mut().graceful_shutdown();
                    }
                    result = conn.as_mut() => {
                        tracing::trace!("connection finished");
                        return map_hyper_result(result);
                    }
                }
            }
        } else {
            map_hyper_result(conn.await)
        }
    }
}

impl HyperConnServer for AutoBuilder<HyperExecutor> {
    #[inline]
    async fn hyper_serve_connection<IO, State, S, R, Response>(
        &self,
        ctx: Context<State>,
        io: IO,
        service: S,
        state_transformer: R,
    ) -> HttpServeResult
    where
        IO: Stream,
        State: Send + Sync + 'static,
        S: Service<R::Output, Request, Response = Response, Error = Infallible>,
        R: StateTransformer<State, Output: Send + Sync + 'static, Error = Infallible>
            + Send
            + Sync
            + 'static,
        Response: IntoResponse + Send + 'static,
    {
        let stream = TokioIo::new(Box::pin(io));
        let guard = ctx.guard().cloned();
        let service = HyperService::new(ctx, service, state_transformer);

        let mut conn = pin!(self.serve_connection_with_upgrades(stream, service));

        if let Some(guard) = guard {
            let mut cancelled_fut = pin!(Fuse::new(guard.cancelled()));

            loop {
                select! {
                    _ = cancelled_fut.as_mut() => {
                        tracing::trace!("signal received: nop: graceful shutdown not supported for auto builder");
                        conn.as_mut().graceful_shutdown();
                    }
                    result = conn.as_mut() => {
                        tracing::trace!("connection finished");
                        return map_boxed_hyper_result(result);
                    }
                }
            }
        } else {
            map_boxed_hyper_result(conn.await)
        }
    }
}

/// A utility function to map boxed, potentially hyper errors, to our own error type.
fn map_boxed_hyper_result(
    result: Result<(), Box<dyn std::error::Error + Send + Sync>>,
) -> HttpServeResult {
    match result {
        Ok(_) => Ok(()),
        Err(err) => match err.downcast::<hyper::Error>() {
            Ok(err) => map_hyper_err_to_result(*err),
            Err(err) => match err.downcast::<std::io::Error>() {
                Ok(err) => {
                    if is_connection_error(&err) {
                        Ok(())
                    } else {
                        Err(err.into())
                    }
                }
                Err(err) => Err(err),
            },
        },
    }
}

/// A utility function to map hyper errors to our own error type.
fn map_hyper_result(result: hyper::Result<()>) -> HttpServeResult {
    match result {
        Ok(_) => Ok(()),
        Err(err) => map_hyper_err_to_result(err),
    }
}

/// A utility function to map hyper errors to our own error type.
fn map_hyper_err_to_result(err: hyper::Error) -> HttpServeResult {
    if err.is_canceled() || err.is_closed() {
        return Ok(());
    }

    if let Some(source_err) = err.source() {
        if let Some(h2_err) = source_err.downcast_ref::<h2::Error>() {
            if h2_err.is_go_away() || h2_err.is_io() {
                return Ok(());
            }
        } else if let Some(io_err) = source_err.downcast_ref::<std::io::Error>() {
            if is_connection_error(io_err) {
                return Ok(());
            }
        }
    }

    Err(err.into())
}

mod private {
    use crate::executor::HyperExecutor;

    pub trait Sealed {}

    impl Sealed for super::Http1Builder {}
    impl Sealed for super::Http2Builder<HyperExecutor> {}
    impl Sealed for super::AutoBuilder<HyperExecutor> {}
}
