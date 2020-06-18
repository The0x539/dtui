use std::sync::Arc;
use tokio::sync::{watch, broadcast, Notify};
use futures::FutureExt;
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
    ) -> Result where Self: Sized {
        let mut handle = session_recv
            .recv()
            .now_or_never()
            .expect("Receiver must have a value ready.")
            .expect("Receiver must not be closed.");

        let mut events = broadcast::channel(1).1;
        let update_notifier = self.update_notifier();

        let mut should_reload = true;

        'main: loop {
            if should_reload {
                should_reload = false;

                if let Some(session) = handle.get_session() {
                    events = session.subscribe_events();
                    self.reload(session).await?;
                } else {
                    // events = ...; // unnecessary
                    // TODO
                    // self.clear();
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
                        _ = time::delay_until(tick) => break 'idle,

                        x = session_recv.recv() => match x {
                            Some(new_handle) => {
                                handle.relinquish().await;
                                handle = new_handle;
                                should_reload = true;
                                continue 'main;
                            },
                            None => break 'main,
                        },
                    };

                    self.on_event(session, event).await?;
                }
            } else {
                if let Some(new_handle) = session_recv.recv().await {
                    handle.relinquish().await;
                    handle = new_handle;
                    should_reload = true;
                    continue 'main;
                } else {
                    break 'main;
                }
            }
        }

        drop(session_recv);
        handle.relinquish().await;
        Ok(())
    }
}
