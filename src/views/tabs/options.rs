#![allow(unused)]

use super::{column, TabData};
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{TextContent, LinearLayout, TextView, Checkbox, DummyView, Button, Panel, EnableableView};
use cursive::traits::Resizable;
use cursive::align::HAlign;
use crate::util;
use async_trait::async_trait;
use static_assertions::const_assert_eq;
use crate::views::spin::SpinView;
use tokio::sync::watch;
use crate::views::linear_panel::LinearPanel;

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

        let bandwidth_limits = LinearPanel::vertical()
            .child(SpinView::new(Some("Download Speed"), Some("kiB/s"), -1.0f64..), None)
            .child(SpinView::new(Some("Upload Speed"), Some("kiB/s"), -1.0f64..), None)
            .child(SpinView::new(Some("Connections"), None, -1i64..), None)
            .child(SpinView::new(Some("Upload Slots"), None, -1i64..), None);

        let col1 = LinearLayout::vertical()
            .child(TextView::new("Bandwidth Limits"))
            .child(bandwidth_limits)
            .max_width(40);

        let ratio_limit_panel = {
            let spinner = SpinView::new(None, None, 0.0f64..);
            let checkbox = labeled_checkbox("Remove at ratio");
            let layout = LinearLayout::vertical().child(spinner).child(checkbox);
            let panel = Panel::new(layout).max_width(30);
            EnableableView::new(panel).disabled()
        };

        let col2 = LinearLayout::vertical()
            .child(labeled_checkbox("Auto Managed"))
            .child(labeled_checkbox("Stop seed at ratio:"))
            .child(ratio_limit_panel)
            .child(Button::new("Apply", |_| ()));

        let view = LinearLayout::horizontal()
            .child(col1)
            .child(DummyView.fixed_width(2))
            .child(col2);

        let data = OptionsData {  };
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        Ok(())
    }
}

