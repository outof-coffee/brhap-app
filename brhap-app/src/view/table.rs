//! The tables. Typed cells per row, no text stretch.

use brhap_core::SettingsRow;
use iced::widget::text::Text;
use iced::widget::{button, checkbox, container, row, table, text, text_input, tooltip};
use iced::{Center, Color, Element, Fill, Length};
use iced_aw::widget::drop_down::{Alignment, DropDown, Offset};
use iced_fonts::bootstrap;

use super::{CAUTION, GLYPH, GOOD, TROUBLE};
use crate::message::Message;
use crate::state::{Brhap, ProfileRow, Row, Status};

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

/// The summary is the longer of the two text columns, so it gets the larger
/// share of whatever width is left after the two icon columns.
pub(crate) fn profiles(state: &Brhap) -> Element<'_, Message> {
    table(
        [
            table::column(text(""), profile_load).width(40).align_x(Center).align_y(Center),
            table::column(text("Profile"), profile_name)
                .width(Length::FillPortion(1))
                .align_y(Center),
            table::column(text("Summary"), profile_summary)
                .width(Length::FillPortion(2))
                .align_y(Center),
            table::column(text(""), profile_remove).width(40).align_x(Center).align_y(Center),
        ],
        state.profile_rows(),
    )
    .padding_x(6)
    .padding_y(4)
    .into()
}

/// The core names each setting and says what it holds. The value column takes
/// the larger share, since it is where the editor opens from.
pub(crate) fn settings(state: &Brhap) -> Element<'_, Message> {
    table(
        [
            table::column(text("Setting"), setting_name)
                .width(Length::FillPortion(1))
                .align_y(Center),
            table::column(text("Value"), setting_value)
                .width(Length::FillPortion(2))
                .align_y(Center),
            table::column(text(""), |_row: SettingsRow| setting_edit(state))
                .width(40)
                .align_x(Center)
                .align_y(Center),
        ],
        state.settings_rows(),
    )
    .padding_x(6)
    .padding_y(4)
    .into()
}

/// The description is what the setting is for, which is worth a sentence but
/// not a column of its own.
///
/// This cell and the value beside it build owned widgets, but every column in
/// a table shares one lifetime, and the editor column borrows state. So both
/// take whatever lifetime that turns out to be rather than insisting on
/// `'static` the way the mod and profile cells do.
fn setting_name<'a>(row: SettingsRow) -> Element<'a, Message> {
    tooltip(
        text(row.name).size(14),
        container(text(row.description).size(12)).padding(6).style(container::rounded_box),
        tooltip::Position::Right,
    )
    .into()
}

/// A stored value is reported as present and never shown. The run of stars is
/// a fixed length, so it says nothing about the value behind it either.
fn setting_value<'a>(row: SettingsRow) -> Element<'a, Message> {
    match row.value {
        Some(_) => text("********").size(13).into(),
        None => text("not set").size(13).into(),
    }
}

/// The editor hangs off the pencil rather than taking over the screen. It is
/// end-aligned because the pencil is the last column, so an overlay wider than
/// the button has to grow inward.
///
/// The input is masked unless the reveal is on, which is the only way to check
/// a pasted key without it sitting on screen afterwards.
fn setting_edit(state: &Brhap) -> Element<'_, Message> {
    let pencil = button(bootstrap::pencil().size(GLYPH).line_height(1.0))
        .style(button::text)
        .on_press(Message::EditSteamKey);

    let reveal = if state.reveal_key { bootstrap::eye_slash() } else { bootstrap::eye() };

    let editor = container(
        row![
            text_input("paste the key", &state.key_input)
                .on_input(Message::KeyInput)
                .on_submit(Message::CommitSteamKey)
                .secure(!state.reveal_key)
                .size(13)
                .width(Fill),
            button(reveal.size(GLYPH).line_height(1.0))
                .style(button::text)
                .on_press(Message::ToggleReveal),
            super::action("save".to_string(), Some(Message::CommitSteamKey)),
        ]
        .spacing(6)
        .align_y(Center),
    )
    .padding(10)
    .style(container::rounded_box);

    DropDown::new(pencil, editor, state.editing_key)
        .width(320)
        .alignment(Alignment::BottomEnd)
        .offset(Offset::new(0.0, 4.0))
        .on_dismiss(Message::CancelKeyEdit)
        .into()
}

/// Loading a profile is what clicking its name used to do. An arrow into a box
/// says so without the name having to look like a link.
fn profile_load(row: ProfileRow) -> Element<'static, Message> {
    tooltip(
        button(bootstrap::box_arrow_in_right().size(GLYPH).line_height(1.0))
            .style(button::text)
            .on_press(Message::ApplyProfile(row.name)),
        container(text("Load this profile").size(12)).padding(6).style(container::rounded_box),
        tooltip::Position::Right,
    )
    .into()
}

fn profile_name(row: ProfileRow) -> Element<'static, Message> {
    text(row.name).size(14).into()
}

fn profile_summary(row: ProfileRow) -> Element<'static, Message> {
    text(row.summary).size(12).into()
}

fn profile_remove(row: ProfileRow) -> Element<'static, Message> {
    tooltip(
        button(bootstrap::trash().size(GLYPH).line_height(1.0).color(TROUBLE))
            .style(button::text)
            .on_press(Message::DeleteProfile(row.name)),
        container(text("Delete this profile").size(12)).padding(6).style(container::rounded_box),
        tooltip::Position::Left,
    )
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
