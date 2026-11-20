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
pub type UpdateState = Background<Option<qsmxt_io::release::check::UpdateInfo>>;

/// The launch-time announcements fetch.
pub type NewsState = Background<qsmxt_io::news::Loaded>;

impl UpdateState {
    pub fn start() -> Self {
        Self { receiver: qsmxt_io::release::check::spawn(), done: false }
    }
}

impl NewsState {
    /// Tests never fetch: a dev build would, and the tests must stay off the network.
    pub fn start() -> Self {
        #[cfg(not(test))]
        return Self { receiver: qsmxt_io::news::spawn(), done: false };
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
        if let Some(Some(info)) = self.update.state.poll() {
            self.update.pending = Some(UpdateModal { tag: info.tag, html_url: info.html_url });
        }
        if let Some(loaded) = self.announcements.state.poll() {
            self.receive_news(loaded);
        }
        self.show_next_pop_up();
    }

    /// Take the announcements the background fetch came back with: queue those due to pop up.
    pub fn receive_news(&mut self, loaded: qsmxt_io::news::Loaded) {
        use qsmxt_io::{news, release as update};
        self.announcements.queue = news::pop_ups(&loaded.items, update::current_version(), &news::load_seen());
        self.announcements.loaded = Some(loaded);
    }

    /// Open the next pop-up that is due, when its turn comes. One pop-up at a time, in this
    /// order: critical announcements, then the update prompt, then ordinary announcements. The
    /// update prompt waits until the announcements are in (so a critical one is not shown after
    /// it), and ordinary announcements until the release check is done. Announcements never
    /// open over another pop-up (a tip, an algorithm list, advice); the update prompt does, as
    /// it always has.
    pub fn show_next_pop_up(&mut self) {
        if self.update.modal.is_some() || self.announcements.modal.is_some() {
            return;
        }
        let other_open = self.tip_modal.is_some()
            || self.algo_modal.is_some()
            || self.rec_modal.is_some()
            || self.refs.modal.is_some();
        if self.announcements.queue.first().is_some_and(|a| a.critical) {
            if !other_open {
                let next = self.announcements.queue.remove(0);
                self.open_news(next);
            }
            return;
        }
        if self.update.pending.is_some() {
            if self.announcements.state.finished() {
                self.update.modal = self.update.pending.take();
            }
            return;
        }
        if !other_open && self.update.state.finished() && !self.announcements.queue.is_empty() {
            let next = self.announcements.queue.remove(0);
            self.open_news(next);
        }
    }
}
