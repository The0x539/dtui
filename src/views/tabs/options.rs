#![allow(unused)]

use super::{column, TabData};
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{TextContent, LinearLayout, TextView, Checkbox, DummyView, Button};
use cursive::traits::Resizable;
use cursive::align::HAlign;
use crate::util;
use async_trait::async_trait;
use static_assertions::const_assert_eq;
use crate::views::spin::SpinView;
use tokio::sync::watch;

#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentOptions {
}

pub(super) struct OptionsData {
}

#[async_trait]
impl TabData for OptionsData {
    type V = LinearLayout;

    fn view() -> (Self::V, Self) {
        fn labeled_checkbox(label: &str) -> LinearLayout {
            // TODO: make this into a full-fledged view class
            // that way, it can have the full Checkbox interface
            LinearLayout::horizontal()
                .child(Checkbox::new())
                .child(TextView::new(label))
        }

        let col1 = LinearLayout::vertical()
            .child(SpinView::new(Some("Download Speed"), -1.0f64..).with_label("kiB/s"))
            .child(SpinView::new(Some("Upload Speed"), -1.0f64..).with_label("kiB/s"));

        let col2 = LinearLayout::vertical()
            .child(SpinView::new(Some("Connections"), -1i64..))
            .child(SpinView::new(Some("Upload Slots"), -1i64..));

        let col3 = LinearLayout::vertical()
            .child(labeled_checkbox("Auto Managed"))
            .child(labeled_checkbox("Stop seed at ratio:"))
            .child(SpinView::new(None, 0.0f64..))
            .child(labeled_checkbox("Remove at ratio"))
            .child(Button::new("Apply", |_| ()));

        let view = LinearLayout::horizontal()
            .child(col1)
            .child(DummyView.fixed_width(2))
            .child(col2)
            .child(DummyView.fixed_width(2))
            .child(col3);

        let data = OptionsData {  };
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        Ok(())
    }
}

