use super::{column, BuildableTabData, TabData};
use crate::util;
use crate::views::thread::ViewThread;
use async_trait::async_trait;
use cursive::align::HAlign;
use cursive::traits::Resizable;
use cursive::views::{Button, DummyView, LinearLayout, TextContent};
use deluge_rpc::{InfoHash, Query, Session};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
struct Tracker {/* we don't actually need any of this */}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Query)]
struct TrackersQuery {
    trackers: Vec<Tracker>,
    tracker_host: String,
    tracker_status: String,
    next_announce: i64,
    private: bool,
}

pub(super) struct TrackersData {
    selection: InfoHash,
    content: TextContent,
}

#[async_trait]
impl ViewThread for TrackersData {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = self.selection;
        let query = session.get_torrent_status::<TrackersQuery>(hash).await?;

        self.content.set_content(
            [
                query.trackers.len().to_string(),
                query.tracker_host,
                query.tracker_status,
                util::ftime_or_dash(query.next_announce),
                String::from(if query.private { "Yes" } else { "No" }),
            ]
            .join("\n"),
        );

        Ok(())
    }
}

impl TabData for TrackersData {
    fn set_selection(&mut self, selection: InfoHash) {
        self.selection = selection;
    }
}

impl BuildableTabData for TrackersData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let rows = [
            "Total Trackers:",
            "Current Tracker:",
            "Tracker Status:",
            "Next Announce:",
            "Private Torrent:",
        ];
        let (mut col_view, col_content) = column(&rows, HAlign::Center);

        let button = Button::new("Edit Trackers", |_| todo!());

        let left_col = LinearLayout::vertical()
            .child(col_view.remove_child(0).unwrap())
            .child(DummyView.fixed_height(1))
            .child(button);

        col_view.insert_child(0, left_col);

        let data = TrackersData {
            selection: InfoHash::default(),
            content: col_content,
        };

        (col_view, data)
    }
}
