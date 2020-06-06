use super::TabData;
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{LinearLayout, TextView, DummyView, Button, Panel, EnableableView};
use cursive::traits::Resizable;
use async_trait::async_trait;
use crate::views::spin::SpinView;
use tokio::sync::watch;
use crate::views::{linear_panel::LinearPanel, labeled_checkbox::LabeledCheckbox};
use std::sync::{Arc, RwLock};
use tokio::sync::Notify;
use tokio::task;
use tokio::time;
use cursive::traits::Nameable;

#[derive(Default, Debug, Clone, Deserialize, Query)]
pub(super) struct OptionsQuery {
    pub max_download_speed: f64,
    pub max_upload_speed: f64,
    pub max_connections: i64,
    pub max_upload_slots: i64,

    pub auto_managed: bool,
    pub stop_at_ratio: bool,
    pub stop_ratio: f64,
    pub remove_at_ratio: bool,
}

#[derive(Clone)]
pub(super) struct OptionsNames {
    pub max_download_speed: String,
    pub max_upload_speed: String,
    pub max_connections: String,
    pub max_upload_slots: String,
    pub auto_managed: String,
    pub stop_at_ratio: String,
    pub stop_ratio: String,
    pub remove_at_ratio: String,

    pub ratio_limit_panel: String,
    pub apply_button: String,
}

impl OptionsNames {
    fn new() -> Self {
        use uuid::Uuid;
        Self {
            max_download_speed: Uuid::new_v4().to_string(),
            max_upload_speed: Uuid::new_v4().to_string(),
            max_connections: Uuid::new_v4().to_string(),
            max_upload_slots: Uuid::new_v4().to_string(),
            auto_managed: Uuid::new_v4().to_string(),
            stop_at_ratio: Uuid::new_v4().to_string(),
            stop_ratio: Uuid::new_v4().to_string(),
            remove_at_ratio: Uuid::new_v4().to_string(),

            ratio_limit_panel: Uuid::new_v4().to_string(),
            apply_button: Uuid::new_v4().to_string(),
        }
    }
}

pub(super) struct OptionsData {
    active_torrent: Option<InfoHash>,
    current_options_send: watch::Sender<OptionsQuery>,
    apply_notify: Arc<Notify>,
    pub current_options_recv: watch::Receiver<OptionsQuery>,
    pub pending_options: Arc<RwLock<Option<OptionsQuery>>>,
    pub names: OptionsNames,
}

impl OptionsData {
    async fn apply(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let new_options = task::block_in_place(|| {
            let mut opts = self.pending_options.write().unwrap();
            assert!(opts.is_some());
            opts.take().unwrap()
        });

        self.current_options_send.broadcast(new_options).unwrap();

        assert!(self.active_torrent.is_some());
        let hash = self.active_torrent.unwrap();

        let options = {
            let c = self.current_options_recv.borrow();
            // Not sure whether I made a mistake with this interface.
            deluge_rpc::TorrentOptions {
                max_download_speed: Some(c.max_download_speed),
                max_upload_speed: Some(c.max_upload_speed),
                max_connections: Some(c.max_connections),
                max_upload_slots: Some(c.max_upload_slots),
                auto_managed: Some(c.auto_managed),
                stop_at_ratio: Some(c.stop_at_ratio),
                stop_ratio: Some(c.stop_ratio),
                remove_at_ratio: Some(c.remove_at_ratio),
                ..Default::default()
            }
        };

        session.set_torrent_options(&[hash], &options).await
    }
}

#[async_trait]
impl TabData for OptionsData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let pending_options = Arc::new(RwLock::new(None));
        let (current_options_send, current_options_recv) = watch::channel(OptionsQuery::default());
        let names = OptionsNames::new();

        macro_rules! set {
            ($obj:ident.$field:ident) => {
                {
                    let cloned_arc = $obj.clone();
                    let current_options_recv = current_options_recv.clone();
                    move |_, v| {
                        cloned_arc
                            .write()
                            .unwrap()
                            .get_or_insert_with(|| current_options_recv.borrow().clone())
                            .$field = v;
                    }
                }
            }
        }

        let bandwidth_limits = {
            let down = SpinView::new(Some("Download Speed"), Some("kiB/s"), -1.0f64..)
                .on_modify(set!(pending_options.max_download_speed))
                .with_name(&names.max_download_speed);

            let up = SpinView::new(Some("Upload Speed"), Some("kiB/s"), -1.0f64..)
                .on_modify(set!(pending_options.max_upload_speed))
                .with_name(&names.max_upload_speed);

            let peers = SpinView::new(Some("Connections"), None, -1i64..)
                .on_modify(set!(pending_options.max_connections))
                .with_name(&names.max_connections);

            let slots = SpinView::new(Some("Upload Slots"), None, -1i64..)
                .on_modify(set!(pending_options.max_upload_slots))
                .with_name(&names.max_upload_slots);

            LinearPanel::vertical()
                .child(down, None)
                .child(up, None)
                .child(peers, None)
                .child(slots, None)
        };

        let col1 = LinearLayout::vertical()
            .child(TextView::new("Bandwidth Limits"))
            .child(bandwidth_limits)
            .max_width(40);

        let apply_notify = Arc::new(Notify::new());

        let col2 = {
            let auto_managed = LabeledCheckbox::new("Auto Managed")
                .on_change(set!(pending_options.auto_managed))
                .with_name(&names.auto_managed);

            let stop_at_ratio = LabeledCheckbox::new("Stop seed at ratio:")
                .on_change(set!(pending_options.stop_at_ratio))
                .with_name(&names.stop_at_ratio);

            let ratio_limit_panel = {
                let spinner = SpinView::new(None, None, 0.0f64..)
                    .on_modify(set!(pending_options.stop_ratio))
                    .with_name(&names.stop_ratio);

                let checkbox = LabeledCheckbox::new("Remove at ratio")
                    .on_change(set!(pending_options.remove_at_ratio))
                    .with_name(&names.remove_at_ratio);

                let layout = LinearLayout::vertical().child(spinner).child(checkbox);
                EnableableView::new(Panel::new(layout))
                    .with_name(&names.ratio_limit_panel)
                    .max_width(30)
            };

            let apply_notify = apply_notify.clone();
            let apply = Button::new("Apply", move |_| apply_notify.notify())
                .with_name(&names.apply_button);
            let apply_panel = Panel::new(apply);

            LinearLayout::vertical()
                .child(auto_managed)
                .child(stop_at_ratio)
                .child(ratio_limit_panel)
                .child(apply_panel)
        };

        let view = LinearLayout::horizontal()
            .child(col1)
            .child(DummyView.fixed_width(2))
            .child(col2);

        let data = OptionsData {
            active_torrent: None,
            current_options_send,
            current_options_recv,
            apply_notify,
            pending_options,
            names,
        };
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        let deadline = time::Instant::now() + time::Duration::from_secs(1);
        let new_active = !self.active_torrent.contains(&hash);

        if new_active {
            self.active_torrent = Some(hash);
            task::block_in_place(|| self.pending_options.write().unwrap().take());
        }

        if new_active || task::block_in_place(|| self.pending_options.read().unwrap().is_none()) {
            let options = session.get_torrent_status::<OptionsQuery>(hash).await?;
            self.current_options_send.broadcast(options).unwrap();
            time::delay_until(deadline).await;
        } else {
            let timeout = time::timeout_at(deadline, self.apply_notify.notified());
            if let Ok(()) = timeout.await {
                self.apply(session).await?;
            }
        }

        Ok(())
    }
}

