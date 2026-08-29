//! Getting blocking core work off the UI thread.
//!
//! brhap-core is synchronous by design and there is no async runtime anywhere
//! in this stack. iced's `Task::perform` wants a future, so the bridge is a
//! plain thread and a oneshot: the thread does the real work, the future
//! resolves when it lands. `iced` re-exports `futures`, so this costs no extra
//! dependency.

use std::future::Future;

use iced::futures::channel::oneshot;

/// Run blocking core work on a worker thread and await the answer.
pub fn blocking<T, F>(work: F) -> impl Future<Output = T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        // The receiver is dropped if the task was cancelled, which is not worth
        // failing over. The work has already run either way.
        let _ = sender.send(work());
    });
    async move { receiver.await.expect("core worker thread panicked") }
}
