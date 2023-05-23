use async_trait::async_trait;

#[derive(Default)]
pub struct IteratorOptions {
    pub chunk_size: Option<usize>,
}

#[async_trait]
pub trait Iterator: Send + Sync {
    type Item: Send + Sync;
    async fn next(&mut self) -> Option<Vec<Self::Item>>;
}
