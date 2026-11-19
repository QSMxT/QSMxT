//! Launch-time background jobs (the release check, the announcements) and the pop-ups they queue.

use super::*;

/// Holds the receiver for a launch-time background job (the release check, the announcements).
///
/// The job runs on its own thread and the result is picked up by [`Self::poll`] on a
/// later frame, so nothing about startup waits on the network. When the job is disabled
/// (opt-out, CI, or for the release check a dev build) the receiver is `None` and `poll`
/// never yields anything. Nothing joins the thread, so it never delays exit either.
#[derive(Debug)]
pub struct Background<T> {
    pub(super) receiver: Option<std::sync::mpsc::Receiver<T>>,
    /// Set once the result has been taken (or there will be none).
    pub(super) done: bool,
}

/// The launch-time release check: `Some` newer release, or `None`.
pub type UpdateState = Background<Option<crate::commands::update::check::UpdateInfo>>;

/// The launch-time announcements fetch.
pub type NewsState = Background<crate::commands::news::Loaded>;

impl UpdateState {
    pub fn start() -> Self {
        Self { receiver: crate::commands::update::check::spawn(), done: false }
    }
}

impl NewsState {
    /// Tests never fetch: a dev build would, and the tests must stay off the network.
    pub fn start() -> Self {
        #[cfg(not(test))]
        return Self { receiver: crate::commands::news::spawn(), done: false };
        #[cfg(test)]
        Self::disabled()
    }
}

impl<T> Background<T> {
    /// A job that never produces a result.
    pub fn disabled() -> Self {
        Self { receiver: None, done: true }
    }

    /// A job that has already produced `value`, for tests.
    #[cfg(test)]
    pub fn ready(value: T) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(value).unwrap();
        Self { receiver: Some(rx), done: false }
    }

    /// Has the result been taken, or is there none to come?
    pub fn finished(&self) -> bool {
        self.done
    }

    /// Non-blocking: returns the result exactly once, the first frame after the
    /// background job finishes. A failed or still-running job yields `None`.
    pub fn poll(&mut self) -> Option<T> {
        if self.done {
            return None;
        }
        let Some(rx) = self.receiver.as_ref() else {
            self.done = true;
            return None;
        };
        match rx.try_recv() {
            Ok(found) => {
                self.done = true;
                self.receiver = None;
                Some(found)
            }
            // Still working; try again next frame.
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            // Thread died without sending; nothing to show.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.done = true;
                self.receiver = None;
                None
            }
        }
    }
}

impl App {
    // ─── Pipeline tab key handling ───

    /// Keys while the algorithm-selection modal is open: ↑↓/jk move, Enter/Space confirm, Esc/q cancel.
    /// Pick up the launch-time release check and announcements, and open the next pop-up
    /// that is due ([`Self::show_next_pop_up`]). Called once per frame from the main loop.
    pub fn poll_launch_checks(&mut self) {
        if let Some(Some(info)) = self.update_state.poll() {
            self.pending_update = Some(UpdateModal { tag: info.tag, html_url: info.html_url });
        }
        if let Some(loaded) = self.news_state.poll() {
            self.receive_news(loaded);
        }
        self.show_next_pop_up();
    }

    /// Take the announcements the background fetch came back with: queue those due to pop up.
    pub fn receive_news(&mut self, loaded: crate::commands::news::Loaded) {
        use crate::commands::{news, update};
        self.news_queue = news::pop_ups(&loaded.items, update::current_version(), &news::load_seen());
        self.news = Some(loaded);
    }

    /// Open the next pop-up that is due, when its turn comes. One pop-up at a time, in this
    /// order: critical announcements, then the update prompt, then ordinary announcements. The
    /// update prompt waits until the announcements are in (so a critical one is not shown after
    /// it), and ordinary announcements until the release check is done. Announcements never
    /// open over another pop-up (a tip, an algorithm list, advice); the update prompt does, as
    /// it always has.
    pub fn show_next_pop_up(&mut self) {
        if self.update_modal.is_some() || self.news_modal.is_some() {
            return;
        }
        let other_open = self.tip_modal.is_some()
            || self.algo_modal.is_some()
            || self.rec_modal.is_some()
            || self.refs.modal.is_some();
        if self.news_queue.first().is_some_and(|a| a.critical) {
            if !other_open {
                let next = self.news_queue.remove(0);
                self.open_news(next);
            }
            return;
        }
        if self.pending_update.is_some() {
            if self.news_state.finished() {
                self.update_modal = self.pending_update.take();
            }
            return;
        }
        if !other_open && self.update_state.finished() && !self.news_queue.is_empty() {
            let next = self.news_queue.remove(0);
            self.open_news(next);
        }
    }
}
