use async_trait::async_trait;
use super::{column, TabData, BuildableTabData};
use deluge_rpc::{Query, Session, InfoHash};
use cursive::align::HAlign;
use cursive::views::{LinearLayout, TextContent, Button, DummyView};
use cursive::traits::Resizable;
use serde::Deserialize;
use crate::util;
use crate::Selection;

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
    selection: Selection,
    content: TextContent,
}

#[async_trait]
impl TabData for TrackersData {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = match self.get_selection() {
            Some(hash) => hash,
            None => return Ok(()),
        };

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

    async fn reload(&mut self, session: &Session, _: InfoHash) -> deluge_rpc::Result<()> {
        self.update(session).await
    }
}

impl BuildableTabData for TrackersData {
    type V = LinearLayout;

    fn view(selection: Selection) -> (Self::V, Self) {
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

        let data = TrackersData { selection, content: col_content };

        (col_view, data)
    }

    fn get_selection(&self) -> Option<InfoHash> {
        *self.selection.read().unwrap()
    }
}
