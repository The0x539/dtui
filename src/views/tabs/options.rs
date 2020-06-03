#![allow(unused)]

use super::{column, TabData};
use deluge_rpc::{Query, InfoHash, Session};
use serde::Deserialize;
use cursive::views::{TextContent, LinearLayout, TextView, NamedView};
use cursive::align::HAlign;
use crate::util;
use async_trait::async_trait;
use static_assertions::const_assert_eq;
use crate::views::spin::SpinView;

#[derive(Debug, Clone, Deserialize, Query)]
struct TorrentOptions {
}

pub(super) struct OptionsData {
}

#[async_trait]
impl TabData for OptionsData {
    type V = NamedView<SpinView<i64, std::ops::Range<i64>>>;

    fn view() -> (Self::V, Self) {
        let view = SpinView::new(Some(String::from("test")), -1..i64::MAX);
        let data = OptionsData {};
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        Ok(())
    }
}

