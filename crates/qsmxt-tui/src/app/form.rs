//! The form: its settings, the pipeline form's screen state, and the facts that decide its rows.

use super::*;

// ─── Algorithm help text (name + DOI) ───

// One entry per QSM_ALGO_OPTIONS entry, in the same order. Consistent format:
//   "NAME (expansion) — one-line description. https://doi.org/…"
// Deep-learning nets are marked "(deep learning; weights auto-downloaded)". Keep this aligned
// with QSM_ALGO_OPTIONS — the coverage test asserts equal lengths.
/// Facts from outside the pipeline form that decide which of its rows apply: the other tabs'
/// settings (an output that forces QSM, the orientation grouping, the echo limit) and the maps
/// whose supplied inputs apply. Set by [`App::sync_pipeline_mode`] before every key
/// and every frame, from [`App::row_context`]; the default is a fresh form's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowContext {
    /// QSM is reconstructed although Compute QSM is off — SMWI needs a χ map, or the statistics
    /// have nothing else to summarise — and why.
    pub qsm_forced_by: Option<&'static str>,
    /// A multi-orientation grouping is set on the Input tab.
    pub orientation_group: bool,
    /// The grouping reconstructs COSMOS (rather than STI).
    pub orientation_cosmos: bool,
    /// The echo count when the run is limited to one (`--num-echoes`); `None` = unknown.
    pub num_echoes: Option<usize>,
    /// The maps whose bring-your-own inputs and settings apply: see [`InputRelevance`].
    pub inputs: InputRelevance,
    /// The selected methods that only work on axial data, so oblique data is resampled to axial
    /// whatever the obliquity threshold says (`bridge::axial_only_algorithms`), named.
    pub axial_forced_by: Option<String>,
    /// Whether coil combination can run: some selected run is uncombined multi-coil data, or
    /// the dataset is not known (no scan yet).
    pub multi_coil: bool,
}

/// Which bring-your-own inputs (and the R2' source) the run can use: the engine's
/// [`qsmxt_engine::requirements::InputsRead`], from the form's requests.
pub type InputRelevance = qsmxt_engine::requirements::InputsRead;

/// The pipeline form's state on screen: the page shown, what decides its rows, focus and editing,
/// and where the mask recipe came from. Its settings are in [`RunForm::settings`].
#[derive(Debug, Clone)]
pub struct PipelineFormState {
    /// Which page's rows to render — set by the App (from the active tab and sub-tab) before
    /// routing input or drawing.
    pub page: Page,
    /// What the rest of the form decides about these rows — set by the App with `page`.
    pub ctx: RowContext,
    /// The mask preset picker's choice: an index into [`MASK_PRESET_OPTIONS`], or
    /// [`MASK_PRESET_CUSTOM`] once the recipe is edited by hand. Not a setting: it names where
    /// the recipe (in the settings store, [`crate::mask_builder`]) came from.
    pub mask_preset: usize,

    // Pipeline tab UI state
    pub focus: usize,
    pub editing: bool,
    pub cursor: usize,
    pub scroll_offset: usize,

    // Mask ops editor state
    /// Text buffer while a mask row's number (a threshold value, BET's fractional intensity, a
    /// refinement step's value, a signal-erode setting) is edited.
    pub mask_value_buf: String,
    /// Whether the focused mask row's number is being edited ([`Self::mask_value_buf`]).
    pub mask_value_editing: bool,
}

impl Default for PipelineFormState {
    fn default() -> Self {
        Self {
            page: Page::Geometry,
            ctx: RowContext::default(),
            mask_preset: 0, // robust threshold
            focus: 0,
            editing: false,
            cursor: 0,
            scroll_offset: 0,
            mask_value_buf: String::new(),
            mask_value_editing: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunForm {
    // Tab 0: Input/Output
    pub bids_dir: String,
    pub output_dir: String,
    pub config_file: String,

    /// The settings of the sections whose rows come from the registry, in a config
    /// ([`crate::settings_rows`]).
    pub settings: crate::settings_rows::SettingsStore,
    /// The recommendations dismissed (Keep mine on a `recommended` or `discouraged` finding), each
    /// for the value its setting had then: `--dismiss-recommendation`, and so the run's config.
    pub dismissed: Vec<qsmxt_engine::Dismissal>,
    /// The toolbox pipeline chosen on the Pipeline tab: the config's `reference_pipeline`, while
    /// the form still makes its choices ([`crate::pipelines_tab`]).
    pub reference_pipeline: Option<String>,

    // Other outputs
    pub export_dicom: bool,
    /// DICOM export: source DICOM directory to inherit identity from, and the maps unticked on
    /// the Export section (by series suffix, lower case). The unticked ones are kept rather than
    /// the ticked, so a map turned on later is exported unless it is unticked too.
    pub source_dicom: String,
    pub dicom_unticked: Vec<String>,

    // Tab 4: Execution
    pub execution_mode: usize, // 0=Local, 1=SLURM
    pub dry_run: bool,
    pub debug: bool,
    pub n_procs: String,
    // Local runs: memory-aware scheduling, cache, and clean-up.
    pub mem_limit_gb: String,
    pub force: bool,
    pub clean_intermediates: bool,
    // SLURM fields
    pub slurm_account: String,
    pub slurm_partition: String,
    pub slurm_time: String,
    pub slurm_mem: String,
    pub slurm_cpus: String,
    pub slurm_submit: bool,
}

/// Named access to a group of same-typed fields: `$get`/`$get_mut` by the field's own name.
macro_rules! named_fields {
    ($get:ident, $get_mut:ident, $t:ty, [$($name:ident),* $(,)?]) => {
        pub fn $get(&self, field: &str) -> Option<&$t> {
            match field { $(stringify!($name) => Some(&self.$name),)* _ => None }
        }
        pub fn $get_mut(&mut self, field: &str) -> Option<&mut $t> {
            match field { $(stringify!($name) => Some(&mut self.$name),)* _ => None }
        }
    };
}

/// The run form's fields, by the names their rows carry.
impl RunForm {
    named_fields!(text, text_mut, String, [
        source_dicom, n_procs, mem_limit_gb, slurm_account,
        slurm_partition, slurm_time, slurm_mem, slurm_cpus,
    ]);
    named_fields!(flag, flag_mut, bool, [
        export_dicom,
        dry_run, debug, force, clean_intermediates, slurm_submit,
    ]);
    named_fields!(choice, choice_mut, usize, [execution_mode]);

    /// The memory limit row, read: `(limit in GB, no limit)` — empty or `auto` detects it.
    pub fn mem_limit(&self) -> (Option<f64>, bool) {
        let v = self.mem_limit_gb.trim();
        if v.eq_ignore_ascii_case("none") { return (None, true); }
        (v.parse().ok().filter(|g: &f64| *g > 0.0), false)
    }

}

impl Default for RunForm {
    fn default() -> Self {
        Self {
            bids_dir: String::new(),
            output_dir: String::new(),
            config_file: String::new(),
            settings: Default::default(),
            dismissed: Vec::new(),
            reference_pipeline: None,
            export_dicom: false,
            source_dicom: String::new(),
            dicom_unticked: Vec::new(),
            execution_mode: 0,
            dry_run: false,
            debug: false,
            n_procs: String::new(),
            mem_limit_gb: String::new(),
            force: false,
            clean_intermediates: false,
            slurm_account: String::new(),
            slurm_partition: String::new(),
            slurm_time: "02:00:00".to_string(),
            slurm_mem: String::new(),
            slurm_cpus: "4".to_string(),
            slurm_submit: false,
        }
    }
}

impl App {
    /// The config the form builds, or why it builds none: the frame's ([`Self::config`]) while a
    /// frame is prepared or drawn, else built now, as a key handler that has just changed the
    /// form needs it.
    pub fn current_config(&self) -> std::borrow::Cow<'_, Result<qsmxt_engine::PipelineConfig, String>> {
        match &self.config {
            Some(c) if self.in_frame => std::borrow::Cow::Borrowed(c),
            _ => std::borrow::Cow::Owned(crate::command::config_from_app(self).map_err(|e| e.to_string())),
        }
    }

    /// Point the pipeline form at the active page, with its context fresh. When the page
    /// changes under it (a tab or sub-tab switch, or a sub-tab that stopped applying), focus goes
    /// back to the top. Called by key handling and before each frame ([`Self::prepare_frame`]) so
    /// the display is consistent.
    pub fn sync_pipeline_mode(&mut self) {
        self.pipeline_state.ctx = self.row_context();
        if let Some(page) = self.page() {
            if page != self.pipeline_state.page {
                self.pipeline_state.page = page;
                self.reset_view();
            }
        }
    }

    /// What the rest of the form decides about the pipeline tabs' rows (see [`RowContext`]).
    pub fn row_context(&self) -> RowContext {
        let config = self.current_config();
        self.row_context_with(config.as_ref().as_ref().ok())
    }

    /// [`Self::row_context`], for the config the form builds now.
    pub(super) fn row_context_with(&self, config: Option<&qsmxt_engine::PipelineConfig>) -> RowContext {
        let group = self.filter_state.orientation_group_by();
        let algorithm = self.field_select("orientation_algorithm");
        // Whether QSM runs is the resolved config's answer — the same enforcement the run applies.
        // The config as the form gives it, and as the run takes it: QSM another output needs is
        // a required change the run makes itself when it is left off.
        let qsm_forced = config.as_ref().filter(|_| !self.form.settings.toggle("pipeline.do_qsm"))
            .and_then(|c| c.normalised().1.into_iter().find(|a| a.setting == "pipeline.do_qsm"));
        RowContext {
            qsm_forced_by: qsm_forced.map(|a| match a.because {
                "pipeline.do_smwi" => "SMWI weights the magnitude by the χ map (QSM → SMWI)",
                _ => "the per-structure statistics need a map to summarise (Other outputs → Segmentation)",
            }),
            orientation_group: group.is_some(),
            orientation_cosmos: algorithm == 0,
            num_echoes: self.filter_state.num_echoes.trim().parse().ok(),
            inputs: self.input_relevance(),
            axial_forced_by: config.as_ref().and_then(|c| {
                let names = qsmxt_engine::bridge::axial_only_algorithms(
                    &c.normalised().0, qsmxt_engine::bridge::RunOrientationFacts { has_measured_r2: false });
                (!names.is_empty()).then(|| names.join(", "))
            }),
            multi_coil: self.multi_coil(),
        }
    }

    /// Whether a selected run is uncombined multi-coil data (true until runs are listed).
    pub fn multi_coil(&self) -> bool {
        self.filter_state.tree.as_ref().is_none_or(|t| {
            let mut any = false;
            t.for_each_run(|leaf| any |= leaf.selected && leaf.multi_coil);
            any
        })
    }

    /// Which bring-your-own inputs the run can use, from what the form asks for (see
    /// [`InputRelevance`]). Decides both whether their rows show and whether they are passed on.
    pub fn input_relevance(&self) -> InputRelevance {
        let f = &self.form;
        let mut c = qsmxt_engine::PipelineConfig::default();
        let st = &f.settings;
        (c.pipeline.do_chi_separation, c.pipeline.do_analysis, c.pipeline.do_smwi) =
            (st.toggle("pipeline.do_chi_separation"), st.toggle("pipeline.do_analysis"), st.toggle("pipeline.do_smwi"));
        (c.pipeline.do_r2map, c.pipeline.do_r2primemap) = (st.toggle("pipeline.do_r2map"), st.toggle("pipeline.do_r2primemap"));
        if let Ok(sep) = serde_json::from_value(st.get("separation.algorithm")) {
            c.separation.algorithm = sep;
        }
        c.separation.custom_r2prime_tool = Some(st.text("separation.custom_r2prime_tool").trim().to_string()).filter(|t| !t.is_empty());
        // R2' is measured as R2* − R2 unless it is always predicted (R2PRIMEnet).
        if st.get("separation.r2prime_strategy") == "r2primenet" {
            c.separation.r2prime_strategy = qsmxt_engine::R2PrimeStrategy::R2primenet;
        }
        InputRelevance::of(&c)
    }
}
