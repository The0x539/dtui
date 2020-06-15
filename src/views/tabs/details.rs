use super::{column, TabData, BuildableTabData};
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{TextContent, LinearLayout, TextView};
use cursive::align::HAlign;
use crate::util;
use async_trait::async_trait;
use static_assertions::const_assert_eq;

#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentDetails {
    name: String,
    download_location: String,
    total_size: u64,
    num_files: u64,
    creator: String,
    comment: String,
    time_added: i64,
    completed_time: i64,
    num_pieces: u64,
    piece_length: u64,
}

pub(super) struct DetailsData {
    active_torrent: Option<InfoHash>,

    top: TextContent,
    left: TextContent,
    right: TextContent,
    bottom: TextContent,
}

#[async_trait]
impl TabData for DetailsData {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = self.active_torrent.unwrap();

        let details = session.get_torrent_status::<TorrentDetails>(hash).await?;

        self.top.set_content([
            details.name,
            details.download_location,
        ].join("\n"));

        self.left.set_content([
            util::fmt_bytes(details.total_size),
            details.num_files.to_string(),
            hash.to_string(),
        ].join("\n"));

        self.right.set_content([
            util::fdate(details.time_added),
            util::fdate_or_dash(details.completed_time),
            format!("{} ({})", details.num_pieces, util::fmt_bytes(details.piece_length).replace(".0", "")),
        ].join("\n"));

        self.bottom.set_content([
            details.creator,
            details.comment,
        ].join("\n"));

        Ok(())
    }

    async fn reload(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        self.active_torrent = Some(hash);
        self.update(session).await
    }
}

impl BuildableTabData for DetailsData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        let (top_view, top) = column(&["Name:", "Download Folder:"], HAlign::Left);
        let (left_view, left) = column(&["Total Size:", "Total Files:", "Hash:"], HAlign::Left);
        let (right_view, right) = column(&["Added:", "Completed:", "Pieces:"], HAlign::Left);
        let (bottom_view, bottom) = column(&["Created By:", "Comments:"], HAlign::Left);

        // We know ahead of time how wide the biggest thing on the left side will be. How fortunate.
        // Unfortunately, the TextView associated with `left` (a TextContent struct) is hard to access.
        // Rather than figuring that out, likely complicating `column()`'s interface,
        // we can just set `left`'s content to something just as wide as its eventual real content.
        const BLANK_INFOHASH: &'static str = "                                        ";
        const_assert_eq!(BLANK_INFOHASH.len(), 40);
        left.set_content(BLANK_INFOHASH);

        let middle_view = LinearLayout::horizontal()
            .child(left_view)
            .child(TextView::new(" ╷ \n │ \n ╵ "))
            .child(right_view);

        let view = LinearLayout::vertical()
            .child(top_view)
            .child(middle_view)
            .child(bottom_view);

        let data = DetailsData { active_torrent: None, top, left, right, bottom };

        (view, data)
    }
}
