//! The TUI's state ([`App`]) and its input handling, by subject: the tabs and their pages, the
//! rows and fields of the form, the Input tab's modes, key handling per tab, pop-ups and
//! background jobs.

mod export;
mod field_paths;
mod fields;
mod form;
mod input;
mod mask;
mod modals;
mod page;
mod rows;
mod tabs;
mod tasks;
#[cfg(test)]
mod tests;

pub use export::*;
pub use field_paths::*;
pub use fields::*;
pub use form::*;
pub use input::*;
pub use mask::*;
pub use modals::*;
pub use page::*;
pub use rows::*;
pub use tasks::*;
use tabs::*;

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::LazyLock;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use qsmxt_io::bids::discovery::{self, BidsTree};
use qsmxt_io::nifti;

use super::mask_builder::MaskRecipe;
use super::settings_rows::{stored_path, stored_placeholder};

/// The settings Basic mode shows, by path ([`App::basic_paths`]).
type BasicPaths = std::rc::Rc<std::collections::BTreeSet<&'static str>>;

/// The release check started at launch, and the pop-up it leads to.
#[derive(Debug)]
pub struct UpdateCheck {
    /// The "a new version is available" pop-up, once the background check has found one.
    pub modal: Option<UpdateModal>,
    /// Background release check started at launch.
    pub state: UpdateState,
    /// A newer release the check found, waiting for its turn among the pop-ups
    /// ([`App::show_next_pop_up`]).
    pub pending: Option<UpdateModal>,
}

/// The announcements fetched at launch, and their pop-up.
#[derive(Debug)]
pub struct Announcements {
    /// An announcement pop-up, or the history of them all (`n` on the tab bar).
    pub modal: Option<NewsModal>,
    /// Background fetch of the announcements started at launch.
    pub state: NewsState,
    /// What that fetch came back with, once it has.
    pub loaded: Option<qsmxt_io::news::Loaded>,
    /// Announcements still to pop up, in order (critical first, then newest first).
    pub queue: Vec<qsmxt_io::news::Announcement>,
}

pub struct App {
    pub active_tab: usize,
    /// The sub-tab last chosen on each tab (see [`App::page`]).
    pub sub_page: [Option<Page>; TAB_NAMES.len()],
    /// Focus is on the sub-tab strip rather than the rows (see [`App::on_subtabs`]).
    pub subtab_focus: bool,
    /// Whether the last frame drew the Reconstruction tab's sections as a sidebar (wide enough)
    /// rather than as a strip of pills; the keys follow what is on screen.
    pub sidebar_layout: std::cell::Cell<bool>,
    /// The width the last frame gave a section's settings, which its notes wrap to; 0 (before
    /// any frame) leaves them as written.
    pub note_width: std::cell::Cell<u16>,
    /// Focus is on the main tab bar: ←/→ switch tabs, ↓ or Enter go into the tab.
    pub tab_focus: bool,
    pub active_field: usize,
    pub editing: bool,
    pub cursor_pos: usize,
    pub form: RunForm,
    pub filter_state: FilterTreeState,
    pub pipeline_state: PipelineFormState,
    pub should_quit: bool,
    pub should_run: bool,
    /// Set when the user accepts the update prompt: the main loop tears the TUI down
    /// and hands over to `qsmxt update`.
    pub should_update: bool,
    pub form_scroll_offset: usize,
    pub methods_scroll_offset: usize,
    /// The Pipeline tab: its scroll, the scan it plans on, where its SVG goes.
    pub pipeline_view: super::pipeline_view::PipelineView,
    pub error_message: Option<String>,
    /// A one-line confirmation in the footer (e.g. "Methods copied to clipboard"); cleared,
    /// like `error_message`, by the next key.
    pub status_message: Option<String>,
    /// Text to put on the clipboard: the main loop takes it and writes it out (OSC 52).
    pub clipboard: Option<String>,
    pub input_mode: InputMode,
    pub example_state: ExampleState,
    pub dicom_state: DicomConvertState,
    pub nifti_state: NiftiState,
    /// When set, an algorithm-selection modal is open over the QSM/χ-separation tab.
    pub algo_modal: Option<AlgoModal>,
    /// The release check started at launch, and its pop-up.
    pub update: UpdateCheck,
    /// The announcements fetched at launch, and their pop-up.
    pub announcements: Announcements,
    /// The "Did you know?" pop-up: shown at launch (unless tips are off) and on `?`.
    pub tip_modal: Option<TipModal>,
    /// The recommendation pop-up, opened when a change creates a recommendation
    /// ([`super::recommend`]).
    pub rec_modal: Option<super::recommend::RecModal>,
    /// What the engine works the pop-ups after a change out from: the findings open after the
    /// last change, the ones waiting for a pop-up, the methods chosen ([`qsmxt_engine::advice::edit`]).
    pub advice: qsmxt_engine::advice::edit::State,
    /// The form at the last check (empty before the first): a key that changed nothing is not
    /// checked.
    pub rec_form: String,
    /// The form as it was at the same point: what "Undo my change" in a required-change pop-up
    /// sets back ([`super::recommend::Undo`]).
    pub rec_snapshot: Option<Box<super::recommend::FormSnapshot>>,
    /// The reference pipeline pop-up opened when a method is chosen, and what it is worked out
    /// from ([`super::recommend::RefState`]).
    pub refs: super::recommend::RefState,
    /// The Pipeline tab ([`super::pipelines_tab`]).
    pub pipelines: super::pipelines_tab::PipelinesState,
    /// Where the clickable things of the last frame are ([`super::mouse`]).
    pub hits: super::mouse::Hits,
    /// Advanced mode: every setting shows. In Basic mode a Reconstruction section shows the
    /// basic settings and any that are changed or flagged ([`qsmxt_engine::schema::regular_view`]).
    /// Advanced until the launch reads the user's saved choice ([`super::prefs`]), which is Basic
    /// unless they chose Advanced.
    pub advanced: bool,
    /// The settings Basic mode shows, for the config they were worked out from.
    basic_cache: std::cell::RefCell<Option<(String, BasicPaths)>>,
    /// Which settings apply, for the config and facts they were worked out for
    /// ([`super::settings_rows`]).
    pub(super) visible_cache: super::settings_rows::VisibleCache,
    /// The config the form builds ([`super::command::config_from_app`]), or why it builds none,
    /// as of the last [`App::prepare_frame`]: what the frame shows. Read through
    /// [`App::current_config`], which uses it only while the form cannot have changed since.
    pub config: Option<Result<qsmxt_engine::PipelineConfig, String>>,
    /// Set while a frame is prepared and drawn, when [`Self::config`] is the form's: nothing
    /// changes the form then. At any other time the form may have changed since, so the config
    /// is built afresh.
    pub in_frame: bool,
}

impl App {
    // `new` starts the launch checks: not what a default value should do.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::with_background(UpdateState::start(), NewsState::start())
    }

    /// A form with no release check or announcements, for trying a value on
    /// ([`super::recommend`]).
    pub(super) fn new_for_trial() -> Self {
        Self::with_background(UpdateState::disabled(), NewsState::disabled())
    }

    fn with_background(update_state: UpdateState, news_state: NewsState) -> Self {
        App {
            active_tab: 0,
            sub_page: [None; TAB_NAMES.len()],
            subtab_focus: false,
            sidebar_layout: std::cell::Cell::new(true),
            note_width: std::cell::Cell::new(0),
            tab_focus: false,
            active_field: 0,
            editing: false,
            cursor_pos: 0,
            form: RunForm::default(),
            filter_state: FilterTreeState::default(),
            pipeline_state: PipelineFormState::default(),
            should_quit: false,
            should_run: false,
            should_update: false,
            form_scroll_offset: 0,
            methods_scroll_offset: 0,
            pipeline_view: Default::default(),
            error_message: None,
            status_message: None,
            clipboard: None,
            algo_modal: None,
            update: UpdateCheck { modal: None, state: update_state, pending: None },
            announcements: Announcements { modal: None, state: news_state, loaded: None, queue: Vec::new() },
            tip_modal: None,
            input_mode: InputMode::Bids,
            example_state: ExampleState::default(),
            dicom_state: DicomConvertState::default(),
            nifti_state: NiftiState::default(),
            rec_modal: None,
            advice: Default::default(),
            rec_form: String::new(),
            rec_snapshot: None,
            refs: Default::default(),
            pipelines: Default::default(),
            hits: Default::default(),
            advanced: true,
            basic_cache: Default::default(),
            visible_cache: Default::default(),
            config: None,
            in_frame: false,
        }
    }

    /// Validate required fields and either set should_run or show error.
    pub fn try_run(&mut self) {
        self.error_message = None;

        // If in DICOM mode, must convert first
        if self.input_mode == InputMode::DicomToBids
            && self.dicom_state.convert_status != ConvertStatus::Done
        {
            self.error_message = Some("Convert DICOM to BIDS first (Enter on Convert button)".to_string());
            self.active_tab = 0;
            return;
        }

        // NIfTI mode: must convert first
        if self.input_mode == InputMode::NIfTI
            && self.nifti_state.convert_status != ConvertStatus::Done
        {
            self.error_message = Some("Convert NIfTI to BIDS first (Enter on Convert button)".to_string());
            self.active_tab = 0;
            return;
        }

        // Example mode: the dataset has to exist before it can be run
        if self.input_mode == InputMode::Example
            && self.example_state.status != ConvertStatus::Done
        {
            self.error_message =
                Some("Download the example dataset first (Enter on Download)".to_string());
            self.active_tab = 0;
            return;
        }

        // BIDS directory is always required
        if self.form.bids_dir.trim().is_empty() {
            self.error_message = Some("BIDS directory is required".to_string());
            self.active_tab = 0;
            self.active_field = 0;
            return;
        }

        // SLURM mode requires account
        if self.form.execution_mode == 1 && self.form.slurm_account.trim().is_empty() {
            self.error_message = Some("SLURM account is required".to_string());
            self.goto_field("slurm_account");
            return;
        }

        self.should_run = true;
    }

    /// What F5 (and the Execution tab's button) does with the current settings: the button's
    /// label and the help line shown while it has focus.
    pub fn run_action(&self) -> (&'static str, &'static str) {
        match (self.form.execution_mode, self.form.dry_run, self.form.slurm_submit) {
            (0, false, _) => ("Run", "Run the pipeline now with these settings (the same as F5)"),
            (0, true, _) => ("Dry run", "Print the processing plan without running anything (the same as F5)"),
            (_, _, true) => ("Submit SLURM jobs", "Write a SLURM job script per run and submit them with sbatch (the same as F5)"),
            (_, _, false) => ("Write SLURM scripts", "Write a SLURM job script per run, to submit yourself (the same as F5)"),
        }
    }

    /// Bring the state a frame shows up to date, before drawing it: the header's pipeline status,
    /// and on a row-based tab the pipeline form's page and context ([`Self::sync_pipeline_mode`];
    /// a page that stopped applying sends focus back to the top). Drawing ([`super::ui::draw`])
    /// then only reads the form, so the next key and the config see the state this left, whether
    /// or not a frame was drawn in between.
    pub fn prepare_frame(&mut self) {
        // Built once for the frame: the header, the command, the methods, the rows' context and
        // Basic mode's choice of rows all read it, many times a frame.
        self.config = Some(super::command::config_from_app(self).map_err(|e| e.to_string()));
        self.in_frame = true;
        // In the order drawing used to do these, so each frame shows what it did.
        self.pipeline_status();
        if Self::is_rows_tab(self.active_tab) {
            self.sync_pipeline_mode();
        }
        self.in_frame = false;
    }
}
