use crate::SessionHandle;
use async_trait::async_trait;
use deluge_rpc::{Event, Session};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, Notify};
use tokio::time;

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

    fn clear(&mut self);

    async fn run(mut self, mut session_recv: watch::Receiver<SessionHandle>) -> Result
    where
        Self: Sized,
    {
        let mut handle = session_recv.borrow().clone();

        let mut events = broadcast::channel(1).1;
        let update_notifier = self.update_notifier();

        let mut should_reload = true;
        let mut should_check = true;

        'main: loop {
            if should_reload {
                should_reload = false;

                if let Some(session) = handle.get_session() {
                    events = session.subscribe_events();
                    self.reload(session).await?;
                } else {
                    self.clear();
                }
            }

            if let Some(session) = handle.get_session() {
                let tick = time::Instant::now() + self.tick();

                // Assuming this will be reasonably fast.
                // If not for that assumption, I'd select between this, shutdown, and new_session.
                self.update(session).await?;

                'idle: loop {
                    // The select macro isn't gonna let us call self.on_event().
                    // As a workaround, we do it like this.
                    let event = tokio::select! {
                        event = events.recv() => event.unwrap(),

                        _ = update_notifier.notified() => break 'idle,
                        _ = time::sleep_until(tick) => break 'idle,

                        x = session_recv.changed() => match x {
                            Ok(()) => {
                                handle = session_recv.borrow().clone();
                                should_reload = true;
                                continue 'main;
                            },
                            Err(_) => {
                                should_check = false;
                                continue 'main;
                            }
                        },
                    };

                    self.on_event(session, event).await?;
                }
            } else if should_check {
                match session_recv.changed().await {
                    Ok(()) => {
                        handle = session_recv.borrow().clone();
                        should_reload = true;
                    }
                    Err(_) => should_check = false,
                }
            } else {
                // There's no active session.
                // The sending end of the channel we'd receive a new one on has been dropped.
                // We're never going to get another session.
                return Ok(());
            }
        }
    }
}
