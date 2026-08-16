//! The mod table. Four typed cells per row, no text stretch.

use iced::widget::text::Text;
use iced::widget::{button, checkbox, container, table, text, tooltip};
use iced::{Center, Color, Element, Fill};
use iced_fonts::bootstrap;

use super::{CAUTION, GOOD, TROUBLE};
use crate::message::Message;
use crate::state::{Brhap, Row, Status};

/// Glyphs read at a glance, so they sit a little above the body text size.
const GLYPH: f32 = 20.0;

/// Alignment belongs to the column, not to a wrapper inside the cell. The
/// pencil sits in a button and is therefore the tallest cell, so it sets the
/// row height and everything else has to be aligned against it.
pub(crate) fn mods(state: &Brhap) -> Element<'_, Message> {
    table(
        [
            table::column(text(""), select).width(30).align_x(Center).align_y(Center),
            table::column(text("Mod"), name).width(Fill).align_y(Center),
            table::column(text(""), over).width(40).align_x(Center).align_y(Center),
            table::column(text(""), status).width(40).align_x(Center).align_y(Center),
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

/// The icon font leaves slack under the glyph, so the text box centres while
/// the glyph inside it rides high. Hugging the line height fixes that.
fn glyph(icon: Text<'static>, color: Color) -> Element<'static, Message> {
    icon.size(GLYPH)
        .line_height(1.0)
        .color(color)
        .width(Fill)
        .align_x(Center)
        .into()
}

fn status(row: Row) -> Element<'static, Message> {
    match row.status {
        Status::Pending => text("...").size(GLYPH).line_height(1.0).into(),
        Status::Failed => glyph(bootstrap::exclamation_triangle(), TROUBLE),
        Status::Unknown => glyph(bootstrap::exclamation_triangle(), CAUTION),
        Status::Ready => glyph(bootstrap::check_circle(), GOOD),
        Status::Unmet => glyph(bootstrap::x_circle(), TROUBLE),
        Status::Quiet => text("").into(),
    }
}
