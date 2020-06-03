#![allow(unused)]

use super::{column, TabData};
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{TextContent, LinearLayout, TextView};
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
    test_val_recv: watch::Receiver<i64>,
}

#[async_trait]
impl TabData for OptionsData {
    type V = SpinView<i64, std::ops::Range<i64>>;

    fn view() -> (Self::V, Self) {
        let (test_val_send, test_val_recv) = watch::channel(0i64);

        let mut view = SpinView::new(Some(String::from("test")), -1..i64::MAX);

        view.set_on_modify(move |v| test_val_send.broadcast(v).unwrap());
        view.set_label("kiB/s");

        let data = OptionsData { test_val_recv };
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        Ok(())
    }
}

