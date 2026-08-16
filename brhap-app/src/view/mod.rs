//! Turning state into widgets. Nothing here decides anything.

mod launch;
mod profiles;
mod table;

use brhap_core::LaunchOptions;
use iced::widget::{button, checkbox, column, image, row, text};
use iced::{Color, Element, Fill};

/// Bundled rather than reached for across crates, the same way the web UI
/// keeps its own copy.
const ICON: &[u8] = include_bytes!("../../assets/icon.png");
use iced_aw::{TabBar, TabLabel};

use crate::message::Message;
use crate::state::{Brhap, Flag, Screen};

/// The web UI's #a33, used for anything the user needs to notice.
pub(crate) const TROUBLE: Color = Color::from_rgb(0.667, 0.2, 0.2);
/// A dark wash of TROUBLE, backing the unmet requirements panel. Sits just
/// above the Dark theme's own background so the panel reads as a block.
pub(crate) const TROUBLE_WASH: Color = Color::from_rgb(0.19, 0.10, 0.10);
/// That panel's heading. On a dark ground the emphasis has to come from being
/// lighter than the body text rather than darker.
pub(crate) const TROUBLE_HEADING: Color = Color::from_rgb(0.95, 0.60, 0.57);
/// Requirements satisfied, or an override in force.
pub(crate) const GOOD: Color = Color::from_rgb(0.15, 0.55, 0.25);
/// We do not know yet, which is not the same as something being wrong.
pub(crate) const CAUTION: Color = Color::from_rgb(0.85, 0.5, 0.1);

pub(crate) fn view(state: &Brhap) -> Element<'_, Message> {
    let body = match state.screen {
        Screen::Launch => launch::screen(state),
        Screen::Profiles => profiles::screen(state),
    };

    column![header(state), tabs(state), body].spacing(10).padding(20).into()
}

/// Identity only. The right-hand side is deliberately empty: the app icon goes
/// there later, and these three rows exist so it has somewhere to sit without
/// the header being rebuilt again.
fn header(state: &Brhap) -> Element<'_, Message> {
    let loaded = if state.apply_note.is_empty() {
        "none".to_string()
    } else {
        state.apply_note.clone()
    };

    row![
        column![
            text("brhap").size(26),
            text("Bohemian Rhapsody").size(14),
            text(format!("Loaded: {loaded}")).size(12),
        ]
        .spacing(4)
        .width(Fill),
        // The 128px source drawn at 64, so it stays sharp on a retina display.
        image(image::Handle::from_bytes(ICON)).width(64).height(64),
    ]
    .into()
}

fn tabs(state: &Brhap) -> Element<'_, Message> {
    TabBar::new(Message::Show)
        .push(Screen::Launch, TabLabel::Text("Launch".to_string()))
        .push(Screen::Profiles, TabLabel::Text("Profiles".to_string()))
        .set_active_tab(&state.screen)
        .text_size(14.0)
        .into()
}

/// A startup parameter switch.
pub(crate) fn flag(current: &LaunchOptions, flag: Flag) -> Element<'static, Message> {
    checkbox(flag.get(current))
        .label(flag.label())
        .size(14)
        .text_size(13)
        .on_toggle(move |value| Message::Flagged(flag, value))
        .into()
}

/// Muted supporting text, the equivalent of the web UI's `.hint`.
pub(crate) fn hint(label: String) -> Element<'static, Message> {
    text(label).size(12).into()
}

/// A button that disables itself when there is nothing to do, which is how a
/// lookup already in flight stops a second click.
pub(crate) fn action(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12));
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}

/// A destructive action, matching `.destructive` in the web UI.
pub(crate) fn danger(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12)).style(button::danger);
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}

/// The same thing drawn as a link, matching `.link` in the web UI.
pub(crate) fn link(label: String, message: Option<Message>) -> Element<'static, Message> {
    let mut control = button(text(label).size(12)).style(button::text);
    if let Some(message) = message {
        control = control.on_press(message);
    }
    control.into()
}
