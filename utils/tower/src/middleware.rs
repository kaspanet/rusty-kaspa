use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use log::trace;
use pin_project_lite::pin_project;
use std::{
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
        match ready!(this.inner.poll_frame(cx)) {
            Some(Ok(frame)) => {
                if let Some(chunk) = frame.data_ref() {
                    trace!("[SIZE MW] body chunk size = {}", chunk.len());
                    let _previous = this.counter.fetch_add(chunk.len(), Ordering::Relaxed);
                    trace!("[SIZE MW] total count: {}", _previous);
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
