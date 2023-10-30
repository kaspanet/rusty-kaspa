#[async_trait::async_trait]
pub trait Handler
where
    Self: 'static + Send + Sync,
{
    async fn start(&mut self);

    fn launch(mut self: Box<Self>) {
        tokio::spawn(async move {
            self.start().await;
        });
    }
}
