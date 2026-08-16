//! brhap (Bohemian Rhapsody), the native application.
//!
//! One self-contained binary. No webview, no bundled frontend, no HTTP. The
//! behaviour lives in ../brhap-core, the same crate the Tauri wrapper uses, so
//! the two frontends cannot drift apart on what an operation means.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod events;
mod message;
mod state;
mod update;
mod view;
mod work;

use iced::{Subscription, Task};

use message::Message;
use state::Brhap;

const WINDOW: (f32, f32) = (1280.0, 720.0);

fn boot() -> (Brhap, Task<Message>) {
    let state = Brhap::new();
    let load = state.reload();
    (state, load)
}

fn subscription(_state: &Brhap) -> Subscription<Message> {
    events::subscription().map(Message::Session)
}

fn main() -> iced::Result {
    iced::application(boot, update::update, view::view)
        .subscription(subscription)
        // Without this every status glyph renders as tofu.
        .font(iced_fonts::BOOTSTRAP_FONT_BYTES)
        .title("brhap")
        .window_size(WINDOW)
        .run()
}
