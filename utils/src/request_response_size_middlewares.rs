use futures::ready;
use hyper::{
    body::{Bytes, HttpBody, SizeHint},
    HeaderMap,
};
use log::{debug, warn};
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tower_http::map_request_body::MapRequestBodyLayer;

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
    B: HttpBody<Data = Bytes>,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = self.project();
        let counter: Arc<AtomicUsize> = this.counter.clone();
        match ready!(this.inner.poll_data(cx)) {
            Some(Ok(chunk)) => {
                debug!("[SIZE MW] response body chunk size = {}", chunk.len());
                let previous = counter.fetch_add(chunk.len(), Ordering::Relaxed);
                debug!("[SIZE MW] total count: {}", previous);

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
            while let Some(chunk) = body.data().await {
                let Ok(chunk) = chunk else { continue };
                debug!("[SIZE MW] request body chunk size = {}", chunk.len());
                let previous = bytes_sent_counter.fetch_add(chunk.len(), Ordering::Relaxed);
                debug!("[SIZE MW] total count: {}", previous);
                if let Err(err) = tx.send_data(chunk).await {
                    warn!("[SIZE MW] error sending data: {}", err)
                    // error can occurs if only channel is already closed
                }
            }

            if let Ok(Some(trailers)) = body.trailers().await {
                if let Err(err) = tx.send_trailers(trailers).await {
                    warn!("[SIZE MW] error sending trailers: {}", err)
                    // error can occurs if only channel is already closed
                }
            }
        });
        f(new_body)
    })
}
