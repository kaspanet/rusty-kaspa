use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use log::debug;
use pin_project_lite::pin_project;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{ready, Context, Poll},
};

pub use http_body_util::BodyExt;
pub use tower::ServiceBuilder;
pub use tower_http::{map_request_body::MapRequestBodyLayer, map_response_body::MapResponseBodyLayer};

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
    B: Body<Data = Bytes> + Default,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let counter: Arc<AtomicUsize> = this.counter.clone();
        match ready!(this.inner.poll_frame(cx)) {
            Some(Ok(frame)) => {
                if let Some(chunk) = frame.data_ref() {
                    debug!("[SIZE MW] response body chunk size = {}", chunk.len());
                    let _previous = counter.fetch_add(chunk.len(), Ordering::Relaxed);
                    debug!("[SIZE MW] total count: {}", _previous);
                }

                Poll::Ready(Some(Ok(frame)))
            }
            x => Poll::Ready(x),
        }
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
    B: Body<Data = Bytes> + Default,
{
    fn default() -> Self {
        Self { inner: Default::default(), counter: Default::default() }
    }
}

pin_project! {
    pub struct ChannelBody<T> {
        #[pin]
        rx: tokio::sync::mpsc::Receiver<Frame<T>>,
    }
}

impl<T> ChannelBody<T> {
    pub fn new() -> (tokio::sync::mpsc::Sender<Frame<T>>, Self) {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        (tx, Self { rx })
    }
}

impl<T> Body for ChannelBody<T>
where
    T: bytes::Buf,
{
    type Data = T;
    type Error = Infallible;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let frame = ready!(self.project().rx.poll_recv(cx));
        Poll::Ready(frame.map(Ok))
    }
}

fn frame_data_length(frame: &Frame<Bytes>) -> usize {
    if let Some(data) = frame.data_ref() {
        data.len()
    } else {
        0
    }
}

pub fn measure_request_body_size_layer<B1, B2, F>(
    bytes_sent_counter: Arc<AtomicUsize>,
    f: F,
) -> MapRequestBodyLayer<impl Fn(B1) -> B2 + Clone>
where
    B1: Body<Data = Bytes> + Unpin + Send + 'static,
    <B1 as Body>::Error: Send,
    F: Fn(ChannelBody<Bytes>) -> B2 + Clone,
{
    MapRequestBodyLayer::new(move |mut body: B1| {
        let (tx, new_body) = ChannelBody::new();
        let bytes_sent_counter = bytes_sent_counter.clone();
        tokio::spawn({
            async move {
                while let Some(Ok(frame)) = body.frame().await {
                    let len = frame_data_length(&frame);
                    debug!("[SIZE MW] request body chunk size = {len}");
                    let _previous = bytes_sent_counter.fetch_add(len, Ordering::Relaxed);
                    debug!("[SIZE MW] total count: {}", _previous);
                    // error can occurs only if the channel is already closed
                    _ = tx.send(frame).await.inspect_err(|err| debug!("[SIZE MW] error sending frame: {}", err));
                }
            }
        });
        f(new_body)
    })
}
