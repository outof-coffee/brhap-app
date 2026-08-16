//! The mod table. Four typed cells per row, no text stretch.

use iced::widget::{button, checkbox, container, table, text, tooltip};
use iced::{Element, Fill};
use iced_fonts::bootstrap;

use super::{CAUTION, GOOD, TROUBLE};
use crate::message::Message;
use crate::state::{Brhap, Row, Status};

/// Glyphs read at a glance, so they sit a little above the body text size.
const GLYPH: f32 = 15.0;

pub(crate) fn mods(state: &Brhap) -> Element<'_, Message> {
    table(
        [
            table::column(text(""), select).width(30),
            table::column(text("Mod"), name).width(Fill),
            table::column(text(""), over).width(40),
            table::column(text(""), status).width(40),
        ],
        state.rows(),
    )
    .padding_x(6)
    .padding_y(4)
    .into()
}

fn select(row: Row) -> Element<'static, Message> {
    let id = row.id.clone();
    checkbox(row.selected)
        .size(15)
        .on_toggle(move |checked| Message::Toggled(id.clone(), checked))
        .into()
}

/// The name is the only text in the table. The workshop id lives in the
/// tooltip, since it is what you paste into Steam but not what you read down
/// the list.
fn name(row: Row) -> Element<'static, Message> {
    tooltip(
        text(row.name).size(14),
        container(text(row.id).size(12)).padding(6).style(container::rounded_box),
        tooltip::Position::Right,
    )
    .into()
}

/// Lit when a local directory has replaced the workshop copy, and the same
/// control clears it again.
fn over(row: Row) -> Element<'static, Message> {
    let set = row.over.is_some();
    let glyph = bootstrap::pencil().size(GLYPH);
    let message = if set {
        Message::ClearOverride(row.id.clone())
    } else {
        Message::PickOverride(row.id.clone())
    };

    let control = button(if set { glyph.color(GOOD) } else { glyph })
        .style(button::text)
        .on_press(message);

    match row.over {
        Some(path) => tooltip(
            control,
            container(text(path.display().to_string()).size(12))
                .padding(6)
                .style(container::rounded_box),
            tooltip::Position::Left,
        )
        .into(),
        None => control.into(),
    }
}

fn status(row: Row) -> Element<'static, Message> {
    match row.status {
        Status::Pending => text("...").size(GLYPH).into(),
        Status::Failed => bootstrap::exclamation_triangle().size(GLYPH).color(TROUBLE).into(),
        Status::Unknown => bootstrap::exclamation_triangle().size(GLYPH).color(CAUTION).into(),
        Status::Ready => bootstrap::check().size(GLYPH).color(GOOD).into(),
        Status::Unmet => bootstrap::x().size(GLYPH).color(TROUBLE).into(),
        Status::Quiet => text("").into(),
    }
}
