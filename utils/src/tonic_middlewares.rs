use bytes::Bytes;
use http_body::Body;
use hyper::http;
use log::debug;
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
    task::{ready, Context, Poll},
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

impl<B> Body for CountBytesBody<B>
where
    B: Body<Data = Bytes>,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = self.project();
        let counter: Arc<AtomicUsize> = this.counter.clone();
        match ready!(this.inner.poll_data(cx)) {
            Some(Ok(chunk)) => {
                debug!("response body chunk size = {}", chunk.len());
                counter.fetch_add(chunk.len(), SeqCst);
                Poll::Ready(Some(Ok(chunk)))
            }
            x => Poll::Ready(x),
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        self.project().inner.poll_trailers(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

pub fn measure_request_body_size_layer(
    bytes_sent_counter: Arc<AtomicUsize>,
) -> MapRequestBodyLayer<impl Fn(hyper::Body) -> hyper::Body + Clone> {
    MapRequestBodyLayer::new(move |mut body: hyper::Body| {
        let (mut tx, new_body) = hyper::Body::channel();

        let bytes_sent_counter = bytes_sent_counter.clone();
        tokio::spawn(async move {
            while let Some(chunk) = body.data().await {
                let chunk = chunk.unwrap();
                debug!("request body chunk size = {}", chunk.len());
                bytes_sent_counter.fetch_add(chunk.len(), SeqCst);
                tx.send_data(chunk).await.unwrap();
            }

            if let Some(trailers) = body.trailers().await.unwrap() {
                tx.send_trailers(trailers).await.unwrap();
            }
        });

        new_body
    })
}
