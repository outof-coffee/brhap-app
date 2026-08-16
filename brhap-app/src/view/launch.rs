//! The launch screen: the mod list, unmet requirements, parameters, launch.

use iced::widget::text::Text;
use iced::widget::{Column, button, container, row, scrollable, text, tooltip};
use iced::{Center, Element, Fill};
use iced_fonts::bootstrap;

use super::{TROUBLE, action, danger, flag, hint, link};
use crate::message::Message;
use crate::state::{Brhap, Flag};

pub(crate) fn screen(state: &Brhap) -> Column<'_, Message> {
    let mut page = Column::new().spacing(10);

    if !state.load_error.is_empty() {
        page = page.push(text(format!("Could not load: {}", state.load_error)));
    }

    page = page
        .push(
            text(format!(
                "{} of {} selected. Dependency lookups happen only when you click.",
                state.selected_count(),
                state.mod_ids.len()
            ))
            .size(13),
        )
        .push(toolbar(state))
        .push(scrollable(super::table::mods(state)).height(Fill));

    let unmet = state.unmet();
    if !unmet.is_empty() {
        page = page.push(text("Unmet requirements").size(16));
        for entry in unmet {
            page = page.push(text(entry).size(12).color(TROUBLE));
        }
    }

    // All five are LaunchOptions fields. The web UI split them only because
    // Intel Mode and Steam Overlay change how the game is started rather than
    // what argv it gets.
    page = page.push(text("Startup parameters").size(16)).push(
        row![
            flag(&state.options, Flag::NoSplash),
            flag(&state.options, Flag::SkipIntro),
            flag(&state.options, Flag::EmptyWorld),
            flag(&state.options, Flag::IntelMode),
            flag(&state.options, Flag::SteamOverlay),
        ]
        .spacing(16),
    );

    // Follows App.svelte:424-438: no plan, nothing to say about launching.
    if let Some(plan) = &state.plan {
        let pid = match state.pid {
            Some(pid) => format!(" (pid {pid})"),
            None => String::new(),
        };

        page = page
            .push(text("Launch").size(16))
            .push(
                text(format!(
                    "{} symlink(s) will be created in the game directory. Status: {}{}",
                    plan.symlinks.len(),
                    state.status,
                    pid
                ))
                .size(13),
            )
            .push(
                row![
                    action("launch".to_string(), (!state.running).then_some(Message::Launch)),
                    danger("stop".to_string(), state.running.then_some(Message::Stop)),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .push(link(
                if state.show_preview { "hide details".to_string() } else { "details".to_string() },
                Some(Message::TogglePreview),
            ));

        if !state.launch_error.is_empty() {
            page = page.push(text(state.launch_error.clone()).size(12).color(TROUBLE));
        }

        if state.show_preview {
            page = page.push(text(plan.preview.clone()).size(11));
        }
    }

    page
}

/// Icon-only, so each control says what it does on hover.
fn tool(
    glyph: Text<'static>,
    explain: &'static str,
    destructive: bool,
    message: Option<Message>,
) -> Element<'static, Message> {
    let style = if destructive { button::danger } else { button::secondary };
    let mut control = button(glyph.size(15.0)).style(style);
    if let Some(message) = message {
        control = control.on_press(message);
    }

    tooltip(
        control,
        container(text(explain).size(12)).padding(6).style(container::rounded_box),
        tooltip::Position::Bottom,
    )
    .into()
}

fn toolbar(state: &Brhap) -> Element<'_, Message> {
    let idle = !state.rescanning;
    let refresh = if state.api_available {
        "Rescan installed mods, then resolve everything through the Steam API"
    } else {
        "Rescan installed mods"
    };

    row![
        tool(
            bootstrap::arrow_clockwise(),
            refresh,
            false,
            idle.then_some(Message::Refresh),
        ),
        tool(
            bootstrap::hourglass(),
            "Look the selected mods up again, 3 seconds apart",
            false,
            (idle && state.selected_count() > 0 && state.pending.is_empty())
                .then_some(Message::Refetch),
        ),
        tool(
            bootstrap::trash(),
            "Discard every cached dependency lookup",
            true,
            idle.then_some(Message::ResetCache),
        ),
        hint(state.rescan_note.clone()),
    ]
    .spacing(6)
    .align_y(Center)
    .into()
}
