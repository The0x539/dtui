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
use crate::util::fmt_bytes;
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
}

struct StatusData {
    state: watch::Sender<TorrentState>,
    progress: Counter,
    down_speed: TextContent,
    up_speed: TextContent,
    down: TextContent,
    up: TextContent,
    seeds: TextContent,
    peers: TextContent,
    ratio: TextContent,
    availability: TextContent,
    seed_rank: TextContent,
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
            if v < 0.0 {
                return None;
            }
            assert!(v <= u64::MAX as f64);
            Some(v as u64)
        }

        let bytespeed = |v| fmt_bytes(v, "/s");
        let bytesize = |v| fmt_bytes(v, "");
        use std::convert::identity;

        let s_d = &mut self.status_data;

        let mut ryu_buf = ryu::Buffer::new();

        s_d.progress.set(status.progress as usize);
        s_d.state.broadcast(status.state).unwrap();
        s_d.down_speed.set_content(twovals_opt(bytespeed, status.download_payload_rate, kibs(status.max_download_speed)));
        s_d.up_speed.set_content(twovals_opt(bytespeed, status.upload_payload_rate, kibs(status.max_upload_speed)));
        s_d.down.set_content(twovals(bytesize, status.total_downloaded, status.total_payload_download));
        s_d.up.set_content(twovals(bytesize, status.total_uploaded, status.total_payload_upload));

        s_d.seeds.set_content(twovals(identity, status.num_seeds, status.total_seeds));
        s_d.peers.set_content(twovals(identity, status.num_peers, status.total_peers));
        s_d.ratio.set_content(ryu_buf.format(status.ratio));
        s_d.availability.set_content(ryu_buf.format(status.availability));
        s_d.seed_rank.set_content(status.seed_rank.to_string());

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


    fn label(s: &str) -> TextView {
        TextView::new(s).effect(cursive::theme::Effect::Bold)
    }

    fn value() -> (TextContent, TextView) {
        let c = TextContent::new("");
        (c.clone(), TextView::new_with_content(c).center())
    }

    let first_column_labels = LinearLayout::vertical()
        .child(label("Down Speed:"))
        .child(label("Up Speed:"))
        .child(label("Downloaded:"))
        .child(label("Uploaded:"));

    let (down_speed, down_speed_view) = value();
    let (up_speed, up_speed_view) = value();
    let (down, down_view) = value();
    let (up, up_view) = value();
    let first_column_values = LinearLayout::vertical()
        .child(down_speed_view)
        .child(up_speed_view)
        .child(down_view)
        .child(up_view);

    let second_column_labels = LinearLayout::vertical()
        .child(label("Seeds:"))
        .child(label("Peers:"))
        .child(label("Share Ratio:"))
        .child(label("Availability:"))
        .child(label("Seed Rank:"));

    let (seeds, seeds_view) = value();
    let (peers, peers_view) = value();
    let (ratio, ratio_view) = value();
    let (availability, availability_view) = value();
    let (seed_rank, seed_rank_view) = value();
    let second_column_values = LinearLayout::vertical()
        .child(seeds_view)
        .child(peers_view)
        .child(ratio_view)
        .child(availability_view)
        .child(seed_rank_view);

    let status = LinearLayout::horizontal()
        .child(first_column_labels)
        .child(DummyView.fixed_width(1))
        .child(first_column_values)
        .child(DummyView.fixed_width(3))
        .child(second_column_labels)
        .child(DummyView.fixed_width(1))
        .child(second_column_values);

    let view = LinearLayout::vertical()
        .child(progress_bar)
        .child(status);

    let data = StatusData {
        state: state_send,
        progress,
        down_speed,
        up_speed,
        down,
        up,
        seeds,
        peers,
        ratio,
        availability,
        seed_rank,
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
