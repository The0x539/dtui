use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use async_trait::async_trait;

type Result = deluge_rpc::Result<()>;

#[async_trait]
pub trait ViewThread: Sized {
    async fn init(&mut self) -> Result {
        Ok(())
    }

    async fn do_update(&mut self) -> Result;

    async fn run(mut self, shutdown: Arc<AsyncRwLock<()>>) -> Result {
        self.init().await?;

        loop {
            let update = self.do_update();

            tokio::select! {
                r = update => r?,
                _ = shutdown.read() => return Ok(()),
            }
        }
    }
}
