use super::column;
use cursive::traits::View;
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{DummyView, TextContent, LinearLayout, TextView};
use cursive::align::HAlign;
use crate::util;

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
    top: TextContent,
    left: TextContent,
    right: TextContent,
}

impl DetailsData {
    pub(super) async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        let details = session.get_torrent_status::<TorrentDetails>(hash).await?;

        self.top.set_content([
            details.name,
            details.download_location,
        ].join("\n"));

        self.left.set_content([
            util::fmt_bytes(details.total_size),
            details.num_files.to_string(),
            hash.to_string(),
            details.creator,
            details.comment,
        ].join("\n"));

        self.right.set_content([
            util::fdate(details.time_added),
            util::fdate_or_dash(details.completed_time),
            format!("{} ({})", details.num_pieces, util::fmt_bytes(details.piece_length).replace(".0", "")),
        ].join("\n"));

        Ok(())
    }
}

pub(super) fn details() -> (impl View, DetailsData) {
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
