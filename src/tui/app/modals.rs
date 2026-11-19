//! Pop-ups: tips, update, announcements and the option picker.

use super::*;

/// The "Did you know?" pop-up, with two buttons: OK, and one that turns startup tips off
/// (or back on, when it was opened with `?` while they are off).
#[derive(Debug, Clone)]
pub struct TipModal {
    pub tip: crate::tui::tips::Tip,
    /// Whether startup tips were on when the pop-up opened; picks the second button.
    pub tips_enabled: bool,
    /// The selected button: 0 is OK, 1 turns startup tips off (or on).
    pub cursor: usize,
}

impl TipModal {
    pub fn new(tip: crate::tui::tips::Tip, tips_enabled: bool) -> Self {
        TipModal { tip, tips_enabled, cursor: 0 }
    }

    /// The labels of the two buttons, OK first.
    pub fn buttons(&self) -> [&'static str; 2] {
        ["OK", if self.tips_enabled { "Don't show tips" } else { "Show tips at startup" }]
    }
}

/// The "new version available" pop-up shown once the background check finds a release
/// newer than the running binary.
#[derive(Debug, Clone)]
pub struct UpdateModal {
    /// Tag of the newer release, e.g. `v9.23.0`.
    pub tag: String,
    /// Release page, shown so the user can read the notes before updating.
    pub html_url: String,
}

/// What the user chose at the update prompt. Returned by [`App::handle_update_modal_key`]
/// so the decision can be asserted in tests without reaching for the side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChoice {
    /// Key was not one of the prompt's bindings; the prompt stays open.
    Ignored,
    /// Update now: tear down the TUI and run the installer.
    Update,
    /// Not now — ask again next launch.
    Later,
}

/// An announcement pop-up ([`crate::commands::news`]), or the history of all of them.
#[derive(Debug, Clone)]
pub struct NewsModal {
    /// The announcement popping up, or every announcement (newest first) in the history.
    pub items: Vec<qsmxt_io::news::Announcement>,
    /// The history (opened with `n`), rather than a pop-up.
    pub history: bool,
    /// A line above the history: that the announcements could not be read and these are the
    /// saved copy, or that they are still being fetched.
    pub note: Option<String>,
    pub scroll: u16,
}

/// A pop-up list for choosing one option of a pipeline `AlgoSelect` (all options at once).
#[derive(Debug, Clone)]
pub struct AlgoModal {
    /// What this modal edits, applied on confirm.
    pub target: AlgoModalTarget,
    /// Modal title (the row label, e.g. "QSM inversion").
    pub title: String,
    pub options: Vec<String>,
    /// Per-option help text (same length as `options`).
    pub help: Vec<String>,
    /// Currently highlighted option index.
    pub cursor: usize,
    /// The first option shown: the list scrolls only when the cursor would leave the window.
    pub offset: std::cell::Cell<usize>,
}

/// The selection an [`AlgoModal`] applies when confirmed. Lets a single pop-up list serve every
/// single-choice selector in the TUI regardless of how that selector stores its value.
#[derive(Debug, Clone)]
pub enum AlgoModalTarget {
    /// A named select (an `AlgoSelect` row of any row-based tab, e.g. "qsm_algorithm").
    Select(String),
    /// The masking input source of a mask section.
    MaskInput(usize),
    /// The generator algorithm (threshold vs BET) of a mask section.
    MaskGenerator(usize),
    /// The threshold method (otsu / fixed / percentile) of a mask section's generator.
    MaskThresholdMethod(usize),
    /// The Input-tab mode selector (BIDS / NIfTI / DICOM).
    InputMode,
    /// How the sections of the mask a section belongs to combine (OR / AND).
    MaskCombine(usize),
    /// HD-BET's inference (whole volume / sliding window / low memory) of a section's generator.
    MaskHdBetInference(usize),
    /// HD-BET's patch step of a section's generator ([`PipelineFormState::hd_bet_step_choices`]).
    MaskHdBetStep(usize),
    /// BET's voxel scaling of a section's generator ([`PipelineFormState::bet_voxel_scale_choices`]).
    MaskBetVoxelScale(usize),
    /// The method of a refinement step that is a threshold.
    MaskRefinementThresholdMethod { section: usize, index: usize },
    /// The type of a refinement step to add to a section ([`MaskRecipe::available_op_types`]).
    MaskAddStep(usize),
    /// The type of the focused DICOM series (Input tab, DICOM mode), in [`dicom::SeriesType::ALL`]'s order.
    DicomSeriesType,
}

/// What each threshold method does, for the threshold-method pop-up.
fn threshold_method_help() -> Vec<String> {
    ["Otsu automatic threshold", "Fixed intensity threshold", "Percentile-based threshold",
     "STI Suite QSM_GUI's starting level: Otsu's threshold of the middle slice"].map(String::from).to_vec()
}

impl App {
    pub(super) fn open_news(&mut self, item: qsmxt_io::news::Announcement) {
        self.news_modal = Some(NewsModal { items: vec![item], history: false, note: None, scroll: 0 });
    }

    /// Open the history of announcements, newest first: what the launch fetch got, or the copy
    /// saved last time while it is still running.
    pub fn open_news_history(&mut self) {
        let (items, note) = match &self.news {
            Some(loaded) => (
                loaded.items.clone(),
                loaded.error.as_ref().map(|_| {
                    "The announcements could not be read, so these are the ones saved the last time they could.".to_string()
                }),
            ),
            None if self.news_state.finished() => (qsmxt_io::news::load_cached(), None),
            None => (
                qsmxt_io::news::load_cached(),
                Some("Checking for new announcements; these are the ones saved last time.".to_string()),
            ),
        };
        self.news_modal = Some(NewsModal { items, history: true, note, scroll: 0 });
    }

    /// Keys while an announcement pop-up or the history is open: ↑↓/PgUp/PgDn/Home scroll, `c`
    /// copies a pop-up's link, Enter/Esc/q close it. Closing a pop-up records it as shown.
    pub fn handle_news_modal_key(&mut self, key: KeyEvent) {
        let Some(m) = self.news_modal.as_mut() else { return };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => m.scroll = m.scroll.saturating_add(1),
            KeyCode::PageUp => m.scroll = m.scroll.saturating_sub(10),
            KeyCode::PageDown => m.scroll = m.scroll.saturating_add(10),
            KeyCode::Home => m.scroll = 0,
            KeyCode::Char('c') | KeyCode::Char('y') if !m.history => {
                if let Some(link) = m.items.first().and_then(|a| a.link.clone()) {
                    self.clipboard = Some(link);
                    self.status_message = Some("Link copied to clipboard".to_string());
                }
            }
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                let m = self.news_modal.take().expect("checked above");
                if !m.history {
                    for a in &m.items {
                        qsmxt_io::news::mark_seen(&a.id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle a key while the update prompt is open. Returns what the user chose.
    pub fn handle_update_modal_key(&mut self, key: KeyEvent) -> UpdateChoice {
        match key.code {
            KeyCode::Enter | KeyCode::Char('u') | KeyCode::Char('U') => {
                self.update_modal = None;
                self.should_update = true;
                UpdateChoice::Update
            }
            KeyCode::Esc | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.update_modal = None;
                UpdateChoice::Later
            }
            _ => UpdateChoice::Ignored,
        }
    }

    /// Open the tip pop-up on the next tip, whether or not startup tips are on.
    pub fn open_tip(&mut self) {
        self.status_message = None;
        self.error_message = None;
        self.tip_modal = Some(TipModal::new(crate::tui::tips::next_tip(), crate::tui::tips::enabled()));
    }

    /// Keys while the tip pop-up is open: ←/→/Tab choose a button, Enter/Space press it,
    /// `n` shows the next tip, Esc/q close it.
    pub fn handle_tip_modal_key(&mut self, key: KeyEvent) {
        let Some(m) = self.tip_modal.as_mut() else { return };
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => m.cursor = 0,
            KeyCode::Right | KeyCode::Char('l') => m.cursor = 1,
            KeyCode::Tab | KeyCode::BackTab => m.cursor = 1 - m.cursor,
            KeyCode::Char('n') => m.tip = crate::tui::tips::next_tip(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let m = self.tip_modal.take().expect("checked above");
                if m.cursor == 1 {
                    crate::tui::tips::set_enabled(!m.tips_enabled);
                    self.status_message = Some(if m.tips_enabled {
                        "Tips turned off. Press ? to see a tip or to turn them back on".to_string()
                    } else {
                        "Tips will show when QSMxT opens".to_string()
                    });
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.tip_modal = None,
            _ => {}
        }
    }

    pub(super) fn handle_algo_modal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(m) = self.algo_modal.as_mut() {
                    m.cursor = m.cursor.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(m) = self.algo_modal.as_mut() {
                    if m.cursor + 1 < m.options.len() {
                        m.cursor += 1;
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(m) = self.algo_modal.take() {
                    self.apply_algo_modal(m);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.algo_modal = None,
            _ => {}
        }
    }

    /// Apply a confirmed [`AlgoModal`] selection to whatever it targets.
    fn apply_algo_modal(&mut self, m: AlgoModal) {
        match m.target {
            AlgoModalTarget::Select(field) => {
                self.set_field_select(&field, m.cursor);
                // Keep focus on the selector row (its value changed; rows below may differ).
                self.restore_focus(&Some(field));
            }
            AlgoModalTarget::MaskInput(section) => {
                self.set_mask_input(section, m.cursor);
            }
            AlgoModalTarget::MaskGenerator(section) => {
                self.set_mask_generator(section, m.cursor);
            }
            AlgoModalTarget::MaskThresholdMethod(section) => {
                self.set_mask_threshold_method(section, m.cursor);
            }
            AlgoModalTarget::InputMode => self.set_input_mode(m.cursor),
            AlgoModalTarget::MaskCombine(section) => {
                use crate::pipeline::config::MaskCombine;
                let mode = if m.cursor == 0 { MaskCombine::Or } else { MaskCombine::And };
                self.set_combine(section, mode);
            }
            AlgoModalTarget::MaskHdBetInference(section) => self.set_hd_bet_inference(section, m.cursor),
            AlgoModalTarget::MaskHdBetStep(section) => {
                let (values, _) = self.mask().hd_bet_step_choices(section);
                if let Some(&v) = values.get(m.cursor) { self.set_hd_bet_tile_step(section, v); }
            }
            AlgoModalTarget::MaskBetVoxelScale(section) => {
                let (values, _) = self.mask().bet_voxel_scale_choices(section);
                if let Some(&v) = values.get(m.cursor) { self.set_bet_voxel_scale(section, v); }
            }
            AlgoModalTarget::MaskRefinementThresholdMethod { section, index } => {
                self.set_refinement_threshold_method(section, index, m.cursor);
            }
            AlgoModalTarget::MaskAddStep(section) => {
                if let Some(kind) = MaskRecipe::available_op_types().get(m.cursor) {
                    self.add_mask_step(section, kind);
                }
            }
            AlgoModalTarget::DicomSeriesType => {
                if let Some(&t) = dicom::SeriesType::ALL.get(m.cursor) { self.set_focused_unique_series_type(t); }
            }
        }
    }

    /// The pop-up a row's choice is made in, when the row is a choice: every option listed, the
    /// current one highlighted. Enter (or Space) opens it; nothing is chosen until it is
    /// confirmed, and Esc leaves the setting as it was.
    pub fn selector_modal(&self, row: &PipelineRow) -> Option<AlgoModal> {
        use crate::pipeline::config::{MaskCombine, MaskOp};
        let ps = self.mask();
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let modal = |target, title: &str, options: Vec<String>, help: Vec<String>, cursor| Some(AlgoModal {
            target, title: title.to_string(), options, help, cursor,
            offset: Default::default(),
        });
        let generator = |section: usize| ps.mask_section(section).map(|s| &s.generator);
        match *row {
            PipelineRow::AlgoSelect { label, field, options, help } => modal(
                AlgoModalTarget::Select(field.to_string()), label.trim(), strings(options), strings(help), self.field_select(field)),
            PipelineRow::MaskOpInput { section } => modal(
                AlgoModalTarget::MaskInput(section), "Masking input",
                MaskRecipe::MASK_INPUT_SOURCES.iter().map(|s| format!("{}", s)).collect(),
                mask_schema().inputs.iter().map(|i| format!("{}: {}", i.label, i.help)).collect(),
                ps.mask_input_index(section)),
            PipelineRow::MaskOpGenerator { section } => modal(
                AlgoModalTarget::MaskGenerator(section), "Mask algorithm",
                strings(MASK_GENERATOR_TYPES),
                // A threshold's method is chosen on its own row.
                MASK_GENERATOR_TYPES.iter().map(|t| match *t {
                    "threshold" => "Keep the voxels of the section's image above a threshold: Otsu's, a value, a percentile, or STI Suite's".to_string(),
                    t => mask_step(t).help.to_string(),
                }).collect(),
                ps.mask_generator_index(section)),
            PipelineRow::MaskOpGeneratorParam { section } => match generator(section)? {
                MaskOp::Threshold { .. } => modal(
                    AlgoModalTarget::MaskThresholdMethod(section), "Threshold method",
                    strings(&["otsu", "fixed", "percentile", "sti-gui"]), threshold_method_help(),
                    ps.mask_threshold_method_index(section)),
                MaskOp::HdBet { .. } => {
                    let choices = &mask_schema().hd_bet_inference;
                    modal(AlgoModalTarget::MaskHdBetInference(section), "HD-BET inference",
                        choices.iter().map(|c| c.name.to_lowercase()).collect(),
                        choices.iter().map(|c| c.help.to_string()).collect(),
                        ps.hd_bet_inference_index(section))
                }
                _ => None,
            },
            PipelineRow::MaskOpHdBetStep { section } => {
                let (values, at) = ps.hd_bet_step_choices(section);
                let help = mask_param("hd-bet", "tile_step").help.to_string();
                modal(AlgoModalTarget::MaskHdBetStep(section), "HD-BET patch step",
                    values.iter().map(|v| format!("{v:.3}")).collect(), vec![help; values.len()], at)
            }
            PipelineRow::MaskOpBetVoxelScale { section } => {
                let (values, at) = ps.bet_voxel_scale_choices(section);
                let help = mask_param("bet", "voxel_scale").help.to_string();
                modal(AlgoModalTarget::MaskBetVoxelScale(section), "BET voxel scale",
                    values.iter().map(|v| format!("{v}x")).collect(), vec![help; values.len()], at)
            }
            PipelineRow::MaskCombineOp { section } => modal(
                AlgoModalTarget::MaskCombine(section), "How the sections combine",
                strings(&["OR", "AND"]),
                strings(&["Union: a voxel is in the mask when any section has it",
                          "Intersection: a voxel is in the mask only when every section has it"]),
                usize::from(ps.combine_mode(section) == MaskCombine::And)),
            PipelineRow::MaskOpEntry { section, index } => match ps.mask_refinements(section)?.get(index)? {
                MaskOp::Threshold { .. } => {
                    let cursor = match ps.mask_refinements(section)?.get(index)? {
                        MaskOp::Threshold { method, .. } => MaskRecipe::MASK_THRESHOLD_METHODS.iter().position(|m| m == method).unwrap_or(0),
                        _ => 0,
                    };
                    modal(AlgoModalTarget::MaskRefinementThresholdMethod { section, index }, "Threshold method",
                        strings(&["otsu", "fixed", "percentile", "sti-gui"]), threshold_method_help(), cursor)
                }
                _ => None,
            },
            PipelineRow::MaskOpAddStep { section } => {
                let available = MaskRecipe::available_op_types();
                modal(AlgoModalTarget::MaskAddStep(section), "Step to add",
                    available.iter().map(|t| t.to_string()).collect(),
                    available.iter().map(|t| mask_schema().steps.iter().find(|s| s.label == *t).map_or("", |s| s.help).to_string()).collect(), 0)
            }
            _ => None,
        }
    }
}
