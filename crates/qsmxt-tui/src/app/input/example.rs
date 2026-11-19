//! The example dataset: its download and the picker's keys.

use super::*;

// ─── Example dataset state ───

/// What a background example download reports back to the UI thread.
pub enum ExampleMessage {
    Log(String),
    /// Every selected acquisition is in place; carries the dataset directory.
    Done(std::path::PathBuf),
    Error(String),
}

/// Default dataset directory when the user leaves the field blank.
pub const EXAMPLE_DEFAULT_DIR: &str = "qsmxt-example";

pub struct ExampleState {
    /// Directory the BIDS dataset is created in (or added to). Blank means
    /// [`EXAMPLE_DEFAULT_DIR`].
    pub output_dir: String,
    /// Focus within the picker: [`Self::BUTTON`] for the Download button above the list, or
    /// the row of an acquisition ([`Self::acquisition`]).
    pub cursor: usize,
    /// Which acquisitions are ticked. Parallel to `EXAMPLES`.
    pub selected: Vec<bool>,
    /// How far the acquisition list is scrolled.
    pub scroll_offset: usize,
    pub status: ConvertStatus,
    pub log: Vec<String>,
    /// `(bytes downloaded, bytes total)` for the archive in flight. Written by the
    /// worker thread at download speed and read once per frame, so it does not go
    /// through the message channel.
    progress: Arc<(AtomicU64, AtomicU64)>,
    receiver: Option<mpsc::Receiver<ExampleMessage>>,
}

impl Default for ExampleState {
    fn default() -> Self {
        let mut selected = vec![false; qsmxt_io::example::EXAMPLES.len()];
        // Tick the default acquisition and start on the Download button: the common
        // case is "just give me an example dataset", which is then a single Enter.
        // Arrowing down from the button walks into the list to pick something else.
        let default_idx = qsmxt_io::example::EXAMPLES
            .iter()
            .position(|e| e.id == qsmxt_io::example::DEFAULT_ID)
            .unwrap_or(0);
        selected[default_idx] = true;
        Self {
            output_dir: String::new(),
            cursor: Self::BUTTON,
            selected,
            scroll_offset: 0,
            status: ConvertStatus::Idle,
            log: Vec::new(),
            progress: Arc::new((AtomicU64::new(0), AtomicU64::new(0))),
            receiver: None,
        }
    }
}

impl ExampleState {
    /// The Download button's place in the focus order: first, above the list.
    pub const BUTTON: usize = 0;

    /// The last place in the focus order: the last acquisition.
    pub fn last_row() -> usize {
        qsmxt_io::example::EXAMPLES.len()
    }

    /// The acquisition the cursor is on, as an index into [`qsmxt_io::example::EXAMPLES`]; `None`
    /// on the button.
    pub fn acquisition(&self) -> Option<usize> {
        self.cursor.checked_sub(1)
    }

    /// The dataset directory, with `~` expanded and the default applied.
    pub fn resolved_dir(&self) -> std::path::PathBuf {
        let trimmed = self.output_dir.trim();
        let raw = if trimmed.is_empty() { EXAMPLE_DEFAULT_DIR } else { trimmed };
        expand_tilde(raw)
    }

    pub fn selected_examples(&self) -> Vec<&'static qsmxt_io::example::Example> {
        qsmxt_io::example::EXAMPLES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected.get(*i).copied().unwrap_or(false))
            .map(|(_, e)| e)
            .collect()
    }

    /// Total download size of the current selection, in bytes.
    pub fn selected_bytes(&self) -> u64 {
        self.selected_examples().iter().map(|e| e.bytes).sum()
    }

    /// `(downloaded, total)` for the archive currently in flight, or `None` when
    /// nothing is downloading.
    /// Set the download's progress, as the download thread does.
    #[cfg(test)]
    pub fn set_progress(&self, done: u64, total: u64) {
        self.progress.0.store(done, Ordering::Relaxed);
        self.progress.1.store(total, Ordering::Relaxed);
    }

    pub fn progress(&self) -> Option<(u64, u64)> {
        let total = self.progress.1.load(Ordering::Relaxed);
        (total > 0).then(|| (self.progress.0.load(Ordering::Relaxed), total))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(slot) = self.acquisition().and_then(|i| self.selected.get_mut(i)) {
            *slot = !*slot;
        }
    }

    /// Start fetching the ticked acquisitions in the background. Returns an error
    /// message for the status bar if the request is not actionable.
    pub fn start_download(&mut self) -> Result<(), String> {
        if self.status == ConvertStatus::Converting {
            return Err("A download is already running".to_string());
        }
        let examples = self.selected_examples();
        if examples.is_empty() {
            return Err("Select at least one acquisition (Space to tick)".to_string());
        }

        let dir = self.resolved_dir();
        self.status = ConvertStatus::Converting;
        self.log.clear();
        self.log.push(format!(
            "Fetching {} acquisition{} ({:.0} MB) into {}",
            examples.len(),
            if examples.len() == 1 { "" } else { "s" },
            self.selected_bytes() as f64 / 1e6,
            dir.display()
        ));

        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let progress = Arc::clone(&self.progress);
        progress.0.store(0, Ordering::Relaxed);
        progress.1.store(0, Ordering::Relaxed);

        std::thread::spawn(move || {
            for example in examples {
                let _ = tx.send(ExampleMessage::Log(format!("Downloading {}", example.id)));
                progress.0.store(0, Ordering::Relaxed);
                progress.1.store(example.bytes, Ordering::Relaxed);

                let zip = match qsmxt_io::example::download::ensure(example, &mut |done, total| {
                    progress.0.store(done, Ordering::Relaxed);
                    progress.1.store(total.max(1), Ordering::Relaxed);
                }) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(ExampleMessage::Error(e.to_string()));
                        return;
                    }
                };
                progress.1.store(0, Ordering::Relaxed);

                let tx_step = tx.clone();
                match qsmxt_io::example::materialize::materialize(example, &zip, &dir, false, &mut |s| {
                    let _ = tx_step.send(ExampleMessage::Log(s));
                }) {
                    Ok(qsmxt_io::example::materialize::Outcome::Written(n)) => {
                        let _ = tx.send(ExampleMessage::Log(format!(
                            "Added {} (ses-{}, acq-{}, run-{}, {n} echoes)",
                            example.id, example.scanner, example.acq, example.run
                        )));
                    }
                    Ok(qsmxt_io::example::materialize::Outcome::Skipped) => {
                        let _ = tx.send(ExampleMessage::Log(format!(
                            "{} is already in this dataset — skipped",
                            example.id
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(ExampleMessage::Error(e.to_string()));
                        return;
                    }
                }
            }
            let _ = tx.send(ExampleMessage::Done(dir));
        });
        Ok(())
    }

    /// Drain worker messages. Returns the dataset directory once every selected
    /// acquisition is in place, so the caller can switch to BIDS mode.
    pub fn poll(&mut self) -> Option<std::path::PathBuf> {
        let rx = self.receiver.as_ref()?;
        let mut done = None;
        loop {
            match rx.try_recv() {
                Ok(ExampleMessage::Log(line)) => self.log.push(line),
                Ok(ExampleMessage::Done(dir)) => {
                    self.status = ConvertStatus::Done;
                    self.log.push("Dataset ready".to_string());
                    done = Some(dir);
                    break;
                }
                Ok(ExampleMessage::Error(e)) => {
                    self.status = ConvertStatus::Error;
                    self.log.push(format!("Error: {e}"));
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // The worker vanished without reporting; do not spin on a dead channel.
                    if self.status == ConvertStatus::Converting {
                        self.status = ConvertStatus::Error;
                        self.log.push("Error: download thread stopped unexpectedly".to_string());
                    }
                    break;
                }
            }
        }
        self.receiver = None;
        self.progress.1.store(0, Ordering::Relaxed);
        done
    }
}

/// Expand a leading `~/` against `$HOME`, leaving anything else untouched.
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::Path::new(&home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

impl App {
    /// Keys for the example-dataset picker (Input tab, past the IO fields).
    pub(super) fn handle_example_tab_key(&mut self, key: KeyEvent) {
        let on_button = self.example_state.cursor == ExampleState::BUTTON;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}
            KeyCode::Up | KeyCode::Char('k') => {
                if on_button {
                    // Back up into the IO fields above the picker.
                    self.active_field = Self::INPUT_IO_FIELDS - 1;
                } else {
                    self.example_state.cursor -= 1;
                }
            }
            // Once a download has started the list is hidden: the button is all there is.
            KeyCode::Down | KeyCode::Char('j') if self.example_state.cursor < ExampleState::last_row()
                && self.example_state.status == ConvertStatus::Idle => {
                self.example_state.cursor += 1;
            }
            KeyCode::Char(' ') if !on_button => {
                self.example_state.toggle_selected();
            }
            KeyCode::Enter => {
                if !on_button {
                    self.example_state.toggle_selected();
                } else if let Err(msg) = self.example_state.start_download() {
                    self.error_message = Some(msg);
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Resetting mid-fetch would orphan the worker thread while it is still
                // writing into the dataset directory, so leave it alone until it lands.
                if self.example_state.status == ConvertStatus::Converting {
                    self.error_message =
                        Some("A download is running — wait for it to finish".to_string());
                } else {
                    self.example_state = ExampleState::default();
                    self.active_field = Self::INPUT_IO_FIELDS;
                }
            }
            KeyCode::F(5) => self.try_run(),
            _ => {}
        }
    }
}
