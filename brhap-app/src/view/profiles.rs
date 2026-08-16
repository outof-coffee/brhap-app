//! The profiles screen, following App.svelte:439-472.

use brhap_core::LaunchOptions;
use iced::widget::{Column, row, text, text_input};
use iced::Center;

use super::{TROUBLE, action, danger, hint, link};
use crate::message::Message;
use crate::state::{Brhap, Flag};

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

    for profile in &state.store.profiles {
        page = page.push(
            row![
                link(profile.name.clone(), Some(Message::ApplyProfile(profile.name.clone()))),
                hint(describe(&profile.ids, &profile.options)),
                danger("delete".to_string(), Some(Message::DeleteProfile(profile.name.clone()))),
            ]
            .spacing(8)
            .align_y(Center),
        );
    }

    page
}

/// One-line summary of a saved launch, following `describe` in App.svelte:121.
fn describe(ids: &[String], options: &LaunchOptions) -> String {
    let flags: Vec<&str> = [
        (options.no_splash, Flag::NoSplash.label()),
        (options.skip_intro, Flag::SkipIntro.label()),
        (options.empty_world, Flag::EmptyWorld.label()),
    ]
    .into_iter()
    .filter_map(|(on, label)| on.then_some(label))
    .collect();

    if flags.is_empty() {
        format!("{} mod(s)", ids.len())
    } else {
        format!("{} mod(s), {}", ids.len(), flags.join(" "))
    }
}
