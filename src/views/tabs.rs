use std::sync::Arc;
use deluge_rpc::{Session, InfoHash, Query, TorrentState};
use super::thread::ViewThread;
use async_trait::async_trait;
use tokio::sync::{RwLock as AsyncRwLock, watch};
use tokio::time;
use serde::Deserialize;
use cursive_tabs::TabPanel;
use tokio::task::{self, JoinHandle};
use cursive::traits::*;
use cursive::view::ViewWrapper;
use crate::util::{ftime_or_dash, fmt_bytes};
use std::fmt::Display;
use cursive::utils::Counter;
use cursive::views::{
    TextView,
    LinearLayout,
    ProgressBar,
    DummyView,
    TextContent,
};

struct TorrentTabsViewThread {
    session: Arc<Session>,
    selected_recv: watch::Receiver<Option<InfoHash>>,
    status_data: StatusData,
}

pub struct TorrentTabsView {
    view: TabPanel<&'static str>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentStatus {
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
    total_seeds: u64,
    num_peers: u64,
    total_peers: u64,
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

struct StatusData {
    state: watch::Sender<TorrentState>,
    progress: Counter,

    columns: [TextContent; 3],
}

#[async_trait]
impl ViewThread for TorrentTabsViewThread {
    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let now = time::Instant::now();

        let hash = match *self.selected_recv.borrow() {
            Some(hash) => hash,
            None => return Ok(()),
        };

        let status = self.session.get_torrent_status::<TorrentStatus>(hash).await?;

        fn twovals<T, F: FnMut(T) -> U, U: Display>(mut f: F, a: T, b: T) -> String {
            format!("{} ({})", f(a), f(b))
        }

        fn twovals_opt<T, F: FnMut(T) -> U, U: Display>(mut f: F, a: T, b: Option<T>) -> String {
            match b {
                Some(b) => twovals(f, a, b),
                None => f(a).to_string(),
            }
        }

        // TODO: figure out a more elegant way of handling all these... kilobyte limits
        fn kibs(mut v: f64) -> Option<u64> {
            v *= 1024.0;
            assert_eq!(v, v.trunc());
            assert!(v.is_finite());
            if v <= 0.0 {
                return None;
            }
            assert!(v <= u64::MAX as f64);
            Some(v as u64)
        }

        let bytespeed = |v| fmt_bytes(v, "/s");
        let bytesize = |v| fmt_bytes(v, "");
        use std::convert::identity;

        let s_d = &mut self.status_data;

        s_d.progress.set(status.progress as usize);
        s_d.state.broadcast(status.state).unwrap();

        s_d.columns[0].set_content([
            twovals_opt(bytespeed, status.download_payload_rate, kibs(status.max_download_speed)),
            twovals_opt(bytespeed, status.upload_payload_rate, kibs(status.max_upload_speed)),
            twovals(bytesize, status.total_downloaded, status.total_payload_download),
            twovals(bytesize, status.total_uploaded, status.total_payload_upload),
        ].join("\n"));

        let mut ryu_buf = ryu::Buffer::new();

        s_d.columns[1].set_content([
            twovals(identity, status.num_seeds, status.total_seeds),
            twovals(identity, status.num_peers, status.total_peers),
            ryu_buf.format(status.ratio).to_owned(),
            ryu_buf.format(status.availability).to_owned(),
            status.seed_rank.to_string(),
        ].join("\n"));

        let last_seen_complete = match status.last_seen_complete {
            0 => String::from("-"),
            t => epochs::unix(t).unwrap().to_string(),
        };

        s_d.columns[2].set_content([
            ftime_or_dash(status.eta),
            ftime_or_dash(status.active_time),
            ftime_or_dash(status.seeding_time),
            ftime_or_dash(status.time_since_transfer),
            last_seen_complete,
        ].join("\n"));

        let new_selection = self.selected_recv.recv();
        tokio::select! {
            _ = new_selection => (),
            _ = time::delay_until(now + time::Duration::from_secs(1)) => (),
        }

        Ok(())
    }
}

fn status() -> (impl View, StatusData) {
    let (state_send, state_recv) = watch::channel(TorrentState::Downloading);

    let progress = Counter::new(0);
    let progress_bar = ProgressBar::new()
        .with_value(progress.clone())
        .with_label(move |val, (_min, _max)| format!("{} {}%", state_recv.borrow().as_str(), val));


    fn column(rows: &[&str]) -> (impl View, TextContent) {
        let labels = TextView::new(rows.join("\n")).effect(cursive::theme::Effect::Bold);

        let content = TextContent::new("");
        let values = TextView::new_with_content(content.clone()).center();

        let view = LinearLayout::horizontal()
            .child(labels)
            .child(DummyView.fixed_width(1))
            .child(values);

        (view, content)
    }

    let (first_column_view, first_column) = column(&[
        "Down Speed:",
        "Up Speed:",
        "Downloaded:",
        "Uploaded:",
    ]);

    let (second_column_view, second_column) = column(&[
        "Seeds:",
        "Peers:",
        "Share Ratio:",
        "Availability:",
        "Seed Rank:",
    ]);

    let (third_column_view, third_column) = column(&[
        "ETA Time:",
        "Active Time:",
        "Seeding Time:",
        "Last Transfer:",
        "Complete Seen:",
    ]);

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

impl TorrentTabsView {
    pub fn new(
        session: Arc<Session>,
        selected_recv: watch::Receiver<Option<InfoHash>>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Self {
        let (status_tab, status_data) = status();

        let details_tab = TextView::new("Torrent details (todo)");
        let options_tab = TextView::new("Torrent options (todo)");
        let files_tab = TextView::new("Torrent files (todo)");
        let peers_tab = TextView::new("Torrent peers (todo)");
        let trackers_tab = TextView::new("Torrent trackers (todo)");

        let thread_obj = TorrentTabsViewThread {
            session,
            selected_recv,
            status_data,
        };
        let thread = task::spawn(thread_obj.run(shutdown));

        let view = TabPanel::new()
            .with_tab("Status", status_tab)
            .with_tab("Details", details_tab)
            .with_tab("Options", options_tab)
            .with_tab("Files", files_tab)
            .with_tab("Peers", peers_tab)
            .with_tab("Trackers", trackers_tab)
            //.with_bar_placement(cursive_tabs::Placement::VerticalLeft)
            .with_active_tab("Status").unwrap();

        Self { view, thread }
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = task::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

impl ViewWrapper for TorrentTabsView {
    cursive::wrap_impl!(self.view: TabPanel<&'static str>);
}
