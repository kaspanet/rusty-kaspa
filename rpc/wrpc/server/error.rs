
#[derive(Debug, Error)]
enum Error {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] workflow_websocket::server::Error),
    #[error("Workflow allocator error: {0}")]
    WorkflowAllocator(String),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

