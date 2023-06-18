use crate::result::Result;
use async_trait::async_trait;

#[derive(Default, Clone)]
pub struct IteratorOptions {
    pub chunk_size: Option<usize>,
}

#[async_trait]
pub trait Iterator: Send + Sync {
    type Item: Send + Sync;
    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>>;

    async fn len(&mut self) -> Result<usize> {
        let mut len = 0;
        while let Some(chunk) = self.next().await? {
            len += chunk.len();
        }
        Ok(len)
    }

    async fn is_empty(&mut self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    async fn collect(&mut self) -> Result<Vec<Self::Item>> {
        let mut result = Vec::new();
        while let Some(chunk) = self.next().await? {
            result.extend(chunk);
        }
        Ok(result)
    }
}
