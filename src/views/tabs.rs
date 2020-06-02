use std::sync::Arc;
use deluge_rpc::{Session, InfoHash, Query};
use super::thread::ViewThread;
use async_trait::async_trait;
use tokio::sync::{RwLock as AsyncRwLock, watch};
use tokio::time;
use serde::Deserialize;
use cursive_tabs::TabPanel;
use tokio::task::{self, JoinHandle};
use cursive::traits::*;
use cursive::view::ViewWrapper;
use crate::util;
use cursive::align::HAlign;
use cursive::views::{
    TextView,
    LinearLayout,
    DummyView,
    TextContent,
};

fn column(rows: &[&str], h_align: HAlign) -> (impl View, TextContent) {
    let labels = TextView::new(rows.join("\n")).effect(cursive::theme::Effect::Bold);

    let content = TextContent::new("");
    let values = TextView::new_with_content(content.clone()).h_align(h_align);

    let view = LinearLayout::horizontal()
        .child(labels)
        .child(DummyView.fixed_width(1))
        .child(values);

    (view, content)
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum Tab { Status, Details, Options, Files, Peers, Trackers }

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

struct TorrentTabsViewThread {
    session: Arc<Session>,
    selected_recv: watch::Receiver<Option<InfoHash>>,
    active_tab_recv: watch::Receiver<Tab>,
    status_data: status::StatusData,
    details_data: DetailsData,
}

pub(crate) struct TorrentTabsView {
    view: TabPanel<Tab>,
    active_tab: Tab,
    active_tab_send: watch::Sender<Tab>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

mod status {
    use super::column;
    use cursive::traits::{View, Resizable};
    use deluge_rpc::{Query, TorrentState};
    use serde::Deserialize;
    use tokio::sync::watch;
    use cursive::views::{DummyView, TextContent, LinearLayout, ProgressBar};
    use cursive::align::HAlign;
    use cursive::utils::Counter;
    use crate::util;

    #[derive(Debug, Clone, Deserialize, Query)]
    pub(crate) struct TorrentStatus {
        state: TorrentState,
        progress: f64,

        download_payload_rate: u64,
        max_download_speed: f64,
        upload_payload_rate: u64,
        max_upload_speed: f64,
        #[serde(rename = "all_time_download")] // wtf
        total_downloaded: u64,
        total_payload_download: u64,
        total_uploaded: u64,
        total_payload_upload: u64,

        num_seeds: u64,
        total_seeds: i64,
        num_peers: u64,
        total_peers: i64,
        ratio: f64,
        #[serde(rename = "distributed_copies")]
        availability: f64,
        seed_rank: u64,

        eta: i64,
        active_time: i64,
        seeding_time: i64,
        time_since_transfer: i64,
        last_seen_complete: i64,
    }

    pub(super) struct StatusData {
        state: watch::Sender<TorrentState>,
        progress: Counter,

        columns: [TextContent; 3],
    }

    impl StatusData {
        pub(super) fn update(&mut self, status: TorrentStatus) {
            self.progress.set(status.progress as usize);
            self.state.broadcast(status.state).unwrap();

            self.columns[0].set_content([
                util::fmt_speed_pair(status.download_payload_rate, status.max_download_speed),
                util::fmt_speed_pair(status.upload_payload_rate, status.max_upload_speed),
                util::fmt_pair(util::fmt_bytes, status.total_downloaded, Some(status.total_payload_download)),
                util::fmt_pair(util::fmt_bytes, status.total_uploaded, Some(status.total_payload_upload)),
            ].join("\n"));

            let mut ryu_buf = ryu::Buffer::new();

            let nonnegative = |n: i64| (n >= 0).then_some(n as u64);

            self.columns[1].set_content([
                util::fmt_pair(|x| x, status.num_seeds, nonnegative(status.total_seeds)),
                util::fmt_pair(|x| x, status.num_peers, nonnegative(status.total_peers)),
                ryu_buf.format(status.ratio).to_owned(),
                ryu_buf.format(status.availability).to_owned(),
                status.seed_rank.to_string(),
            ].join("\n"));

            self.columns[2].set_content([
                util::ftime_or_dash(status.eta),
                util::ftime_or_dash(status.active_time),
                util::ftime_or_dash(status.seeding_time),
                util::ftime_or_dash(status.time_since_transfer),
                util::fdate_or_dash(status.last_seen_complete),
            ].join("\n"));
        }
    }

    pub(super) fn status() -> (impl View, StatusData) {
        let (state_send, state_recv) = watch::channel(TorrentState::Downloading);

        let progress = Counter::new(0);
        let progress_bar = ProgressBar::new()
            .with_value(progress.clone())
            .with_label(move |val, (_min, _max)| format!("{} {}%", state_recv.borrow().as_str(), val));

        let (first_column_view, first_column) = column(&[
            "Down Speed:",
            "Up Speed:",
            "Downloaded:",
            "Uploaded:",
        ], HAlign::Center);

        let (second_column_view, second_column) = column(&[
            "Seeds:",
            "Peers:",
            "Share Ratio:",
            "Availability:",
            "Seed Rank:",
        ], HAlign::Center);

        let (third_column_view, third_column) = column(&[
            "ETA Time:",
            "Active Time:",
            "Seeding Time:",
            "Last Transfer:",
            "Complete Seen:",
        ], HAlign::Center);

        let status = LinearLayout::horizontal()
            .child(first_column_view)
            .child(DummyView.fixed_width(3))
            .child(second_column_view)
            .child(DummyView.fixed_width(3))
            .child(third_column_view);

        let view = LinearLayout::vertical()
            .child(progress_bar)
            .child(status);

        let data = StatusData {
            state: state_send,
            progress,
            columns: [first_column, second_column, third_column],
        };

        (view, data)
    }

}


#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentDetails {
    name: String,
    download_location: String,
    total_size: u64,
    num_files: u64,
    // Don't need to query for this
    // hash: InfoHash,
    creator: String,
    comment: String,
    time_added: i64,
    completed_time: i64,
    num_pieces: u64,
    piece_length: u64,
}

struct DetailsData {
    top: TextContent,
    left: TextContent,
    right: TextContent,
}

#[async_trait]
impl ViewThread for TorrentTabsViewThread {
    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let now = time::Instant::now();

        let hash = match *self.selected_recv.borrow() {
            Some(hash) => hash,
            None => return Ok(()),
        };

        let active_tab = *self.active_tab_recv.borrow();

        match active_tab {
            Tab::Status => self.update_status_tab(hash).await?,
            Tab::Details => self.update_details_tab(hash).await?,
            _ => (),
        }

        let new_selection = self.selected_recv.recv();
        let new_active_tab = self.active_tab_recv.recv();
        tokio::select! {
            _ = new_selection => (),
            _ = new_active_tab => (),
            _ = time::delay_until(now + time::Duration::from_secs(1)) => (),
        }

        Ok(())
    }
}

impl TorrentTabsViewThread {
    async fn update_status_tab(&mut self, hash: InfoHash) -> deluge_rpc::Result<()> {
        let status = self.session.get_torrent_status::<status::TorrentStatus>(hash).await?;

        self.status_data.update(status);

        Ok(())
    }

    async fn update_details_tab(&mut self, hash: InfoHash) -> deluge_rpc::Result<()> {
        let details = self.session.get_torrent_status::<TorrentDetails>(hash).await?;

        let d_d = &mut self.details_data;

        d_d.top.set_content([
            details.name,
            details.download_location,
        ].join("\n"));

        d_d.left.set_content([
            util::fmt_bytes(details.total_size),
            details.num_files.to_string(),
            hash.to_string(),
            details.creator,
            details.comment,
        ].join("\n"));

        d_d.right.set_content([
            util::fdate(details.time_added),
            util::fdate_or_dash(details.completed_time),
            format!("{} ({})", details.num_pieces, util::fmt_bytes(details.piece_length).replace(".0", "")),
        ].join("\n"));

        Ok(())
    }
}

fn details() -> (impl View, DetailsData) {
    let (top_view, top) = column(&["Name:", "Download Folder:"], HAlign::Left);
    let (left_view, left) = column(&[
        "Total Size:",
        "Total Files:",
        "Hash:",
        "Created By:",
        "Comments:",
    ], HAlign::Left);
    let (right_view, right) = column(&["Added:", "Completed:", "Pieces:"], HAlign::Left);

    let bottom_view = LinearLayout::horizontal()
        .child(left_view)
        .child(TextView::new([" │ "; 3].join("\n")))
        .child(right_view);

    let view = LinearLayout::vertical()
        .child(top_view)
        .child(DummyView)
        .child(bottom_view);

    let data = DetailsData { top, left, right };

    (view, data)
}

impl TorrentTabsView {
    pub(crate) fn new(
        session: Arc<Session>,
        selected_recv: watch::Receiver<Option<InfoHash>>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Self {
        let (status_tab, status_data) = status::status();
        let (details_tab, details_data) = details();

        let active_tab = Tab::Status;
        let (active_tab_send, active_tab_recv) = watch::channel(active_tab);

        let options_tab = TextView::new("Torrent options (todo)");
        let files_tab = TextView::new("Torrent files (todo)");
        let peers_tab = TextView::new("Torrent peers (todo)");
        let trackers_tab = TextView::new("Torrent trackers (todo)");

        let thread_obj = TorrentTabsViewThread {
            session,
            selected_recv,
            active_tab_recv,
            status_data,
            details_data,
        };
        let thread = task::spawn(thread_obj.run(shutdown));

        let view = TabPanel::new()
            .with_tab(Tab::Status, status_tab)
            .with_tab(Tab::Details, details_tab)
            .with_tab(Tab::Options, options_tab)
            .with_tab(Tab::Files, files_tab)
            .with_tab(Tab::Peers, peers_tab)
            .with_tab(Tab::Trackers, trackers_tab)
            //.with_bar_placement(cursive_tabs::Placement::VerticalLeft)
            .with_active_tab(active_tab).unwrap();

        Self { view, active_tab, active_tab_send, thread }
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = task::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

use cursive::event::{Event, EventResult};

impl ViewWrapper for TorrentTabsView {
    cursive::wrap_impl!(self.view: TabPanel<Tab>);

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        let old_tab = self.active_tab;
        let result = self.view.on_event(event);
        if let Some(new_tab) = self.view.active_tab() {
            if new_tab != old_tab {
                self.active_tab = new_tab;
                self.active_tab_send.broadcast(new_tab).unwrap();
            }
        }
        result
    }
}
