use std::sync::Arc;
use tokio::sync::{watch, RwLock as AsyncRwLock};
use async_trait::async_trait;
use deluge_rpc::Session;

type Result = deluge_rpc::Result<()>;

#[async_trait]
pub trait ViewThread: Sized {
    async fn init(&mut self, _session: &Session) -> Result {
        Ok(())
    }

    async fn do_update(&mut self, session: &Session) -> Result;

    async fn run(
        mut self,
        mut session_recv: watch::Receiver<Option<Arc<Session>>>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Result {
        let mut session: Option<Arc<Session>> = session_recv.borrow().clone();

        let mut should_reinit = true;

        loop {
            if should_reinit {
                if let Some(ses) = &session {
                    self.init(ses).await?;
                }
                should_reinit = false;
            }

            if let Some(ses) = &session {
                let update = self.do_update(ses);

                tokio::select! {
                    r = update => r?,
                    new_session = session_recv.recv() => {
                        session = new_session.unwrap();
                        should_reinit = true;
                    },
                    _ = shutdown.read() => return Ok(()),
                }
            } else {
                tokio::select! {
                    new_session = session_recv.recv() => {
                        session = new_session.unwrap();
                        should_reinit = true;
                    },
                    _ = shutdown.read() => return Ok(()),
                }
            }
        }
    }
}
