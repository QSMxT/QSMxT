//! The rows of the row-based tabs: what each page shows, Basic mode, and focus.

use super::*;

// ─── Pipeline tab state ───

/// A visible row on a row-based tab.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineRow {
    /// Algorithm selector: ◀ value ▶
    AlgoSelect {
        label: &'static str,
        field: &'static str,
        options: &'static [&'static str],
        help: &'static [&'static str], // help text per option
    },
    /// Text parameter input
    Param {
        label: &'static str,
        field: &'static str,
        help: &'static str,
    },
    /// Checkbox toggle
    Toggle {
        label: &'static str,
        field: &'static str,
        help: &'static str,
    },
    /// Section separator (blank line, not focusable)
    Separator,
    /// Informational hint line (styled, not focusable)
    Note { text: std::borrow::Cow<'static, str> },
    /// Section header "── Mask N ──" (not focusable)
    MaskSectionHeader { section: usize },
    /// Header for the steps that run on the combined mask (not focusable)
    MaskCombinedHeader,
    /// Header for the two-pass reliable mask (not focusable)
    MaskTwoPassHeader,
    /// "COMBINED WITH ◀ OR ▶" before `section`, the second or later section of the main or the
    /// reliable mask (focusable, ←/→ toggles that mask's OR/AND; `d` removes `section`)
    MaskCombineOp { section: usize },
    /// Input source for a mask section (focusable, ←/→ to cycle)
    MaskOpInput { section: usize },
    /// Generator algorithm selector (threshold or BET, ←/→ to switch)
    MaskOpGenerator { section: usize },
    /// Generator parameter (threshold method or BET fractional intensity)
    MaskOpGeneratorParam { section: usize },
    /// Threshold value (only shown for fixed/percentile threshold methods)
    MaskOpThresholdValue { section: usize },
    /// HD-BET sliding-window step (only shown when the generator is HD-BET)
    MaskOpHdBetStep { section: usize },
    /// BET voxel-size scaling (only shown when the generator is BET)
    MaskOpBetVoxelScale { section: usize },
    /// A refinement step (editable, deletable, reorderable)
    MaskOpEntry { section: usize, index: usize },
    /// One extra parameter of a multi-parameter refinement step (signal-erode), indexed into
    /// [`signal_erode_params`] (focusable, ←/→ to adjust)
    MaskOpSignalErodeParam { section: usize, index: usize, param: usize },
    /// "Add step..." row for appending new ops to a section
    MaskOpAddStep { section: usize },
    /// "Add mask..." row for adding a new OR'd section — to the main mask, or to the two-pass
    /// reliable mask when `two_pass` is set
    MaskOpAddSection { two_pass: bool },
    /// The Execution tab's button: Enter does what F5 does ([`App::try_run`]).
    RunButton,
    /// A line that reads as a value (not focusable): the mask recipe, collapsed in Basic mode.
    Summary { text: String },
    /// The last row of a Reconstruction section in Basic mode: how many settings it hides.
    /// Enter (or a click) shows every setting, as `A` does.
    AdvancedFooter { hidden: usize },
}
/// What a QSM section says while QSM is off.
pub(super) const QSM_OFF_NOTE: &str = "QSM is off: turn it on in Compute QSM, the first QSM section.";

impl App {
    /// QSM → Compute QSM: the QSM group's switch, with what turning it off does, or why QSM is
    /// reconstructed anyway.
    fn compute_qsm_rows(&self, ctx: &RowContext, rows: &mut Vec<PipelineRow>) {
        rows.push(PipelineRow::Toggle {
            label: "Compute QSM", field: "pipeline.do_qsm",
            help: "Enable QSM reconstruction (disable to only run the other outputs)",
        });
        if !self.form.settings.toggle("pipeline.do_qsm") {
            match ctx.qsm_forced_by {
                Some(why) => rows.push(PipelineRow::Note { text: format!("  Required: {why}. Left off, the run turns it on").into() }),
                None => {
                    rows.push(PipelineRow::Note { text: "  Off: no field map, background removal, inversion or reference.".into() });
                    rows.push(PipelineRow::Note { text: "  χ-separation and SMWI need QSM, or a supplied one.".into() });
                }
            }
        }
    }

    /// The notes a page of a stage that does not run shows instead of its settings: a QSM section
    /// says where QSM is turned on while it is off, and why when the settings skip its stage (the
    /// sidebar lists every section whatever the settings). `None` while the stage runs.
    pub fn page_notes(&self, page: Page, ctx: &RowContext) -> Option<Vec<PipelineRow>> {
        let notes = |lines: &[&'static str]| Some(lines.iter().map(|&l| PipelineRow::Note { text: l.into() }).collect());
        match page {
            Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation | Page::Reference
                if !self.qsm_runs(ctx) => notes(&[QSM_OFF_NOTE]),
            Page::FieldMap if !self.field_mapping_runs_for(ctx.num_echoes).0 => notes(&[
                "No field mapping with these settings: the inversion takes the wrapped phase",
                "itself (iQSM, iQSM+, single-echo TGV), or iQFM makes the field from it.",
            ]),
            Page::Background if !self.inversion_needs_bg_removal() => notes(&[
                "No background removal with these settings: the inversion takes the total field",
                "or the wrapped phase itself (see Inversion).",
            ]),
            Page::MultiOrientation if !ctx.orientation_group => notes(&[
                "No orientation groups: runs of one head at different angles, reconstructed together.",
                "Choose them on 1 Input → Orientations, and these settings apply.",
            ]),
            _ => None,
        }
    }

    /// Whether the selected inversion reads a background-removed local field (QSM.rs
    /// `InversionInput::LocalField`), and so whether background removal is part of the run.
    pub fn inversion_needs_bg_removal(&self) -> bool {
        !qsmxt_engine::bridge::skips_bgremove(&self.stage_config())
    }

    /// The settings that decide which reconstruction stages run — the inversion (and MEDI's SMV
    /// mode) and the background removal — on an otherwise default config.
    fn stage_config(&self) -> qsmxt_engine::PipelineConfig {
        let st = &self.form.settings;
        let mut config = qsmxt_engine::PipelineConfig::default();
        if let Ok(a) = serde_json::from_value(st.get("inversion.algorithm")) {
            config.inversion.algorithm = a;
        }
        config.inversion.medi.smv = st.toggle("inversion.medi.smv");
        if let Ok(b) = serde_json::from_value(st.get("bg_removal.algorithm")) {
            config.bg_removal.algorithm = b;
        }
        config
    }

    /// Whether field mapping runs in front of the inversion, for a run limited to `num_echoes`
    /// echoes (`None`: unknown): `(for some runs, for every run)`.
    ///
    /// The runner's own rule ([`qsmxt_io::pipeline::runner::field_mapping_runs`]), asked for the
    /// echo count the run is limited to, or for one and for several echoes when that is unknown —
    /// TGV maps the field of multi-echo data but reconstructs a single echo from its wrapped phase.
    pub fn field_mapping_runs_for(&self, num_echoes: Option<usize>) -> (bool, bool) {
        let config = self.stage_config();
        let counts = match num_echoes {
            Some(n) => vec![n],
            None => vec![1, 2],
        };
        let runs: Vec<bool> = counts.iter()
            .map(|&n| qsmxt_engine::bridge::field_mapping_runs(&config, n)).collect();
        (runs.iter().any(|&r| r), runs.iter().all(|&r| r))
    }

    /// Whether QSM is reconstructed: Compute QSM is on, or another output forces it.
    pub fn qsm_runs(&self, ctx: &RowContext) -> bool {
        self.form.settings.toggle("pipeline.do_qsm") || ctx.qsm_forced_by.is_some()
    }
}

#[cfg(any(test, feature = "testutils"))]
impl App {
    /// The chosen inversion, as its position in [`QSM_ALGO_OPTIONS`].
    pub fn qsm_algorithm(&self) -> usize {
        let algorithm = self.form.settings.get("inversion.algorithm");
        QSM_ALGO_OPTIONS.iter().position(|a| algorithm == *a).expect("an inversion of QSM_ALGO_OPTIONS")
    }

    /// Choose the inversion at position `i` in [`QSM_ALGO_OPTIONS`], leaving HEIDI as it is.
    pub fn set_qsm_algorithm(&mut self, i: usize) {
        self.form.settings.set("inversion.algorithm", QSM_ALGO_OPTIONS[i]);
    }
}

/// Indices of the rows focus can land on: every row but separators, notes and mask headers.
pub fn focusable_indices(rows: &[PipelineRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, PipelineRow::Separator | PipelineRow::Note { .. }
            | PipelineRow::MaskSectionHeader { .. } | PipelineRow::MaskCombinedHeader
            | PipelineRow::MaskTwoPassHeader | PipelineRow::Summary { .. }))
        .map(|(i, _)| i)
        .collect()
}

/// Notes wrapped to `width` columns (0 leaves them as written). A note continued on the next
/// row (one that does not end a sentence or a heading) is joined to it first, so a note written
/// over two rows reflows as one; each wrapped row keeps the note's indent.
pub fn wrap_notes(rows: Vec<PipelineRow>, width: usize) -> Vec<PipelineRow> {
    if width == 0 {
        return rows;
    }
    let mut joined: Vec<PipelineRow> = Vec::with_capacity(rows.len());
    for row in rows {
        if let (PipelineRow::Note { text }, Some(PipelineRow::Note { text: prev })) = (&row, joined.last_mut()) {
            if !prev.trim_end().ends_with(['.', ':', ')']) {
                *prev = format!("{} {}", prev.trim_end(), text.trim_start()).into();
                continue;
            }
        }
        joined.push(row);
    }
    let mut out = Vec::with_capacity(joined.len());
    for row in joined {
        let PipelineRow::Note { text } = &row else {
            out.push(row);
            continue;
        };
        let indent = text.len() - text.trim_start().len();
        let pad = " ".repeat(indent);
        let mut line = String::new();
        for word in text.split_whitespace() {
            if !line.is_empty() && pad.len() + line.chars().count() + 1 + word.chars().count() > width {
                out.push(PipelineRow::Note { text: format!("{pad}{line}").into() });
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(PipelineRow::Note { text: format!("{pad}{line}").into() });
    }
    out
}

/// The named field a row edits, if it edits one.
pub fn row_field(row: &PipelineRow) -> Option<&'static str> {
    match row {
        PipelineRow::AlgoSelect { field, .. }
        | PipelineRow::Param { field, .. }
        | PipelineRow::Toggle { field, .. } => Some(field),
        _ => None,
    }
}

const EXECUTION_MODE_OPTIONS: &[&str] = &["Local", "SLURM"];
const EXECUTION_MODE_HELP: &[&str] = &["Local execution or generate SLURM job scripts"; 2];

impl App {
    /// The active page's rows.
    pub fn rows(&self) -> Vec<PipelineRow> {
        self.page().map(|p| self.page_rows(p)).unwrap_or_default()
    }

    /// The settings Basic mode shows ([`qsmxt_engine::schema::regular_view`]), compared with the
    /// chosen pipeline's values or QSMxT's defaults; cached until the config changes.
    fn basic_paths(&self) -> BasicPaths {
        let config = self.current_config();
        let Ok(config) = config.as_ref() else { return Default::default() };
        let facts = self.view_facts();
        let key = format!("{}{facts:?}", serde_json::to_string(&config).unwrap_or_default());
        if let Some((k, paths)) = &*self.basic_cache.borrow() {
            if *k == key {
                return paths.clone();
            }
        }
        let base = qsmxt_engine::schema::baseline(config, &facts, &qsmxt_engine::PipelineConfig::default());
        let paths = std::rc::Rc::new(qsmxt_engine::schema::regular_view(config, &base, &facts));
        *self.basic_cache.borrow_mut() = Some((key, paths.clone()));
        paths
    }

    /// A Reconstruction section's rows in Basic mode: the settings [`Self::basic_paths`] lists
    /// (and the rows that edit no setting), the mask recipe as one line unless it is edited by
    /// hand, and a last row saying how many settings are hidden.
    fn basic_rows(&self, rows: Vec<PipelineRow>) -> Vec<PipelineRow> {
        use PipelineRow::*;
        let shown = self.basic_paths();
        let collapse_recipe = self.pipeline_state.mask_preset != *MASK_PRESET_CUSTOM;
        let main_recipe = |section: usize| !is_two_pass_section(section);
        let mut hidden = 0;
        // Each row, and whether it shows.
        let mut kept: Vec<(PipelineRow, bool)> = Vec::new();
        for row in rows {
            let keep = match &row {
                AlgoSelect { field, .. } | Param { field, .. } | Toggle { field, .. } => {
                    field_path(field).is_none_or(|p| shown.contains(p))
                }
                MaskCombineOp { section } | MaskOpInput { section } | MaskOpGenerator { section }
                | MaskOpGeneratorParam { section } | MaskOpThresholdValue { section } | MaskOpHdBetStep { section }
                | MaskOpBetVoxelScale { section } | MaskOpEntry { section, .. } | MaskOpSignalErodeParam { section, .. }
                | MaskOpAddStep { section } | MaskSectionHeader { section } => !(collapse_recipe && main_recipe(*section)),
                MaskOpAddSection { two_pass } => !(collapse_recipe && !*two_pass),
                MaskCombinedHeader => !collapse_recipe,
                _ => true,
            };
            if !keep && row_field(&row).is_some() {
                hidden += 1;
            }
            let preset = matches!(&row, AlgoSelect { field: "mask_preset", .. });
            kept.push((row, keep));
            if preset && collapse_recipe {
                hidden += 1; // the recipe's editor
                for text in self.mask().mask_recipe_summary() {
                    kept.push((Summary { text }, true));
                }
            }
        }
        // A note that heads a block of rows goes with them when they are all hidden.
        for i in 0..kept.len() {
            if matches!(kept[i].0, Note { .. }) {
                let block: Vec<bool> = kept[i + 1..].iter()
                    .take_while(|(r, _)| !matches!(r, Note { .. } | Separator))
                    .map(|(_, k)| *k)
                    .collect();
                if !block.is_empty() && block.iter().all(|k| !k) {
                    kept[i].1 = false;
                }
            }
        }
        let mut out: Vec<PipelineRow> = Vec::new();
        for (row, keep) in kept {
            let blank = matches!(row, Separator);
            if keep && !(blank && (out.is_empty() || matches!(out.last(), Some(Separator)))) {
                out.push(row);
            }
        }
        while matches!(out.last(), Some(Separator)) {
            out.pop();
        }
        if hidden > 0 {
            out.push(Separator);
            out.push(AdvancedFooter { hidden });
        }
        out
    }

    /// Switch between Basic and Advanced mode (`A`), keeping focus on the same setting where it
    /// still shows, and remember the choice.
    pub fn toggle_advanced(&mut self) {
        let field = self.focused_row().as_ref().and_then(row_field);
        self.advanced = !self.advanced;
        crate::prefs::set_advanced(self.advanced);
        if !(field.is_some_and(|f| self.focus_field(f))) {
            let n = self.focusable_rows().len();
            if self.focus() >= n {
                self.set_focus(n.saturating_sub(1));
            }
        }
    }

    /// Any page's rows, as they would show now.
    pub fn page_rows(&self, page: Page) -> Vec<PipelineRow> {
        let config = self.current_config();
        let config = config.as_ref().as_ref().ok();
        self.page_rows_in(page, &self.row_context_with(config), config)
    }

    /// Every row of the pipeline form's pages that shows now, page after page: for tests that ask
    /// whether a setting is reachable, not where.
    #[cfg(test)]
    pub fn pipeline_rows(&self) -> Vec<PipelineRow> {
        Page::RECONSTRUCTION.iter().filter(|p| p.is_pipeline()).flat_map(|&p| self.page_rows(p)).collect()
    }

    /// Every page of every tab with its rows, as they apply now (the config and context worked
    /// out once).
    pub fn all_pages(&self) -> Vec<(usize, Page, Vec<PipelineRow>)> {
        let config = self.current_config();
        let config = config.as_ref().as_ref().ok();
        let ctx = self.row_context_with(config);
        (0..TAB_NAMES.len())
            .flat_map(|t| self.pages(t).into_iter().map(move |p| (t, p)))
            .map(|(t, p)| (t, p, self.page_rows_in(p, &ctx, config)))
            .collect()
    }

    fn page_rows_in(&self, page: Page, ctx: &RowContext, config: Option<&qsmxt_engine::PipelineConfig>) -> Vec<PipelineRow> {
        let rows = self.all_page_rows_in(page, ctx, config);
        let rows = if self.advanced || !Page::RECONSTRUCTION.contains(&page) { rows } else { self.basic_rows(rows) };
        wrap_notes(rows, self.note_width.get().saturating_sub(2) as usize)
    }

    /// A page's rows with every setting shown (Advanced mode).
    fn all_page_rows_in(&self, page: Page, ctx: &RowContext, config: Option<&qsmxt_engine::PipelineConfig>) -> Vec<PipelineRow> {
        if page.is_pipeline() {
            // A stage that does not run says why, whoever builds its rows.
            if let Some(notes) = self.page_notes(page, ctx) {
                return notes;
            }
            return self.registry_rows(page, ctx, config).unwrap_or_else(|| {
                // Compute QSM is the one page built by hand.
                let mut rows = Vec::new();
                if page == Page::ComputeQsm {
                    self.compute_qsm_rows(ctx, &mut rows);
                }
                rows
            });
        }
        if let Some(rows) = self.registry_rows(page, ctx, config) {
            return rows;
        }
        match page {
            Page::Export => self.export_rows(),
            Page::Execution => self.execution_rows(),
            _ => Vec::new(),
        }
    }

    /// Indices into [`App::rows`] of the rows focus can land on.
    pub fn focusable_rows(&self) -> Vec<usize> {
        focusable_indices(&self.rows())
    }

    /// The focused row's position among [`App::focusable_rows`].
    pub fn focus(&self) -> usize {
        self.pipeline_state.focus
    }

    pub(crate) fn set_focus(&mut self, focus: usize) {
        self.pipeline_state.focus = focus;
    }

    /// The focused row of the active tab.
    pub fn focused_row(&self) -> Option<PipelineRow> {
        let rows = self.rows();
        let i = *focusable_indices(&rows).get(self.focus())?;
        rows.get(i).cloned()
    }

    /// The field the focused row edits, if it edits one.
    pub fn focused_field_name(&self) -> Option<String> {
        self.focused_row().as_ref().and_then(row_field).map(str::to_string)
    }

    /// After the rows changed, put focus back on the row editing `field` (or keep it in range).
    pub fn restore_focus(&mut self, field: &Option<String>) {
        if let Some(name) = field {
            if self.focus_field(name) {
                return;
            }
        }
        let max = self.focusable_rows().len().saturating_sub(1);
        if self.focus() > max {
            self.set_focus(max);
        }
    }

    /// Focus the row editing `field` on the active page; false if no visible row does.
    pub fn focus_field(&mut self, field: &str) -> bool {
        let rows = self.rows();
        match focusable_indices(&rows).iter().position(|&i| row_field(&rows[i]) == Some(field)) {
            Some(fi) => {
                self.set_focus(fi);
                true
            }
            None => false,
        }
    }

    /// Open the tab and sub-tab that show `field` now, and focus it; false if none does.
    pub fn goto_field(&mut self, field: &str) -> bool {
        let found = self.all_pages().into_iter()
            .find(|(_, _, rows)| rows.iter().any(|r| row_field(r) == Some(field)));
        let Some((tab, page, _)) = found else { return false };
        self.active_tab = tab;
        self.sub_page[tab] = Some(page);
        self.reset_view();
        self.sync_pipeline_mode();
        self.focus_field(field)
    }

    /// The Execution tab: local or SLURM, each mode's own settings, then what applies to both.
    fn execution_rows(&self) -> Vec<PipelineRow> {
        use PipelineRow::{AlgoSelect, Param, Toggle};
        let local = self.form.execution_mode == 0;
        let mut rows = vec![AlgoSelect { label: "Execution mode", field: "execution_mode", options: EXECUTION_MODE_OPTIONS, help: EXECUTION_MODE_HELP }];
        if local {
            rows.push(Toggle { label: "  Dry run", field: "dry_run", help: "Print the processing plan, pipeline graph and methods text without running anything" });
            rows.push(Param { label: "  Num processes", field: "n_procs", help: "Number of parallel threads (empty = auto)" });
            // The memory limit schedules concurrent local runs.
            rows.push(Param { label: "  Memory limit (GB)", field: "mem_limit_gb", help: "Memory budget for scheduling concurrent runs: auto (empty) detects it, a number of GB, or none for no limit" });
        } else {
            rows.push(Param { label: "  SLURM account", field: "slurm_account", help: "SLURM account name (required for SLURM mode)" });
            rows.push(Param { label: "  SLURM partition", field: "slurm_partition", help: "SLURM partition (optional)" });
            rows.push(Param { label: "  SLURM time limit", field: "slurm_time", help: "Wall time limit per job (e.g. 02:00:00)" });
            rows.push(Param { label: "  SLURM memory (GB)", field: "slurm_mem", help: "Memory per job in GB (empty = estimated per run)" });
            rows.push(Param { label: "  SLURM CPUs/Task", field: "slurm_cpus", help: "CPUs per SLURM task" });
            rows.push(Toggle { label: "  Auto-submit", field: "slurm_submit", help: "Automatically submit generated scripts via sbatch" });
        }
        rows.push(PipelineRow::Separator);
        rows.push(Toggle { label: "Debug logging", field: "debug", help: "Enable verbose debug log output" });
        rows.push(Toggle { label: "Force re-run", field: "force", help: "Ignore cached pipeline state and recompute everything" });
        rows.push(Toggle { label: "Clean intermediates", field: "clean_intermediates", help: "Remove intermediate files after the run, keeping only final outputs" });
        rows.push(PipelineRow::Separator);
        rows.push(PipelineRow::RunButton);
        rows
    }
}
