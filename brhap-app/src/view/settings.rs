//! The settings screen. One setting for now, edited in place from its row.

use iced::widget::{Column, text};

use super::TROUBLE;
use crate::message::Message;
use crate::state::Brhap;

pub(crate) fn screen(state: &Brhap) -> Column<'_, Message> {
    let mut page = Column::new().spacing(10).push(text("Settings").size(16));

    if !state.settings_error.is_empty() {
        page = page.push(text(state.settings_error.clone()).size(12).color(TROUBLE));
    }

    page.push(super::table::settings(state))
}
