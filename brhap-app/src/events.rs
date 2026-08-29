//! Session events, from the watcher thread into the iced runtime.
//!
//! brhap-core reports a running game through a listener callback, which fires
//! on the session's own watcher thread. iced wants a `Subscription`, and
//! `Subscription::run` takes a function pointer rather than a closure, so the
//! receiver cannot simply be captured.
//!
//! The way through is the standard iced worker pattern: the subscription builds
//! the channel inside the stream and hands the sending half back as its first
//! message. Since the listener has to exist before that arrives, it holds an
//! `Outbox` slot that gets filled on `Incoming::Ready`.

use std::sync::{Arc, Mutex};

use brhap_core::{Event, Listener};
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::{Subscription, stream};

/// How many events may queue before the watcher thread starts dropping them.
/// A launch produces a handful, so this is slack rather than a real bound.
const BUFFER: usize = 100;

#[derive(Debug, Clone)]
pub enum Incoming {
    /// The subscription is live. Carries the sender the listener should use.
    Ready(mpsc::Sender<Event>),
    Emitted(Event),
}

/// The listener's route into the iced runtime.
///
/// Cloneable and cheap. Empty until the subscription starts, which is fine:
/// nothing can launch a game before the UI is up, so there is nothing to lose.
#[derive(Clone, Default)]
pub struct Outbox(Arc<Mutex<Option<mpsc::Sender<Event>>>>);

impl Outbox {
    /// A listener for `Core::new`. Called from the watcher thread, so it sends
    /// without blocking and gives up rather than stalling that thread.
    pub fn listener(&self) -> Listener {
        let slot = Arc::clone(&self.0);
        Arc::new(move |event: Event| {
            let mut held = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(sender) = held.as_mut() {
                let _ = sender.try_send(event);
            }
        })
    }

    fn fill(&self, sender: mpsc::Sender<Event>) {
        *self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sender);
    }

    /// Take the sender the subscription just produced.
    pub fn accept(&self, incoming: &Incoming) {
        if let Incoming::Ready(sender) = incoming {
            self.fill(sender.clone());
        }
    }
}

fn worker() -> impl Stream<Item = Incoming> {
    stream::channel(BUFFER, async |mut output| {
        let (sender, mut receiver) = mpsc::channel(BUFFER);
        let _ = output.send(Incoming::Ready(sender)).await;

        while let Some(event) = receiver.next().await {
            let _ = output.send(Incoming::Emitted(event)).await;
        }
    })
}

pub fn subscription() -> Subscription<Incoming> {
    Subscription::run(worker)
}
