use std::sync::Arc;
use tokio::sync::{watch, RwLock as AsyncRwLock};
use async_trait::async_trait;
use tokio::time;
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

    fn tick() -> time::Duration {
        time::Duration::from_secs(5)
    }

    fn should_update_now(&self) -> bool {
        false
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
                    events = Some(ses.subscribe_events());
                    self.init(ses).await?;
                } else {
                    events = None;
                }
                should_reinit = false;
            }

            let tick = time::Instant::now() + Self::tick();

            let (ses, evs) = match (&session, &mut events) {
                (Some(ses), Some(evs)) => (ses, evs),
                _ => tokio::select! {
                    new_session = session_recv.recv() => {
                        session = new_session.unwrap();
                        should_reinit = true;
                        continue;
                    },
                    _ = shutdown.read() => return Ok(()),
                }
            };

            // Assuming this will be reasonably fast.
            // If not for that assumption, I'd select between this, shutdown, and new_session.
            self.do_update(ses).await?;

            loop {
                if self.should_update_now() {
                    break;
                }

                let event = tokio::select! {
                    event = evs.recv() => event.unwrap(),

                    new_session = session_recv.recv() => {
                        session = new_session.unwrap();
                        should_reinit = true;
                        break;
                    },

                    _ = time::delay_until(tick) => break,

                    _ = shutdown.read() => return Ok(()),
                };

                self.on_event(ses, event).await?;
            }
        }
    }
}
