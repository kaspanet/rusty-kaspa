use futures::ready;
use hyper::{
    body::{Bytes, HttpBody, SizeHint},
    HeaderMap,
};
use log::*;
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
pub use tower::ServiceBuilder;
pub use tower_http::map_request_body::MapRequestBodyLayer;
pub use tower_http::map_response_body::MapResponseBodyLayer;

pin_project! {
    pub struct CountBytesBody<B> {
        #[pin]
        pub inner: B,
        pub counter: Arc<AtomicUsize>,
    }
}

impl<B> CountBytesBody<B> {
    pub fn new(inner: B, counter: Arc<AtomicUsize>) -> CountBytesBody<B> {
        CountBytesBody { inner, counter }
    }
}

impl<B> HttpBody for CountBytesBody<B>
where
    B: HttpBody<Data = Bytes> + Default,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = self.project();
        let counter: Arc<AtomicUsize> = this.counter.clone();
        match ready!(this.inner.poll_data(cx)) {
            Some(Ok(chunk)) => {
                debug!("[SIZE MW] response body chunk size = {}", chunk.len());
                let _previous = counter.fetch_add(chunk.len(), Ordering::Relaxed);
                debug!("[SIZE MW] total count: {}", _previous);

                Poll::Ready(Some(Ok(chunk)))
            }
            x => Poll::Ready(x),
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        self.project().inner.poll_trailers(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B> Default for CountBytesBody<B>
where
    B: HttpBody<Data = Bytes> + Default,
{
    fn default() -> Self {
        Self { inner: Default::default(), counter: Default::default() }
    }
}

pub fn measure_request_body_size_layer<B1, B2, F>(
    bytes_sent_counter: Arc<AtomicUsize>,
    f: F,
) -> MapRequestBodyLayer<impl Fn(B1) -> B2 + Clone>
where
    B1: HttpBody<Data = Bytes> + Unpin + Send + 'static,
    <B1 as HttpBody>::Error: Send,
    F: Fn(hyper::body::Body) -> B2 + Clone,
{
    MapRequestBodyLayer::new(move |mut body: B1| {
        let (mut tx, new_body) = hyper::Body::channel();
        let bytes_sent_counter = bytes_sent_counter.clone();
        tokio::spawn(async move {
            while let Some(Ok(chunk)) = body.data().await {
                debug!("[SIZE MW] request body chunk size = {}", chunk.len());
                let _previous = bytes_sent_counter.fetch_add(chunk.len(), Ordering::Relaxed);
                debug!("[SIZE MW] total count: {}", _previous);
                if let Err(_err) = tx.send_data(chunk).await {
                    // error can occurs only if the channel is already closed
                    debug!("[SIZE MW] error sending data: {}", _err)
                }
            }

            if let Ok(Some(trailers)) = body.trailers().await {
                if let Err(_err) = tx.send_trailers(trailers).await {
                    // error can occurs only if the channel is already closed
                    debug!("[SIZE MW] error sending trailers: {}", _err)
                }
            }
        });
        f(new_body)
    })
}
