//! The settings screen. One setting for now, edited in place from its row.

use iced::widget::{Column, text};

use super::{CAUTION, TROUBLE};
use crate::message::Message;
use crate::state::Brhap;

pub(crate) fn screen(state: &Brhap) -> Column<'_, Message> {
    let mut page = Column::new().spacing(10).push(text("Settings").size(16));

    if !state.settings_error.is_empty() {
        page = page.push(text(state.settings_error.clone()).size(12).color(TROUBLE));
    }

    page = page.push(super::table::settings(state));

    // The saved key wins quietly, so an environment key that is set and going
    // unused would otherwise look like it was simply not working.
    if state.key_shadows_env {
        page = page.push(
            text(format!(
                "The saved key is in use. {} is set as well and is being ignored.",
                brhap_core::KEY_VAR
            ))
            .size(12)
            .color(CAUTION),
        );
    }

    page
}
