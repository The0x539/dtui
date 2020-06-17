use cursive::view::ViewWrapper;
use cursive::views::{LinearLayout, TextView, DummyView};

use crate::views::labeled_checkbox::LabeledCheckbox;
use crate::form::Form;

pub(crate) struct RemoveTorrentPrompt {
    inner: LinearLayout,
}

const WARNING_TRIANGLE: &str = concat!(
    "   ▄   \n",
    "  ▟▀▙  \n",
    " ▟█▄█▙ \n",
    "▟██▄██▙",
);

impl RemoveTorrentPrompt {
    pub fn new_single(name: impl AsRef<str>) -> Self {
        let top = LinearLayout::horizontal()
            .child(TextView::new(WARNING_TRIANGLE))
            .child(DummyView)
            .child(TextView::new("\nRemove the selected torrent?").center());

        let content = LinearLayout::vertical()
            .child(top)
            .child(TextView::new(name.as_ref()).center())
            .child(LabeledCheckbox::new("Include downloaded files"));

        Self { inner: content }
    }
}

impl ViewWrapper for RemoveTorrentPrompt {
    cursive::wrap_impl!(self.inner: LinearLayout);
}

impl Form for RemoveTorrentPrompt {
    type Data = bool;

    fn into_data(self) -> Self::Data {
        self.inner
            .get_child(2)
            .unwrap()
            .downcast_ref::<LabeledCheckbox>()
            .unwrap()
            .is_checked()
    }
}
