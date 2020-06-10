use async_trait::async_trait;
use super::{column, TabData};
use deluge_rpc::{Query, Session, InfoHash};
use cursive::align::HAlign;
use cursive::views::{LinearLayout, TextContent, Button};
use serde::Deserialize;
use crate::util;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
struct Tracker { /* we don't actually need any of this */ }

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Query)]
struct TrackersQuery {
    trackers: Vec<Tracker>,
    tracker_host: String,
    tracker_status: String,
    next_announce: i64,
    private: bool,
}

pub(super) struct TrackersData {
    active_torrent: Option<InfoHash>,
    content: TextContent,
}

#[async_trait]
impl TabData for TrackersData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let rows = [
            "Total Trackers:",
            "Current Tracker:",
            "Tracker Status:",
            "Next Announce:",
            "Private Torrent:",
        ];
        let (col_view, col_content) = column(&rows, HAlign::Center);

        let button = Button::new("Edit Trackers", |_| todo!());

        let view = LinearLayout::vertical().child(col_view).child(button);
        let data = TrackersData { active_torrent: None, content: col_content };

        (view, data)
    }

    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = self.active_torrent.unwrap();

        let query = session.get_torrent_status::<TrackersQuery>(hash).await?;

        self.content.set_content([
            query.trackers.len().to_string(),
            query.tracker_host,
            query.tracker_status,
            util::ftime_or_dash(query.next_announce),
            String::from(if query.private { "Yes" } else { "No" }),
        ].join("\n"));

        Ok(())
    }

    async fn reload(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        self.active_torrent = Some(hash);
        self.update(session).await
    }
}
