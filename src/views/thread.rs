use std::sync::Arc;
use tokio::sync::{watch, RwLock as AsyncRwLock};
use async_trait::async_trait;
use futures::FutureExt;
use deluge_rpc::{Session, Event};

type Result = deluge_rpc::Result<()>;

#[async_trait]
pub trait ViewThread: Sized {
    async fn init(&mut self, _session: &Session) -> Result {
        Ok(())
    }

    async fn do_update(&mut self, _session: &Session) -> Result {
        Ok(())
    }

    async fn on_event(&mut self, _session: &Session, _event: Event) -> Result {
        Ok(())
    }

    async fn run(
        mut self,
        mut session_recv: watch::Receiver<Option<Arc<Session>>>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Result {
        let mut session: Option<Arc<Session>> = session_recv.borrow().clone();
        let mut events = None;

        let mut should_reinit = true;

        loop {
            if should_reinit {
                if let Some(ses) = &session {
                    self.init(ses).await?;
                    events = Some(ses.subscribe_events());
                } else {
                    events = None;
                }
                should_reinit = false;
            }

            if let (Some(session), Some(events)) = (&session, &mut events) {
                while let Some(event) = events.recv().now_or_never() {
                    self.on_event(session, event.unwrap()).await?;
                }
            }

            let update = session.as_ref().map(|ses| self.do_update(ses));
            tokio::select! {
                r = update.unwrap(), if update.is_some() => r?,
                new_session = session_recv.recv() => {
                    session = new_session.unwrap();
                    should_reinit = true;
                },
                _ = shutdown.read() => return Ok(()),
            }
        }
    }
}
