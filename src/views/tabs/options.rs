use super::{BuildableTabData, TabData};
use crate::views::spin::SpinView;
use crate::views::thread::ViewThread;
use crate::views::{
    labeled_checkbox::LabeledCheckbox,
    static_linear_layout::{
        panel::{Child, StaticLinearPanel},
        StaticLinearLayout,
    },
};
use async_trait::async_trait;
use cursive::traits::Resizable;
use cursive::views::{
    Button, DummyView, EditView, EnableableView, Panel, ResizedView, TextContent, TextView,
};
use deluge_rpc::{InfoHash, Query, Session};
use serde::Deserialize;
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tokio::sync::Notify;
use tokio::task;
use tokio::time;

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

    pub owner: String,
    pub shared: bool,
    pub prioritize_first_last_pieces: bool,
    pub sequential_download: bool,
    pub super_seeding: bool,
    pub move_completed: bool,
    pub move_completed_path: String,
}

pub(super) struct OptionsData {
    selection: InfoHash,
    current_options_send: watch::Sender<OptionsQuery>,
    apply_notify: Arc<Notify>,
    owner: TextContent,
    pub current_options_recv: watch::Receiver<OptionsQuery>,
    pub pending_options: Arc<RwLock<Option<OptionsQuery>>>,
}

impl OptionsData {
    async fn apply(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let new_options = task::block_in_place(|| {
            let mut opts = self.pending_options.write().unwrap();
            assert!(opts.is_some());
            opts.take().unwrap()
        });

        self.current_options_send.send(new_options).unwrap();

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
                shared: Some(c.shared),
                prioritize_first_last_pieces: Some(c.prioritize_first_last_pieces),
                sequential_download: Some(c.sequential_download),
                super_seeding: Some(c.super_seeding),
                move_completed: Some(c.move_completed),
                move_completed_path: Some(c.move_completed_path.clone()),
                ..Default::default()
            }
        };

        session
            .set_torrent_options(&[self.selection], &options)
            .await
    }
}

#[async_trait]
impl ViewThread for OptionsData {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let deadline = time::Instant::now() + time::Duration::from_secs(1);

        if task::block_in_place(|| self.pending_options.read().unwrap().is_none()) {
            let hash = self.selection;
            let options = session.get_torrent_status::<OptionsQuery>(hash).await?;
            self.owner.set_content(&options.owner);
            self.current_options_send.send(options).unwrap();
        } else {
            let timeout = time::timeout_at(deadline, self.apply_notify.notified());
            if let Ok(()) = timeout.await {
                self.apply(session).await?;
            }
        }

        Ok(())
    }

    async fn reload(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        task::block_in_place(|| self.pending_options.write().unwrap().take());

        let hash = self.selection;
        let options = session.get_torrent_status::<OptionsQuery>(hash).await?;
        self.owner.set_content(&options.owner);
        self.current_options_send.send(options).unwrap();

        Ok(())
    }

    fn clear(&mut self) {
        // ¯\_(ツ)_/¯
        // I don't know what code I should be running here.
        // Like, logically, I should be greying out all the buttons.
        // But do I do that here...?
        // This code is very separated from being able to access the actual View objects.
    }
}

impl TabData for OptionsData {
    fn set_selection(&mut self, selection: InfoHash) {
        self.selection = selection;
    }
}

type FloatSpinView = SpinView<f64, std::ops::RangeFrom<f64>>;
type IntSpinView = SpinView<i64, std::ops::RangeFrom<i64>>;

type BandwidthLimits = (
    Child<FloatSpinView>,
    Child<FloatSpinView>,
    Child<IntSpinView>,
    Child<IntSpinView>,
);
type BandwidthLimitsPanel = StaticLinearPanel<BandwidthLimits>;
type BandwidthLimitsColumn = StaticLinearLayout<(TextView, BandwidthLimitsPanel)>;

pub(super) type RatioLimitControls = StaticLinearLayout<(FloatSpinView, LabeledCheckbox)>;

type SecondColumnElements = (
    LabeledCheckbox,
    LabeledCheckbox,
    EnableableView<Panel<RatioLimitControls>>,
    Panel<Button>,
);
type SecondColumn = StaticLinearLayout<SecondColumnElements>;

type OwnerTextView = StaticLinearLayout<(TextView, TextView)>;

type ThirdColumnElements = (
    OwnerTextView,
    LabeledCheckbox,
    LabeledCheckbox,
    LabeledCheckbox,
    LabeledCheckbox,
    LabeledCheckbox,
    ResizedView<EditView>,
);
type ThirdColumn = StaticLinearLayout<ThirdColumnElements>;

pub(super) type OptionsView = StaticLinearLayout<(
    BandwidthLimitsColumn,
    ResizedView<DummyView>,
    SecondColumn,
    ResizedView<DummyView>,
    ThirdColumn,
)>;

impl OptionsView {
    pub fn bandwidth_limits(&mut self) -> &mut BandwidthLimits {
        self.get_children_mut()
            .0
            .get_children_mut()
            .1
            .get_children_mut()
    }

    pub fn second_column(&mut self) -> &mut SecondColumnElements {
        self.get_children_mut().2.get_children_mut()
    }

    pub fn apply_button(&mut self) -> &mut Panel<Button> {
        &mut self.second_column().3
    }

    pub fn third_column(&mut self) -> &mut ThirdColumnElements {
        self.get_children_mut().4.get_children_mut()
    }

    pub fn move_completed_path(&mut self) -> &mut EditView {
        self.third_column().6.get_inner_mut()
    }

    pub(super) fn update(&mut self, opts: OptionsQuery) {
        let col1 = self.bandwidth_limits();
        col1.0.get_inner_mut().set_val(opts.max_download_speed);
        col1.1.get_inner_mut().set_val(opts.max_upload_speed);
        col1.2.get_inner_mut().set_val(opts.max_connections);
        col1.3.get_inner_mut().set_val(opts.max_upload_slots);

        let col2 = self.second_column();
        col2.0.set_checked(opts.auto_managed);
        col2.1.set_checked(opts.stop_at_ratio);
        col2.2.set_enabled(opts.stop_at_ratio);
        col2.3.get_inner_mut().disable();

        let ratio_limit_panel = col2.2.get_inner_mut().get_inner_mut().get_children_mut();
        ratio_limit_panel.0.set_val(opts.stop_ratio);
        ratio_limit_panel.1.set_checked(opts.remove_at_ratio);

        let col3 = self.third_column();
        col3.1.set_checked(opts.shared);
        col3.2.set_checked(opts.prioritize_first_last_pieces);
        col3.3.set_checked(opts.sequential_download);
        col3.4.set_checked(opts.super_seeding);
        col3.5.set_checked(opts.move_completed);

        let path = self.move_completed_path();
        path.set_enabled(opts.move_completed);
        path.set_content(&opts.move_completed_path);
    }
}

impl BuildableTabData for OptionsData {
    type V = OptionsView;

    fn view() -> (Self::V, Self) {
        let pending_options = Arc::new(RwLock::new(None));
        let (current_options_send, current_options_recv) = watch::channel(OptionsQuery::default());
        macro_rules! set {
            ($obj:ident.$field:ident) => {{
                let cloned_arc = $obj.clone();
                let current_options_recv = current_options_recv.clone();
                move |_, v| {
                    cloned_arc
                        .write()
                        .unwrap()
                        .get_or_insert_with(|| current_options_recv.borrow().clone())
                        .$field = v;
                }
            }};
        }

        let bandwidth_limits = {
            let down = SpinView::new(Some("Download Speed"), Some("kiB/s"), -1.0f64..)
                .on_modify(set!(pending_options.max_download_speed));

            let up = SpinView::new(Some("Upload Speed"), Some("kiB/s"), -1.0f64..)
                .on_modify(set!(pending_options.max_upload_speed));

            let peers = SpinView::new(Some("Connections"), None, -1i64..)
                .on_modify(set!(pending_options.max_connections));

            let slots = SpinView::new(Some("Upload Slots"), None, -1i64..)
                .on_modify(set!(pending_options.max_upload_slots));

            BandwidthLimitsPanel::vertical((down, up, peers, slots))
        };

        let col1 =
            BandwidthLimitsColumn::vertical((TextView::new("Bandwidth Limits"), bandwidth_limits));

        let apply_notify = Arc::new(Notify::new());

        let col2 = {
            let auto_managed =
                LabeledCheckbox::new("Auto Managed").on_change(set!(pending_options.auto_managed));

            let stop_at_ratio = LabeledCheckbox::new("Stop seed at ratio:")
                .on_change(set!(pending_options.stop_at_ratio));

            let ratio_limit_panel = {
                let spinner =
                    SpinView::new(None, None, 0.0f64..).on_modify(set!(pending_options.stop_ratio));

                let checkbox = LabeledCheckbox::new("Remove at ratio")
                    .on_change(set!(pending_options.remove_at_ratio));

                let layout = RatioLimitControls::vertical((spinner, checkbox));
                EnableableView::new(Panel::new(layout))
            };

            let apply_notify = apply_notify.clone();
            let apply = Button::new("Apply", move |_| apply_notify.notify_one());
            let apply_panel = Panel::new(apply);

            SecondColumn::vertical((auto_managed, stop_at_ratio, ratio_limit_panel, apply_panel))
        };

        let owner_content = TextContent::new("");

        let col3 = {
            let owner_text = TextView::new_with_content(owner_content.clone());

            let owner = OwnerTextView::horizontal((TextView::new("Owner: "), owner_text));

            let shared = LabeledCheckbox::new("Shared").on_change(set!(pending_options.shared));

            let prioritize_first_last_pieces = LabeledCheckbox::new("Prioritize First/Last")
                .on_change(set!(pending_options.prioritize_first_last_pieces));

            let sequential_download = LabeledCheckbox::new("Sequential Download")
                .on_change(set!(pending_options.sequential_download));

            let super_seeding = LabeledCheckbox::new("Super Seeding")
                .on_change(set!(pending_options.super_seeding));

            let move_completed = LabeledCheckbox::new("Move completed:")
                .on_change(set!(pending_options.move_completed));

            let edit_cb = {
                let cloned_arc = pending_options.clone();
                let current_options_recv = current_options_recv.clone();
                move |_: &mut cursive::Cursive, v: &str, _: usize| {
                    cloned_arc
                        .write()
                        .unwrap()
                        .get_or_insert_with(|| current_options_recv.borrow().clone())
                        .move_completed_path = String::from(v);
                }
            };

            let move_completed_path = EditView::new().on_edit(edit_cb).min_width(25);

            ThirdColumn::vertical((
                owner,
                shared,
                prioritize_first_last_pieces,
                sequential_download,
                super_seeding,
                move_completed,
                move_completed_path,
            ))
        };

        let view = OptionsView::horizontal((
            col1,
            DummyView.fixed_width(2),
            col2,
            DummyView.fixed_width(2),
            col3,
        ));

        let data = Self {
            selection: InfoHash::default(),
            current_options_send,
            current_options_recv,
            owner: owner_content,
            apply_notify,
            pending_options,
        };
        (view, data)
    }
}
