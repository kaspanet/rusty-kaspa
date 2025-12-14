use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Mock connection for testing
pub struct MockConnection {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    in_chan: Arc<Mutex<Option<mpsc::UnboundedReceiver<Vec<u8>>>>>,
    #[allow(dead_code)]
    out_chan: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
    in_tx: mpsc::UnboundedSender<Vec<u8>>,
    out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

static CHANNEL_COUNTER: AtomicI32 = AtomicI32::new(0);

impl MockConnection {
    pub fn new() -> (Self, mpsc::UnboundedSender<Vec<u8>>) {
        let id = format!("mc_{}", CHANNEL_COUNTER.fetch_add(1, Ordering::Relaxed));
        
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, _out_rx) = mpsc::unbounded_channel();
        
        let in_tx_clone = in_tx.clone();
        
        // Store receiver in the struct - caller can access via read_test_data_from_buffer
        let conn = Self {
            id,
            in_chan: Arc::new(Mutex::new(Some(in_rx))),
            out_chan: Arc::new(Mutex::new(Some(out_tx))),
            in_tx,
            out_rx: _out_rx,
        };
        
        (conn, in_tx_clone)
    }

    pub fn async_write_test_data_to_read_buffer(&self, data: String) {
        let _ = self.in_tx.send(data.into_bytes());
    }

    pub fn read_test_data_from_buffer(&mut self) -> Option<Vec<u8>> {
        self.out_rx.try_recv().ok()
    }

    pub fn async_read_test_data_from_buffer<F: FnOnce(Vec<u8>) + Send + 'static>(&self, handler: F) {
        // Note: This requires the receiver to be shared, which isn't possible with mpsc
        // This is a limitation of the mock - in real usage, you'd need a different approach
        // For now, we'll leave this as a placeholder
        let _ = handler;
    }
}

/// Mock address for testing
#[derive(Debug, Clone)]
pub struct MockAddr {
    id: String,
}

impl MockAddr {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl std::net::ToSocketAddrs for MockAddr {
    type Iter = std::iter::Once<std::net::SocketAddr>;
    
    fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
        // Return a dummy address
        Ok(std::iter::once("127.0.0.1:0".parse().unwrap()))
    }
}

impl std::fmt::Display for MockAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

