use super::{column, TabData};
use cursive::traits::Resizable;
use deluge_rpc::{Query, TorrentState, Session, InfoHash};
use serde::Deserialize;
use tokio::sync::watch;
use cursive::views::{DummyView, TextContent, LinearLayout, ProgressBar};
use cursive::align::HAlign;
use cursive::utils::Counter;
use crate::util;
use async_trait::async_trait;

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

#[async_trait]
impl TabData for StatusData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let (state_send, state_recv) = watch::channel(TorrentState::Downloading);

        let progress = Counter::new(0);
        let progress_bar = ProgressBar::new()
            .with_value(progress.clone())
            .with_label(move |val, (_min, _max)| format!("{} {}%", state_recv.borrow().as_str(), val));

        let (col1, col2, col3) = (
            ["Down Speed:", "Up Speed:", "Downloaded:", "Uploaded:"],
            ["Seeds:", "Peers:", "Share Ratio:", "Availability:", "Seed Rank:"],
            ["ETA Time:", "Active Time:", "Seeding Time:", "Last Transfer:", "Complete Seen:"],
        );

        let (col1_view, col1_content) = column(&col1, HAlign::Center);
        let (col2_view, col2_content) = column(&col2, HAlign::Center);
        let (col3_view, col3_content) = column(&col3, HAlign::Center);

        let status = LinearLayout::horizontal()
            .child(col1_view)
            .child(DummyView.fixed_width(3))
            .child(col2_view)
            .child(DummyView.fixed_width(3))
            .child(col3_view);

        let view = LinearLayout::vertical()
            .child(progress_bar)
            .child(status);

        let data = StatusData {
            state: state_send,
            progress,
            columns: [col1_content, col2_content, col3_content],
        };

        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        let status = session.get_torrent_status::<TorrentStatus>(hash).await?;

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

        Ok(())
    }
}

