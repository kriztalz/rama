//! Builder types to compose layers and services

use super::{
    layer::{
        layer_fn, Either, Identity, LayerFn, MapErrLayer, MapRequestLayer, MapResponseLayer, Stack,
    },
    BoxService, Layer, Service,
};
use std::fmt;

/// Declaratively construct [`Service`] values.
///
/// [`ServiceBuilder`] provides a [builder-like interface][builder] for composing
/// layers to be applied to a [`Service`].
///
/// [`Service`]: crate::service::Service
/// [builder]: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
#[derive(Clone)]
pub struct ServiceBuilder<L> {
    layer: L,
}

impl Default for ServiceBuilder<Identity> {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBuilder<Identity> {
    /// Create a new [`ServiceBuilder`].
    pub fn new() -> Self {
        ServiceBuilder {
            layer: Identity::new(),
        }
    }
}

impl<L> ServiceBuilder<L> {
    /// Add a new layer `T` into the [`ServiceBuilder`].
    ///
    /// This wraps the inner service with the service provided by a user-defined
    /// [`Layer`]. The provided layer must implement the [`Layer`] trait.
    ///
    /// [`Layer`]: crate::service::Layer
    pub fn layer<T>(self, layer: T) -> ServiceBuilder<Stack<T, L>> {
        ServiceBuilder {
            layer: Stack::new(layer, self.layer),
        }
    }

    /// Optionally add a new layer `T` into the [`ServiceBuilder`].
    pub fn option_layer<T>(
        self,
        layer: Option<T>,
    ) -> ServiceBuilder<Stack<Either<T, Identity>, L>> {
        let layer = if let Some(layer) = layer {
            Either::Left(layer)
        } else {
            Either::Right(Identity::new())
        };
        self.layer(layer)
    }

    /// Add a [`Layer`] built from a function that accepts a service and returns another service.
    ///
    /// See the documentation for [`layer_fn`] for more details.
    ///
    /// [`layer_fn`]: crate::service::layer::layer_fn
    pub fn layer_fn<F>(self, f: F) -> ServiceBuilder<Stack<LayerFn<F>, L>> {
        self.layer(layer_fn(f))
    }

    /// Map one request type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapRequest`]
    /// middleware.
    ///
    /// [`MapRequest`]: crate::service::layer::MapRequest
    pub fn map_request<F, R1, R2>(self, f: F) -> ServiceBuilder<Stack<MapRequestLayer<F>, L>>
    where
        F: FnMut(R1) -> R2 + Clone,
    {
        self.layer(MapRequestLayer::new(f))
    }

    /// Map one response type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapResponse`]
    /// middleware.
    ///
    /// [`MapResponse`]: crate::service::layer::MapResponse
    pub fn map_response<F>(self, f: F) -> ServiceBuilder<Stack<MapResponseLayer<F>, L>> {
        self.layer(MapResponseLayer::new(f))
    }

    /// Map one error type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapErr`]
    /// middleware.
    ///
    /// [`MapErr`]: crate::service::layer::MapErr
    pub fn map_err<F>(self, f: F) -> ServiceBuilder<Stack<MapErrLayer<F>, L>> {
        self.layer(MapErrLayer::new(f))
    }

    // /// Apply an asynchronous function after the service, regardless of whether the future
    // /// succeeds or fails.
    // ///
    // /// This wraps the inner service with an instance of the [`Then`]
    // /// middleware.
    // ///
    // /// This is similar to the [`map_response`] and [`map_err`] functions,
    // /// except that the *same* function is invoked when the service's future
    // /// completes, whether it completes successfully or fails. This function
    // /// takes the [`Result`] returned by the service's future, and returns a
    // /// [`Result`].
    // ///
    // /// See the documentation for the [`then` combinator] for details.
    // ///
    // /// [`Then`]: crate::util::Then
    // /// [`then` combinator]: crate::util::ServiceExt::then
    // /// [`map_response`]: ServiceBuilder::map_response
    // /// [`map_err`]: ServiceBuilder::map_err
    // pub fn then<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::ThenLayer<F>, L>> {
    //     self.layer(crate::util::ThenLayer::new(f))
    // }

    // /// Executes a new future after this service's future resolves. This does
    // /// not alter the behaviour of the [`poll_ready`] method.
    // ///
    // /// This method can be used to change the [`Response`] type of the service
    // /// into a different type. You can use this method to chain along a computation once the
    // /// service's response has been resolved.
    // ///
    // /// This wraps the inner service with an instance of the [`AndThen`]
    // /// middleware.
    // ///
    // /// See the documentation for the [`and_then` combinator] for details.
    // ///
    // /// [`Response`]: crate::Service::Response
    // /// [`poll_ready`]: crate::Service::poll_ready
    // /// [`and_then` combinator]: crate::util::ServiceExt::and_then
    // /// [`AndThen`]: crate::util::AndThen
    // pub fn and_then<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::AndThenLayer<F>, L>> {
    //     self.layer(crate::util::AndThenLayer::new(f))
    // }

    // /// Maps this service's result type (`Result<Self::Response, Self::Error>`)
    // /// to a different value, regardless of whether the future succeeds or
    // /// fails.
    // ///
    // /// This wraps the inner service with an instance of the [`MapResult`]
    // /// middleware.
    // ///
    // /// See the documentation for the [`map_result` combinator] for details.
    // ///
    // /// [`map_result` combinator]: crate::util::ServiceExt::map_result
    // /// [`MapResult`]: crate::util::MapResult
    // pub fn map_result<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::MapResultLayer<F>, L>> {
    //     self.layer(crate::util::MapResultLayer::new(f))
    // }

    /// Returns the underlying `Layer` implementation.
    pub fn into_inner(self) -> L {
        self.layer
    }

    /// Wrap the service `S` with the middleware provided by this
    /// [`ServiceBuilder`]'s [`Layer`]'s, returning a new [`Service`].
    ///
    /// [`Layer`]: crate::service::Layer
    /// [`Service`]: crate::service::Service
    pub fn service<S>(&self, service: S) -> L::Service
    where
        L: Layer<S>,
    {
        self.layer.layer(service)
    }

    // /// Wrap the async function `F` with the middleware provided by this [`ServiceBuilder`]'s
    // /// [`Layer`]s, returning a new [`Service`].
    // ///
    // /// [`Layer`]: crate::service::Layer
    // /// [`Service`]: crate::service::Service
    // /// [`service_fn`]: crate::service::service_fn
    // pub fn service_fn<F>(self, f: F) -> L::Service
    // where
    //     L: Layer<crate::util::ServiceFn<F>>,
    // {
    //     self.service(crate::util::service_fn(f))
    // }

    /// This ensures the service produced
    /// by the inner [`Layer`] is Boxed as [`BoxService`] and can be used in situations where
    /// dynamic dispatch is required.
    ///
    /// See that method for more details.
    pub fn boxed<S, R, State>(
        self,
    ) -> ServiceBuilder<
        Stack<
            LayerFn<
                fn(
                    L::Service,
                ) -> crate::service::BoxService<
                    State,
                    R,
                    <L::Service as Service<State, R>>::Response,
                    <L::Service as Service<State, R>>::Error,
                >,
            >,
            L,
        >,
    >
    where
        L: Layer<S>,
        L::Service: Service<State, R> + Clone,
    {
        self.layer_fn(BoxService::new)
    }
}

impl<L: fmt::Debug> fmt::Debug for ServiceBuilder<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ServiceBuilder").field(&self.layer).finish()
    }
}

impl<S, L> Layer<S> for ServiceBuilder<L>
where
    L: Layer<S>,
{
    type Service = L::Service;

    fn layer(&self, inner: S) -> Self::Service {
        self.layer.layer(inner)
    }
}
