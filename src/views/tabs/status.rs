use super::{column, BuildableTabData, TabData};
use crate::util;
use crate::views::thread::ViewThread;
use async_trait::async_trait;
use cursive::align::HAlign;
use cursive::traits::Resizable;
use cursive::utils::Counter;
use cursive::views::{DummyView, LinearLayout, ProgressBar, TextContent};
use deluge_rpc::{InfoHash, Query, Session, TorrentState};
use serde::Deserialize;
use tokio::sync::watch;

#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentStatus {
    state: TorrentState,
    progress: f32,

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
    selection: InfoHash,

    progress_label_send: watch::Sender<String>,
    progress_val: Counter,

    columns: [TextContent; 3],
}

#[async_trait]
impl ViewThread for StatusData {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = self.selection;
        let status = session.get_torrent_status::<TorrentStatus>(hash).await?;

        self.progress_val.set((status.progress * 100.0) as usize);
        let label = format!(
            "{} {}%",
            status.state,
            util::fmt::percentage(status.progress),
        );
        self.progress_label_send.broadcast(label).unwrap();

        self.columns[0].set_content(
            [
                util::fmt::speed_pair(status.download_payload_rate, status.max_download_speed),
                util::fmt::speed_pair(status.upload_payload_rate, status.max_upload_speed),
                util::fmt::pair(
                    util::fmt::bytes,
                    status.total_downloaded,
                    Some(status.total_payload_download),
                ),
                util::fmt::pair(
                    util::fmt::bytes,
                    status.total_uploaded,
                    Some(status.total_payload_upload),
                ),
            ]
            .join("\n"),
        );

        let nonnegative = |n: i64| (n >= 0).then_some(n as u64);
        let mut ryu_buf = ryu::Buffer::new();

        self.columns[1].set_content(
            [
                util::fmt::pair(|x| x, status.num_seeds, nonnegative(status.total_seeds)),
                util::fmt::pair(|x| x, status.num_peers, nonnegative(status.total_peers)),
                ryu_buf.format(status.ratio).to_owned(),
                ryu_buf.format(status.availability).to_owned(),
                status.seed_rank.to_string(),
            ]
            .join("\n"),
        );

        self.columns[2].set_content(
            [
                util::fmt::time_or_dash(status.eta),
                util::fmt::time_or_dash(status.active_time),
                util::fmt::time_or_dash(status.seeding_time),
                util::fmt::time_or_dash(status.time_since_transfer),
                util::fmt::date_or_dash(status.last_seen_complete),
            ]
            .join("\n"),
        );

        Ok(())
    }

    fn clear(&mut self) {
        self.progress_val.set(0);
        self.progress_label_send.broadcast(String::new()).unwrap();
        self.columns.iter_mut().for_each(|c| c.set_content(""));
    }
}

impl TabData for StatusData {
    fn set_selection(&mut self, selection: InfoHash) {
        self.selection = selection;
    }
}

impl BuildableTabData for StatusData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let (progress_label_send, progress_label_recv) = watch::channel(String::new());

        let progress_val = Counter::new(0);
        let progress_bar = ProgressBar::new()
            .max(10000)
            .with_value(progress_val.clone())
            .with_label(move |_, _| progress_label_recv.borrow().clone())
            .full_width();

        let (col1, col2, col3) = (
            ["Down Speed:", "Up Speed:", "Downloaded:", "Uploaded:"],
            [
                "Seeds:",
                "Peers:",
                "Share Ratio:",
                "Availability:",
                "Seed Rank:",
            ],
            [
                "ETA Time:",
                "Active Time:",
                "Seeding Time:",
                "Last Transfer:",
                "Complete Seen:",
            ],
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

        let view = LinearLayout::vertical().child(progress_bar).child(status);

        let data = StatusData {
            selection: InfoHash::default(),
            progress_label_send,
            progress_val,
            columns: [col1_content, col2_content, col3_content],
        };

        (view, data)
    }
}
