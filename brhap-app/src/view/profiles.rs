//! The profiles screen, following App.svelte:439-472.

use iced::Center;
use iced::widget::{Column, row, text, text_input};

use super::{TROUBLE, action};
use crate::message::Message;
use crate::state::{Brhap, describe};

pub(crate) fn screen(state: &Brhap) -> Column<'_, Message> {
    let mut page = Column::new().spacing(10).push(text("Save the last launch").size(16));

    page = match &state.store.last_launch {
        Some(last) => page
            .push(text(format!("Last launched: {}", describe(&last.ids, &last.options))).size(13))
            .push(
                row![
                    text_input("profile name", &state.profile_name)
                        .on_input(Message::ProfileName)
                        .on_submit(Message::SaveProfile)
                        .size(13)
                        .width(260),
                    action(
                        "save".to_string(),
                        (!state.profile_name.trim().is_empty()).then_some(Message::SaveProfile),
                    ),
                ]
                .spacing(6)
                .align_y(Center),
            ),
        None => page.push(
            text("Launch the game once, and what you selected shows up here to name.").size(13),
        ),
    };

    if !state.profile_error.is_empty() {
        page = page.push(text(state.profile_error.clone()).size(12).color(TROUBLE));
    }

    page = page.push(text("Saved profiles").size(16));

    if state.store.profiles.is_empty() {
        return page.push(text("Nothing saved yet.").size(13));
    }

    page.push(super::table::profiles(state))
}
