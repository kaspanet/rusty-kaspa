use crate::tcp_limiter::Limit;
use pin_project::{pin_project, pinned_drop};
use std::{
    io,
    pin::Pin,
    sync::{atomic::Ordering, Arc},
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tonic::transport::server::Connected;

impl Wrapper {
    pub fn new(inner: TcpStream, limit: impl Into<Arc<Limit>>) -> Option<Self> {
        let limit = limit.into();

        loop {
            let current = limit.load(Ordering::Acquire);
            if current + 1 > limit.max() {
                return None;
            }
            match limit.compare_exchange(current, current + 1, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return Some(Self { inner, limit }),
                Err(_) => continue, // The global counter was updated by another thread, retry
            }
        }
    }
}

#[pinned_drop]
impl PinnedDrop for Wrapper {
    fn drop(self: Pin<&mut Self>) {
        self.limit.fetch_sub(1, Ordering::Release);
    }
}

impl AsyncWrite for Wrapper {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.project();
        this.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        this.inner.poll_shutdown(cx)
    }
}

#[pin_project(PinnedDrop)]
pub struct Wrapper {
    #[pin]
    inner: TcpStream,
    limit: Arc<Limit>,
}

impl Connected for Wrapper {
    type ConnectInfo = <TcpStream as Connected>::ConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.inner.connect_info()
    }
}

impl AsyncRead for Wrapper {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        this.inner.poll_read(cx, buf)
    }
}
