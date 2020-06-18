use std::sync::Arc;
use tokio::sync::{watch, RwLock as AsyncRwLock, Notify};
use async_trait::async_trait;
use tokio::time;
use deluge_rpc::{Session, Event};
use crate::SessionHandle;

type Result = deluge_rpc::Result<()>;

#[async_trait]
pub(crate) trait ViewThread: Send {
    async fn reload(&mut self, session: &Session) -> Result {
        self.update(session).await
    }

    async fn update(&mut self, _session: &Session) -> Result;

    async fn on_event(&mut self, _session: &Session, _event: Event) -> Result {
        Ok(())
    }

    fn tick(&self) -> time::Duration {
        time::Duration::from_secs(5)
    }

    fn update_notifier(&self) -> Arc<Notify> {
        Arc::new(Notify::new())
    }

    async fn run(
        mut self,
        mut session_recv: watch::Receiver<SessionHandle>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Result where Self: Sized {
        let mut handle = SessionHandle::default();
        let mut events = None;
        let mut update_notifier = Arc::new(Notify::new());

        let mut should_reinit = false;

        'main: loop {
            if should_reinit {
                if let Some(ses) = handle.get_session() {
                    events = Some(ses.subscribe_events());
                    self.reload(ses).await?;
                    update_notifier = self.update_notifier();
                } else {
                    events = None;
                }
                should_reinit = false;
            }

            let tick = time::Instant::now() + self.tick();

            let (ses, evs) = match (handle.get_session(), &mut events) {
                (Some(ses), Some(evs)) => (ses, evs),
                _ => tokio::select! {
                    new_session = session_recv.recv() => {
                        handle.relinquish().await;
                        handle = new_session.unwrap();
                        should_reinit = true;
                        continue 'main;
                    },
                    _ = shutdown.read() => break 'main,
                }
            };

            // Assuming this will be reasonably fast.
            // If not for that assumption, I'd select between this, shutdown, and new_session.
            self.update(ses).await?;

            'idle: loop {
                // The select macro isn't gonna let us call self.on_event().
                // As a workaround, we do it like this.
                let event = tokio::select! {
                    event = evs.recv() => event.unwrap(),
                    new_session = session_recv.recv() => {
                        handle.relinquish().await;
                        handle = new_session.unwrap();
                        should_reinit = true;
                        break 'idle;
                    },
                    _ = update_notifier.notified() => break 'idle,
                    _ = time::delay_until(tick) => break 'idle,
                    _ = shutdown.read() => break 'main,
                };

                self.on_event(ses, event).await?;
            }
        }

        handle.relinquish().await;
        Ok(())
    }
}
