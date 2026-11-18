//! Rows built from the engine's settings registry ([`qsmxt_engine::schema`]), and the settings
//! they edit, kept in a config rather than in fields of the form.
//!
//! A page built here lists its section's settings in the registry's order, each shown while
//! `settings_view` says it applies, with one widget per value type: a checkbox for a bool, a
//! select for a choice, a text row for anything typed. Labels, help and options are the
//! registry's TUI wording; a setting is indented once for each setting it is listed under on the
//! same page. The rows keep the form's field names ([`FIELD_PATHS`]), which the rest of the TUI
//! (focus, advice, Basic mode) knows them by.
//!
//! The settings of the sections in [`STORED_SECTIONS`] live in [`SettingsStore`]: a config, plus
//! the text typed on their text rows. Other settings still live in the form's own fields until
//! their sections move here.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use qsmxt_engine::schema::{self, EchoKind, Section, Setting, ValueType, ViewFacts};
use qsmxt_engine::PipelineConfig;

use super::app::{field_of_path, field_path, App, Page, PipelineRow, RowContext, FIELD_PATHS, QSM_ALGO_OPTIONS};

/// The sections whose settings are kept in [`SettingsStore`] rather than in the form's fields.
pub const STORED_SECTIONS: &[Section] =
    &[Section::Geometry, Section::Masking, Section::Motion, Section::Background, Section::Reference, Section::Magnitude, Section::SwiSmwi,
      Section::Relaxometry, Section::Segmentation, Section::MultiOrientation, Section::FieldMap,
      Section::Inversion, Section::Separation];

/// Choices that one select picks with another: `(path, choice, other)`. The select lists the
/// setting's options but `choice`, then each of `other`'s, which picks `choice` with that value;
/// `other` has no row of its own. The QSM reference picker lists the regions with the methods.
const PICKERS: &[(&str, &str, &str)] = &[("qsm.reference", "region", "qsm.reference_region")];

fn picker(path: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    PICKERS.iter().find(|(p, ..)| *p == path)
}

/// Settings of [`STORED_SECTIONS`] still kept in the form's fields, with why: their rows come
/// from the registry all the same.
const KEPT_IN_FORM: &[(&str, &str)] = &[
    ("bg_removal.algorithm", "the pipeline form's rule for which stages run (stage_config) reads it"),
    ("inversion.algorithm", "the pipeline form's rule for which stages run (stage_config) reads it"),
    ("inversion.medi.smv", "the pipeline form's rule for which stages run (stage_config) reads it"),
    ("inversion.heidi.enabled", "the inversion picker's LSQR+HEIDI reads and sets it with the algorithm"),
    ("pipeline.do_qsm", "whether QSM runs (qsm_runs) reads it; its own page is built by hand"),
];

/// Rows a page puts before a setting's row, from the form and the rows' context: a note, a
/// separator, or a choice the TUI offers that is no setting of its own.
type Before = fn(&App, &RowContext) -> Vec<PipelineRow>;

/// A condition of the TUI's on a setting's row, from the form.
type ShownIf = fn(&App) -> bool;

/// A page whose rows come from the registry.
struct PageLayout {
    page: Page,
    section: Section,
    /// For a page that shows part of a section (SWI and SMWI share one): whether it shows the
    /// setting at this path.
    only: Option<fn(&str) -> bool>,
    /// Settings of another section repeated at the end of the page, each under a setting of the
    /// page: `(path, under)`, shown while `under` is on and the setting applies.
    repeats: &'static [(&'static str, &'static str)],
    /// Rows before a setting's row, while the setting applies.
    before: &'static [(&'static str, Before)],
    /// Rows after a setting's rows, while they show.
    after: &'static [(&'static str, Before)],
    /// A condition of the TUI's on a setting's row besides the setting applying.
    shown_if: &'static [(&'static str, ShownIf)],
    /// Settings shown as a block under a setting of the page they have no parent in.
    blocks: &'static [Block],
    /// Settings indented other than by their parents: `(path, depth)`.
    depths: &'static [(&'static str, usize)],
    /// Settings listed first, in this order, ahead of the registry's.
    first: &'static [&'static str],
    /// The page's own order where it is not the registry's, given the form.
    arrange: Option<fn(&App, &mut Vec<&'static Setting>)>,
    /// Settings worded otherwise while a condition of the TUI's holds: `(path, when, label, help)`
    /// (the registry's help when `help` is `None`).
    reworded: &'static [(&'static str, ShownIf, &'static str, Option<&'static str>)],
}

/// Settings shown as a block under a setting of a page they have no parent in: those whose path
/// starts with `prefix`, shown while `under` is on, after a note naming the block (`heading`).
/// A block of the page's own section stays where the registry puts it; one of another section
/// follows `under`'s row.
#[derive(Clone, Copy)]
struct Block {
    prefix: &'static str,
    under: &'static str,
    heading: &'static str,
}

impl Block {
    const fn with_under(self, under: &'static str) -> Block {
        Block { under, ..self }
    }
}

/// The rigid registration serves motion correction and the co-registration of orientations, so
/// it is no one setting's.
const REGISTRATION: Block =
    Block { prefix: "registration.", under: "motion_correction.enabled", heading: "Rigid registration (advanced):" };

impl PageLayout {
    /// How deep `s` is indented on the page: as [`PageLayout::depths`] says, else one deeper
    /// than its parent on the same page.
    fn depth(&self, s: &Setting) -> usize {
        match (self.depths.iter().find(|(p, _)| *p == s.path), s.parent.map(setting)) {
            (Some(&(_, d)), _) => d,
            (None, Some(p)) if p.section == s.section => 1 + self.depth(p),
            _ => 0,
        }
    }
}

/// A page of a section's settings, in the registry's order and indentation.
const fn plain(page: Page, section: Section) -> PageLayout {
    PageLayout {
        page, section, only: None, repeats: &[], before: &[], after: &[], shown_if: &[], blocks: &[], depths: &[], first: &[],
        arrange: None, reworded: &[],
    }
}

fn is_swi(path: &str) -> bool {
    path == "pipeline.do_swi" || path.starts_with("swi.")
}

/// Where the segmentation comes from: a choice of the TUI's, which is whether a dseg tool is
/// named (`segmentation.custom_dseg_tool`).
const SEGMENTATION_SOURCE_OPTIONS: &[&str] = &["synthseg", "custom dseg"];
const SEGMENTATION_SOURCE_HELP: &[&str] = &[
    "SynthSeg: whole-brain parcellation straight off the GRE magnitude — no T1w, no registration. Needs a deep-learning build",
    "A dseg from BIDS derivatives (e.g. FreeSurfer's, made elsewhere); SynthSeg runs where none is found",
];

/// Fields of rows that edit no setting of their own but say something of one (the segmentation's
/// source): a setting's own rows are the others that edit its path.
const TUI_CHOICES: &[&str] = &["segmentation_source"];

const LAYOUTS: &[PageLayout] = &[
    PageLayout {
        before: &[
            // A selected method that only works on axial data resamples whatever the switch says.
            ("pipeline.output_space", |_, ctx| ctx.axial_forced_by.iter()
                .map(|why| PipelineRow::Note { text: format!("  Oblique data is resampled to axial for {why}").into() })
                .collect()),
        ],
        ..plain(Page::Geometry, Section::Geometry)
    },
    PageLayout {
        blocks: &[REGISTRATION],
        ..plain(Page::Motion, Section::Motion)
    },
    PageLayout {
        // The mask's erosion and clean-up go with the method chosen; the deconvolution threshold
        // reads as one of V-SHARP's settings.
        depths: &[
            ("bg_removal.erode_voxels", 1), ("bg_removal.polish_mask", 1), ("bg_removal.ukb_mask_trim", 1),
            ("bg_removal.vsharp.threshold", 1),
        ],
        ..plain(Page::Background, Section::Background)
    },
    PageLayout {
        // The same rigid registration as motion correction's ([`REGISTRATION`]), under the
        // co-registration it serves here.
        blocks: &[REGISTRATION.with_under("multi_orientation.register")],
        // COSMOS's weighting reads as a setting of the page, its solver's settings under it.
        depths: &[
            ("multi_orientation.magnitude_weighting", 0), ("multi_orientation.cosmos_tol", 1),
            ("multi_orientation.cosmos_max_iter", 1),
        ],
        ..plain(Page::MultiOrientation, Section::MultiOrientation)
    },
    plain(Page::Reference, Section::Reference),
    plain(Page::Relaxometry, Section::Relaxometry),
    PageLayout {
        // The source is chosen first; the dseg tool is SynthSeg's alternative, and both sit under it.
        before: &[
            ("segmentation.custom_dseg_tool", |_, _| vec![PipelineRow::AlgoSelect {
                label: "  Source", field: "segmentation_source",
                options: SEGMENTATION_SOURCE_OPTIONS, help: SEGMENTATION_SOURCE_HELP,
            }]),
            ("pipeline.do_analysis", |_, _| vec![PipelineRow::Separator]),
        ],
        shown_if: &[("segmentation.custom_dseg_tool", |app| app.field_select("segmentation_source") == 1)],
        depths: &[
            ("segmentation.custom_dseg_tool", 2), ("segmentation.version", 2), ("segmentation.crop", 2),
            ("segmentation.patch", 2), ("segmentation.flip_averaging", 2), ("segmentation.topology_cleanup", 2),
            ("segmentation.sigma_smoothing", 2),
        ],
        // The statistics read a χ map, which may be supplied rather than computed.
        repeats: &[("separation.custom_qsm_tool", "pipeline.do_analysis")],
        ..plain(Page::Segmentation, Section::Segmentation)
    },
    PageLayout {
        // A QSM per echo comes first: it decides whether anything that combines echoes applies.
        first: &["qsm.per_echo"],
        before: &[
            ("qsm.per_echo", |app, ctx| (!app.pipeline_state.field_mapping_runs_for(ctx.num_echoes).1)
                .then(|| PipelineRow::Note { text: "Applies to multi-echo data only: TGV reconstructs a single echo from its wrapped phase".into() })
                .into_iter().collect()),
        ],
        // The template echo reads as one of ROMEO's settings, not as one under Individual mode.
        depths: &[("field_mapping.romeo.template", 1)],
        ..plain(Page::FieldMap, Section::FieldMap)
    },
    PageLayout {
        // Compute QSM has a page of its own.
        only: Some(|p| p != "pipeline.do_qsm"),
        arrange: Some(arrange_inversion),
        // Under QSMART, iLSQR's settings are its inner inversion's.
        reworded: &[
            ("inversion.ilsqr.tol", qsmart_chosen, "iLSQR tolerance", Some("Streaking-stage LSQR tolerance (shared with iLSQR)")),
            ("inversion.ilsqr.precision", qsmart_chosen, "iLSQR precision", None),
        ],
        // HEIDI reads as a step of its own after the inversion, its settings under it; MEDI's
        // mask shrinking and QSMART's iLSQR settings read as the method's own.
        depths: &[
            ("inversion.heidi.enabled", 0), ("inversion.medi.smv_shrink_mask", 1),
            ("inversion.qsmart.ilsqr_max_iter", 1), ("inversion.qsmart.ilsqr_pad_mm", 1),
        ],
        ..plain(Page::Inversion, Section::Inversion)
    },
    PageLayout {
        // The method stands apart from the switch above it and its settings below.
        before: &[
            ("separation.algorithm", |_, _| vec![PipelineRow::Separator]),
            ("separation.custom_qsm_tool", |app, _| if app.field_toggle("do_chi_separation") { vec![PipelineRow::Separator] } else { vec![] }),
        ],
        after: &[("separation.algorithm", |_, _| vec![PipelineRow::Separator])],
        // The supplied χ map shows here for χ-separation; the statistics and SMWI show it on
        // their own pages.
        shown_if: &[("separation.custom_qsm_tool", |app| app.field_toggle("do_chi_separation"))],
        // χ-sepnet's patches read as its settings, beside its route rather than under it.
        depths: &[
            ("separation.algorithm", 0), ("separation.custom_qsm_tool", 1),
            ("separation.chi_sepnet.patch", 1), ("separation.chi_sepnet.frame_patch", 1),
        ],
        arrange: Some(arrange_separation),
        ..plain(Page::Separation, Section::Separation)
    },
    plain(Page::Magnitude, Section::Magnitude),
    PageLayout { only: Some(is_swi), ..plain(Page::Swi, Section::SwiSmwi) },
    PageLayout {
        only: Some(|p| !is_swi(p)),
        // SMWI reads a χ map, which may be supplied rather than computed.
        repeats: &[("separation.custom_qsm_tool", "pipeline.do_smwi")],
        ..plain(Page::Smwi, Section::SwiSmwi)
    },
];

fn qsmart_chosen(app: &App) -> bool {
    QSM_ALGO_OPTIONS.get(app.pipeline_state.qsm_algorithm) == Some(&"qsmart")
}

/// The Inversion page's order: each method's settings, NLTV's echo time last; under QSMART, its
/// inner inversion's settings after the choice of it (iLSQR's tolerance, QSMART's own iLSQR
/// settings, then iLSQR's precision); then HEIDI's and the tiling's.
fn arrange_inversion(app: &App, order: &mut Vec<&'static Setting>) {
    fn take(order: &mut Vec<&'static Setting>, f: impl Fn(&str) -> bool) -> Vec<&'static Setting> {
        let (taken, kept) = order.iter().partition(|s| f(s.path));
        *order = kept;
        taken
    }
    fn put_after(order: &mut Vec<&'static Setting>, after: &str, items: Vec<&'static Setting>) {
        let at = order.iter().position(|s| s.path == after).map_or(order.len(), |i| i + 1);
        order.splice(at..at, items);
    }
    let te = take(order, |p| p == "inversion.nltv.te");
    put_after(order, "inversion.nltv.pad", te);
    let heidi = take(order, |p| p.starts_with("inversion.heidi."));
    let at = order.iter().position(|s| s.path == "inversion.tile_size").unwrap_or(order.len());
    order.splice(at..at, heidi);
    if qsmart_chosen(app) {
        let own = |p: &str| p.starts_with("inversion.qsmart.") || p.starts_with("inversion.heidi.")
            || p.starts_with("inversion.tile_") || p == "inversion.algorithm";
        let inner = take(order, |p| p.starts_with("inversion.") && !own(p));
        put_after(order, "inversion.qsmart.inversion", inner);
        let ilsqr = take(order, |p| p.starts_with("inversion.qsmart.ilsqr_"));
        put_after(order, "inversion.ilsqr.tol", ilsqr);
    }
}

/// The Separation page's order: χ-separation (MEDI)'s CSF R2* with its own settings, before those
/// of the iLSQR that reconstructs the QSM it starts from.
fn arrange_separation(_: &App, order: &mut Vec<&'static Setting>) {
    if let Some(i) = order.iter().position(|s| s.path == "separation.chi_sep_medi.csf_r2star") {
        let csf = order.remove(i);
        let at = order.iter().position(|s| s.path == "separation.chi_sep_medi.cg_max_iter").map_or(order.len(), |i| i + 1);
        order.insert(at, csf);
    }
}

/// The number a number-or-choice setting's select offers first, as its row words it:
/// `(path, label, help)`. Its value row edits the number.
const NUMBER_OPTIONS: &[(&str, &str, &str)] = &[
    ("separation.decompose.chi_bound", "ppm", "A fixed bound, in ppm (Chi bound (ppm); default: 0.5)"),
];

/// Text rows that read "(empty)" rather than "(default)" while blank, as the run form's rows did
/// before their settings moved here.
const READS_EMPTY: &[&str] = &[
    "swi.strength", "swi.hp_sigma", "swi.mip_window", "smwi.threshold_ppm", "smwi.power", "smwi.mip_window",
    "segmentation.crop", "segmentation.patch", "segmentation.sigma_smoothing",
];

/// Text rows that read blank at their default, which their placeholder names ("0 (none)").
const BLANK_AT_DEFAULT: &[&str] = &["multi_orientation.lambda"];

/// Triples typed as one value for every axis, or three, and shown as one while all three agree.
const ONE_VALUE_FOR_EVERY_AXIS: &[&str] = &["field_mapping.laplacian_fft_pad"];

/// Echo numbers the config counts from 0 and their rows from 1, as the command line does.
const COUNTED_FROM_ONE: &[&str] = &["field_mapping.romeo.template"];

/// Help the TUI words otherwise than the registry: a row counting from 1 says so.
const TUI_HELP: &[(&str, &str)] = &[("field_mapping.romeo.template", "Echo index for spatial unwrapping (1-indexed)")];

fn tui_help(s: &Setting) -> &'static str {
    TUI_HELP.iter().find(|(p, _)| *p == s.path).map_or(s.tui_help, |(_, h)| h)
}

/// Makes a setting's value on a fresh form ([`FORM_DEFAULTS`]).
type FormDefault = fn() -> Value;

/// Settings a fresh form holds otherwise than QSMxT's default, and so always passes on while they
/// apply. Unset, the reliable mask is the one the run resolves it to
/// (`default_two_pass_sections`); the form holds that mask, so that the mask builder shows and
/// edits it, and passes it on whenever two-pass is on, as it always has.
const FORM_DEFAULTS: &[(&str, FormDefault)] = &[
    ("masking.two_pass_sections", || serde_json::to_value(qsmxt_engine::default_two_pass_sections()).unwrap_or_default()),
];

/// A setting's value on a fresh form: [`FORM_DEFAULTS`]'s, else QSMxT's default.
fn form_default(path: &str) -> Value {
    FORM_DEFAULTS.iter().find(|(p, _)| *p == path).map_or_else(|| setting(path).default.clone(), |(_, v)| v())
}

/// Makes the value a checkbox takes when it is ticked ([`ON_VALUES`]).
type OnValue = fn() -> Value;

/// The value a checkbox with a value takes when it is ticked, for one off by default.
const ON_VALUES: &[(&str, OnValue)] = &[
    // ROMEO's own threshold for re-unwrapping, which QSMxT leaves off.
    ("field_mapping.romeo.temporal_uncertain_unwrapping",
     || Value::from(qsm_core::unwrap::RomeoParams::default().temporal_uncertain_unwrapping)),
    // Tiles of 128 voxels.
    ("inversion.tile_size", || Value::from(128)),
];

/// The value of a checkbox with a value when it is on and nothing else is chosen: its default,
/// or for one off by default, [`ON_VALUES`].
fn on_value(s: &Setting) -> Value {
    ON_VALUES.iter().find(|(p, _)| *p == s.path).map_or_else(|| s.default.clone(), |(_, v)| v())
}

/// Makes the time an echo-time row starts at ([`ECHO_TIME_MS`]).
type MsDefault = fn() -> String;

/// What a method's echo-time row starts at when a time is chosen: its default choice on
/// qsm-core's default echo times (blank if those give none).
const ECHO_TIME_MS: &[(&str, MsDefault)] = &[
    ("inversion.nltv.te", || { let p = qsm_core::inversion::NltvParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.medi.te", || { let p = qsm_core::inversion::MediParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.tfi.te", || { let p = qsm_core::inversion::TfiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.ndi.te", || { let p = qsm_core::inversion::NdiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.fansi.te", || { let p = qsm_core::inversion::FansiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.l1qsm.te", || { let p = qsm_core::inversion::L1QsmParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.whqsm.te", || { let p = qsm_core::inversion::WhQsmParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.heidi.te", || { let p = qsm_core::inversion::HeidiParams::default(); ms_default(p.te, &p.echo_times) }),
    ("inversion.star.te", || { let p = qsm_core::inversion::StarParams::default(); ms_default(p.te, &p.echo_times) }),
    // TGV's parameters carry no echo times: the ones qsm-core's NDI (also at the first echo by
    // default) starts from.
    ("inversion.tgv.te", || ms_default(qsm_core::inversion::TgvParams::default().te, &qsm_core::inversion::NdiParams::default().echo_times)),
];

fn ms_default(te: qsm_core::EchoTime, echo_times: &[f64]) -> String {
    te.resolve(echo_times).map(|s| qsmxt_engine::EchoTimeChoice::Seconds(s).to_string()).unwrap_or_default()
}

/// The time an echo-time row starts at ([`ECHO_TIME_MS`]).
fn echo_time_ms(path: &str) -> String {
    ECHO_TIME_MS.iter().find(|(p, _)| *p == path).map(|(_, f)| f()).unwrap_or_default()
}

/// The number of a number-or-choice setting when the number is chosen: its default, when that is
/// a number, else the number its value row's help names (none).
fn number_default(s: &Setting) -> Value {
    if s.default.is_number() { s.default.clone() } else { Value::Null }
}

/// An echo time in ms, as the config holds it.
fn echo_time_of_ms(text: &str) -> Option<Value> {
    text.trim().parse::<f64>().ok()?;
    serde_json::to_value(qsmxt_engine::EchoTimeChoice::parse(text).ok()?).ok()
}

/// Every setting, by path.
fn by_path() -> &'static HashMap<&'static str, &'static Setting> {
    static MAP: LazyLock<HashMap<&'static str, &'static Setting>> =
        LazyLock::new(|| schema::settings().iter().map(|s| (s.path, s)).collect());
    &MAP
}

pub(super) fn setting(path: &str) -> &'static Setting {
    by_path().get(path).unwrap_or_else(|| panic!("no setting {path}"))
}

/// The config path of a row's field, when its setting is kept in [`SettingsStore`].
pub fn stored_path(field: &str) -> Option<&'static str> {
    field_path(field).filter(|p| {
        STORED_SECTIONS.contains(&setting(p).section) && !KEPT_IN_FORM.iter().any(|(k, _)| k == p)
    }).filter(|_| !TUI_CHOICES.contains(&field))
}

/// A select's options and their help, as the TUI words them, for the setting at `path`.
pub fn options(path: &str) -> (&'static [&'static str], &'static [&'static str]) {
    type Options = (Vec<&'static str>, Vec<&'static str>);
    static MAP: LazyLock<HashMap<&'static str, Options>> = LazyLock::new(|| {
        schema::settings().iter()
            .map(|s| {
                let mut opts = s.value_type.options();
                if let Some(&(_, choice, other)) = picker(s.path) {
                    opts.retain(|o| o.value != choice);
                    opts.extend(setting(other).value_type.options());
                }
                let mut labels: Vec<&'static str> = opts.iter().map(|o| o.label).collect();
                let mut help: Vec<&'static str> = opts.iter().map(|o| match s.value_type {
                    // The echo times are every method's; the row says which is this one's default.
                    ValueType::EchoTime { .. } if o.value == s.default =>
                        Box::leak(format!("{} (default)", tui_option_help(o)).into_boxed_str()),
                    _ => tui_option_help(o),
                }).collect();
                // A number, offered before the named choices.
                if let Some(&(_, label, h)) = NUMBER_OPTIONS.iter().find(|(p, ..)| *p == s.path) {
                    labels.insert(0, label);
                    help.insert(0, h);
                }
                // A name for a value with a switch turned on, listed after that value.
                for a in schema::aliases::PICKER_ALIASES.iter().filter(|a| a.setting == s.path) {
                    if let Some(i) = opts.iter().position(|o| o.value == a.value) {
                        labels.insert(i + 1, a.name);
                        help.insert(i + 1, a.help);
                    }
                }
                (s.path, (labels, help))
            })
            .collect()
    });
    let (labels, help) = &MAP[path];
    (labels, help)
}

/// An option's help as the TUI shows it: with its link, which the registry keeps apart. Made once
/// per option, so kept for good.
fn tui_option_help(o: &'static schema::EnumOption) -> &'static str {
    match &o.url {
        // After a sentence, the link follows it; after a name, a dash leads to it.
        Some(url) if o.tui_help.ends_with('.') => Box::leak(format!("{} {url}", o.tui_help).into_boxed_str()),
        Some(url) => Box::leak(format!("{} — {url}", o.tui_help).into_boxed_str()),
        None => o.tui_help.as_str(),
    }
}

/// `label` indented `depth` levels. Rows carry `&'static str` labels, so each indented label is
/// made once and kept; there are as many as the registry has labels, at most a few levels deep.
fn indented(depth: usize, label: &'static str) -> &'static str {
    if depth == 0 {
        return label;
    }
    static MADE: LazyLock<Mutex<HashMap<(usize, &'static str), &'static str>>> = LazyLock::new(Default::default);
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    made.entry((depth, label))
        .or_insert_with(|| Box::leak(format!("{}{label}", "  ".repeat(depth)).into_boxed_str()))
}

/// How many settings `s` is listed under on its own section's page.
pub(super) fn depth_of(s: &Setting) -> usize {
    match s.parent.map(setting) {
        Some(p) if p.section == s.section => 1 + depth_of(p),
        _ => 0,
    }
}

/// Write `value` at `path` in a config being built as JSON.
fn put(config: &mut Value, path: &str, value: Value) {
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut obj = config;
    for k in parent.split('.').filter(|k| !k.is_empty()) {
        obj = obj.as_object_mut().expect("an object").entry(k).or_insert_with(|| Value::Object(Default::default()));
    }
    if let Some(map) = obj.as_object_mut() {
        map.insert(key.to_string(), value);
    }
}

/// The config with these values and QSMxT's defaults elsewhere, if it accepts them all. Built
/// from the values alone (every section of the config fills in its own defaults), so it costs
/// as little as there are values.
fn config_with(values: &[(&'static str, Value)]) -> Option<PipelineConfig> {
    config_try(values).ok()
}

/// `config` with these values written onto it; a value it refuses is left as `config` has it.
pub fn with_values(config: &PipelineConfig, values: &[(&'static str, Value)]) -> PipelineConfig {
    let Ok(mut json) = serde_json::to_value(config) else { return config.clone() };
    for (p, v) in values {
        put(&mut json, p, v.clone());
    }
    serde_json::from_value(json).unwrap_or_else(|_| {
        let mut out = config.clone();
        for (p, v) in values {
            let _ = schema::set(&mut out, p, v.clone());
        }
        out
    })
}

/// [`config_with`], or why the config refuses the values.
fn config_try(values: &[(&'static str, Value)]) -> Result<PipelineConfig, String> {
    let mut json = Value::Object(Default::default());
    for (p, v) in values {
        put(&mut json, p, v.clone());
    }
    serde_json::from_value(json).map_err(|e| e.to_string())
}

/// A value as its text row shows it: a number as Rust writes it (4, 0.5), a triple or list with
/// spaces between, nothing for an unset value.
fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(xs) => xs.iter().map(text_of).collect::<Vec<_>>().join(" "),
        Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
            (Some(u), _, _) => u.to_string(),
            (_, Some(i), _) => i.to_string(),
            (_, _, Some(f)) => format!("{f}"),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

/// The value text typed on a row of `s` stands for, or `None` for a blank or unreadable one. A
/// blank one leaves the setting at its default (as the command line does without the option);
/// an unreadable one is refused while it applies ([`SettingsStore::changed_or_refused`]).
fn parse(s: &Setting, text: &str) -> Option<Value> {
    let text = text.trim();
    let numbers = || text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty());
    let number = |t: &str| t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::from);
    let int = |t: &str| t.parse::<u64>().ok().map(Value::from).or_else(|| t.parse::<i64>().ok().map(Value::from));
    match s.value_type {
        _ if text.is_empty() && !matches!(s.value_type, ValueType::Text) => None,
        // Typed from 1; 0 is the first echo too, as the command line takes it.
        ValueType::Int if COUNTED_FROM_ONE.contains(&s.path) => text.parse::<u64>().ok().map(|n| Value::from(n.saturating_sub(1))),
        ValueType::Int => int(text),
        ValueType::Float => number(text),
        ValueType::FloatVec3 => numbers().map(number).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntVec3 if ONE_VALUE_FOR_EVERY_AXIS.contains(&s.path) => {
            let v = text.split_whitespace().map(int).collect::<Option<Vec<_>>>()?;
            match v.len() {
                1 => Some(Value::from(vec![v[0].clone(); 3])),
                3 => Some(Value::from(v)),
                _ => None,
            }
        }
        // Whole-number triples are typed with spaces, as their rows always took them.
        ValueType::IntVec3 => text.split_whitespace().map(int).collect::<Option<Vec<_>>>().filter(|v| v.len() == 3).map(Value::from),
        ValueType::IntList => numbers().map(int).collect::<Option<Vec<_>>>().filter(|v| !v.is_empty()).map(Value::from),
        ValueType::Text if text.is_empty() => Some(if s.optional { Value::Null } else { Value::from("") }),
        ValueType::Text => Some(Value::from(text)),
        ValueType::EchoTime { .. } => echo_time_of_ms(text),
        ValueType::NumberOrChoice { .. } => number(text),
        // The echo number of an echo reference, written as the config writes it ("3").
        ValueType::EchoReference { .. } => echo_number(s, &Value::from(text)).map(|n| Value::from(n.to_string())),
        _ => None,
    }
}

/// A setting whose typed text the run cannot take, and why.
pub type Refused = (&'static str, String);

/// What a text row of `s` takes, as a message refusing other text words it ("not a number").
fn what_it_takes(s: &Setting) -> &'static str {
    match s.value_type {
        ValueType::Int if COUNTED_FROM_ONE.contains(&s.path) => "an echo number",
        ValueType::Int => "a whole number",
        ValueType::Float | ValueType::NumberOrChoice { .. } => "a number",
        ValueType::FloatVec3 => "three numbers",
        ValueType::IntVec3 if ONE_VALUE_FOR_EVERY_AXIS.contains(&s.path) => "one whole number, or three",
        ValueType::IntVec3 => "three whole numbers",
        ValueType::IntList => "a list of whole numbers",
        ValueType::EchoTime { .. } => "a time in ms",
        _ => "a value this setting takes",
    }
}

/// The echo number an echo reference's value gives, when it gives one the setting takes.
fn echo_number(s: &Setting, v: &Value) -> Option<u32> {
    let ValueType::EchoReference { options } = &s.value_type else { return None };
    let n: u32 = v.as_str()?.trim().parse().ok()?;
    options.iter().any(|o| matches!(o.kind, EchoKind::EchoNumber { min } if n >= min)).then_some(n)
}

/// The settings of [`STORED_SECTIONS`]: the values that differ from QSMxT's defaults, and the
/// text typed on their text rows, by path. A typed text stands for its value (see
/// [`SettingsStore::config`]), so the row shows what was typed, a half-typed number included.
#[derive(Clone, Debug, Default)]
pub struct SettingsStore {
    values: BTreeMap<&'static str, Value>,
    typed: BTreeMap<&'static str, String>,
}

impl SettingsStore {
    /// A setting's value (not counting typed text).
    pub fn get(&self, path: &str) -> Value {
        self.values.get(path).cloned().unwrap_or_else(|| form_default(path))
    }

    /// Set a value, dropping any text typed for it. A value the config refuses is left out.
    pub fn set(&mut self, path: &'static str, value: impl Into<Value>) {
        self.typed.remove(path);
        self.put(path, value.into());
    }

    /// Set a value, leaving any text typed for it.
    fn put(&mut self, path: &'static str, value: Value) {
        if value == form_default(path) {
            self.values.remove(path);
        } else if config_with(&[(path, value.clone())]).is_some() {
            self.values.insert(path, value);
        }
    }

    /// A checkbox: a bool, or whether a setting with a value that turns it off is on.
    pub fn toggle(&self, path: &str) -> bool {
        match &setting(path).off {
            Some(off) => self.get(path) != *off,
            None => self.get(path).as_bool().unwrap_or(false),
        }
    }

    pub fn flip(&mut self, path: &'static str) {
        let on = self.toggle(path);
        self.switch(path, !on);
    }

    /// Tick or untick a checkbox. A setting with a value that turns it off keeps its value's text
    /// while it is off, and takes it again when it is turned back on (its default, when that
    /// text is no value).
    pub fn switch(&mut self, path: &'static str, on: bool) {
        let s = setting(path);
        let Some(off) = &s.off else { return self.set(path, on) };
        if on == self.toggle(path) {
            return;
        }
        if on {
            let v = self.typed.get(path).and_then(|t| parse(s, t)).filter(|v| v != off)
                .unwrap_or_else(|| on_value(s));
            self.put(path, v);
        } else {
            if !self.typed.contains_key(path) {
                let text = self.value_text(path);
                self.typed.insert(path, text);
            }
            self.put(path, off.clone());
        }
    }

    /// A choice's position in the setting's options. An echo reference given as an echo number
    /// is its echo-number option.
    pub fn select(&self, path: &str) -> usize {
        let v = self.get(path);
        let s = setting(path);
        if let Some(&(_, choice, other)) = picker(path) {
            let own = s.value_type.options().into_iter().filter(|o| o.value != choice).collect::<Vec<_>>();
            if v == choice {
                let w = self.get(other);
                return setting(other).value_type.options().iter().position(|o| o.value == w).map_or(0, |i| own.len() + i);
            }
            return own.iter().position(|o| o.value == v).unwrap_or(0);
        }
        let number = echo_number(s, &v).is_some();
        s.value_type.options().iter().position(|o| o.value == v).or_else(|| match &s.value_type {
            ValueType::EchoReference { options } =>
                options.iter().position(|o| number && matches!(o.kind, EchoKind::EchoNumber { .. })),
            // A given time: `{ "seconds": s }`.
            ValueType::EchoTime { options } if v.is_object() => options.iter().position(|o| o.kind == EchoKind::Time),
            _ => None,
        }).map(|i| i + usize::from(matches!(s.value_type, ValueType::NumberOrChoice { .. })))
        .unwrap_or(0)
    }

    pub fn set_select(&mut self, path: &'static str, index: usize) {
        if index == self.select(path) {
            return; // an echo number chosen again keeps its number
        }
        let s = setting(path);
        if let Some(&(_, choice, other)) = picker(path) {
            let own = s.value_type.options().into_iter().filter(|o| o.value != choice).collect::<Vec<_>>();
            match own.get(index) {
                Some(o) => {
                    self.set(path, o.value.clone());
                    self.set(other, setting(other).default.clone());
                }
                None => {
                    let others = setting(other).value_type.options();
                    if let Some(o) = others.get((index - own.len()).min(others.len().saturating_sub(1))) {
                        self.set(path, choice);
                        self.set(other, o.value.clone());
                    }
                }
            }
            return;
        }
        if let (ValueType::NumberOrChoice { .. }, Some(i)) = (&s.value_type, index.checked_sub(1)) {
            // A named choice; the number typed waits under it.
            let opts = s.value_type.options();
            if let Some(o) = opts.get(i.min(opts.len().saturating_sub(1))) {
                self.put(path, o.value.clone());
            }
            return;
        }
        if let ValueType::NumberOrChoice { .. } = s.value_type {
            // The number: the one typed, else the default's.
            let v = self.typed.get(path).and_then(|t| parse(s, t)).unwrap_or_else(|| number_default(s));
            return self.put(path, v);
        }
        let opts = s.value_type.options();
        if let Some(o) = opts.get(index.min(opts.len().saturating_sub(1))) {
            match &s.value_type {
                // The echo number typed waits under a named echo, for the number to be chosen again.
                ValueType::EchoReference { .. } => self.put(path, o.value.clone()),
                // A given time: the one typed, else the method's on the default echo times. The
                // time typed waits under a named echo time, as an echo number does.
                ValueType::EchoTime { options } if options[index.min(options.len() - 1)].kind == EchoKind::Time => {
                    let text = self.typed.get(path).cloned().unwrap_or_else(|| echo_time_ms(path));
                    if let Some(v) = echo_time_of_ms(&text).or_else(|| echo_time_of_ms(&echo_time_ms(path))) {
                        self.put(path, v);
                    }
                }
                ValueType::EchoTime { .. } => self.put(path, o.value.clone()),
                _ => self.set(path, o.value.clone()),
            }
        }
    }

    /// A text row's text: what was typed, else the value's.
    pub fn text(&self, path: &str) -> Cow<'_, str> {
        match self.typed.get(path) {
            Some(t) => Cow::Borrowed(t),
            None => Cow::Owned(self.value_text(path)),
        }
    }

    /// The text of a row, to type into.
    pub fn text_mut(&mut self, path: &'static str) -> &mut String {
        if !self.typed.contains_key(path) {
            let text = self.value_text(path);
            self.typed.insert(path, text);
        }
        self.typed.get_mut(path).expect("just inserted")
    }

    /// A value as its text row shows it. An echo reference's row shows its echo number: the
    /// value's, or the first the setting takes while a named echo is chosen.
    fn value_text(&self, path: &str) -> String {
        let s = setting(path);
        let v = self.get(path);
        match &s.value_type {
            _ if BLANK_AT_DEFAULT.contains(&path) && v == s.default => String::new(),
            _ if ONE_VALUE_FOR_EVERY_AXIS.contains(&path) => match v.as_array() {
                Some(xs) if !xs.is_empty() && xs.iter().all(|x| *x == xs[0]) => text_of(&xs[0]),
                _ => text_of(&v),
            },
            // A given echo time, in ms; while a named one is chosen, the method's on the default
            // echo times.
            ValueType::EchoTime { .. } => serde_json::from_value::<qsmxt_engine::EchoTimeChoice>(v.clone()).ok()
                .filter(|te| matches!(te, qsmxt_engine::EchoTimeChoice::Seconds(_)))
                .map_or_else(|| echo_time_ms(path), |te| te.to_string()),
            // The number; while a named choice is chosen, the default's.
            ValueType::NumberOrChoice { .. } if !v.is_number() => text_of(&number_default(s)),
            _ if COUNTED_FROM_ONE.contains(&path) => v.as_u64().map_or_else(|| text_of(&v), |n| (n + 1).to_string()),
            ValueType::EchoReference { options } => echo_number(s, &v).map(|n| n.to_string()).unwrap_or_else(|| {
                options.iter().find_map(|o| match o.kind { EchoKind::EchoNumber { min } => Some(min.to_string()), _ => None })
                    .unwrap_or_default()
            }),
            _ => text_of(&v),
        }
    }

    /// Put a setting back to QSMxT's default. `value_row` resets only the value's row: an echo
    /// reference's number, an echo time's time or a number-or-choice's number (the choice leaves
    /// the number, as the two rows are reset one at a time), or the value of a checkbox with a
    /// value.
    pub fn reset(&mut self, path: &'static str, value_row: bool) {
        let s = setting(path);
        let echo = matches!(s.value_type,
            ValueType::EchoReference { .. } | ValueType::EchoTime { .. } | ValueType::NumberOrChoice { .. });
        if value_row || !echo {
            self.typed.remove(path);
        }
        if !value_row {
            self.values.remove(path);
        } else if s.off.is_some() && self.toggle(path) {
            // The value of a checkbox that is on: back to the default value, still on.
            self.put(path, on_value(s));
        } else if matches!(s.value_type, ValueType::NumberOrChoice { .. }) && self.get(path).is_number() {
            // The number: back to the default's, still a number.
            self.put(path, number_default(s));
        } else if matches!(s.value_type, ValueType::EchoTime { .. }) && self.get(path).is_object() {
            // A given echo time: back to the method's, still given.
            if let Some(v) = echo_time_of_ms(&echo_time_ms(path)) {
                self.put(path, v);
            }
        }
    }

    /// The stored settings that differ from QSMxT's defaults as the run takes them (those a fresh
    /// form holds otherwise, [`FORM_DEFAULTS`], included), typed text
    /// read as its value (a blank text leaves the default), and the settings whose typed text the
    /// run cannot take, with why: text that reads as no value of its setting ("abc" for a
    /// number), a number outside the setting's range or one the config cannot hold, or an echo
    /// number the setting does not take. The command line refuses all of them, and running with
    /// the default instead would change a setting behind the user's back.
    pub fn changed_or_refused(&self) -> (Vec<(&'static str, Value)>, Vec<Refused>) {
        let mut out: BTreeMap<&'static str, Value> = self.values.clone();
        for (path, value) in FORM_DEFAULTS {
            out.entry(path).or_insert_with(value);
        }
        let mut refused = Vec::new();
        for (&path, text) in &self.typed {
            let s = setting(path);
            // A value kept while its checkbox is off is not taken.
            if s.off.as_ref().is_some_and(|off| self.get(path) == *off) {
                continue;
            }
            // A time typed is taken only while a given time is chosen, a number only while the
            // number is.
            if matches!(s.value_type, ValueType::EchoTime { .. }) && !self.get(path).is_object()
                || matches!(s.value_type, ValueType::NumberOrChoice { .. }) && !self.get(path).is_number()
            {
                continue;
            }
            if let ValueType::EchoReference { .. } = s.value_type {
                // Taken only while an echo by number is chosen, and as it is typed.
                if echo_number(s, &self.get(path)).is_none() {
                    continue;
                }
                let v = Value::from(text.trim());
                match config_try(&[(path, v.clone())]) {
                    Ok(_) => out.insert(path, v),
                    Err(e) => {
                        refused.push((path, format!("{} {}: {e}", s.cli_flag.unwrap_or(path), text.trim())));
                        out.remove(path)
                    }
                };
                continue;
            }
            let value = parse(s, text);
            if value.is_none() && !text.trim().is_empty() {
                let flag = s.cli_flag.unwrap_or(path);
                refused.push((path, format!("{flag} {}: not {}", text.trim(), what_it_takes(s))));
                out.remove(path);
                continue;
            }
            let Some(v) = value else {
                out.remove(path);
                continue;
            };
            // A number outside the setting's range, or one the config cannot hold (a negative
            // count), is refused as the run refuses it, rather than run as the default.
            let flag = s.cli_flag.unwrap_or(path);
            let numbers: Vec<f64> = match &v {
                Value::Array(xs) => xs.iter().filter_map(Value::as_f64).collect(),
                other => other.as_f64().into_iter().collect(),
            };
            let why = if numbers.iter().any(|&x| !s.in_range(x)) {
                Some(format!("must be {}", s.range_text()))
            } else {
                config_try(&[(path, v.clone())]).err().map(|e| match s.value_type {
                    ValueType::Int | ValueType::IntVec3 | ValueType::IntList if numbers.iter().any(|&x| x < 0.0) =>
                        "must be at least 0".to_string(),
                    _ => e,
                })
            };
            if let Some(why) = why {
                refused.push((path, format!("{flag} {}: {why}", text.trim())));
                out.remove(path);
                continue;
            }
            if v == s.default { out.remove(path) } else { out.insert(path, v) };
        }
        (out.into_iter().collect(), refused)
    }

    /// [`Self::changed_or_refused`]'s values.
    pub fn changed(&self) -> Vec<(&'static str, Value)> {
        self.changed_or_refused().0
    }

    /// The config with these of the stored settings ([`Self::changed`]) and QSMxT's defaults
    /// elsewhere.
    pub fn config_of(&self, values: &[(&'static str, Value)]) -> PipelineConfig {
        config_with(values).unwrap_or_default()
    }

    /// The config of the stored settings, QSMxT's defaults elsewhere.
    pub fn config(&self) -> PipelineConfig {
        self.config_of(&self.changed())
    }
}

/// The placeholder of a blank text row whose setting is stored, when it is one.
pub fn stored_placeholder(path: &str) -> &'static str {
    if READS_EMPTY.contains(&path) { "(empty)" } else { "(default)" }
}

/// The settings that apply to a config ([`schema::visible_settings`]), kept until the config or
/// the facts change: several pages are built for one config.
pub type VisibleCache = RefCell<Option<(PipelineConfig, ViewFacts, Rc<HashSet<&'static str>>)>>;

impl App {
    /// The settings that apply to `config`, from [`Self::visible_cache`] when it has them.
    pub(super) fn visible_settings(&self, config: &PipelineConfig) -> Rc<HashSet<&'static str>> {
        let facts = self.row_facts();
        if let Some((c, f, v)) = &*self.visible_cache.borrow() {
            if c == config && *f == facts {
                return v.clone();
            }
        }
        let visible: Rc<HashSet<&'static str>> = Rc::new(schema::visible_settings(config, &facts).into_iter()
            .filter(|(_, v)| *v).map(|(p, _)| p).collect());
        *self.visible_cache.borrow_mut() = Some((config.clone(), facts, visible.clone()));
        visible
    }

    /// What the rows' visibility is decided from: the echo limit and whether the selected runs
    /// are uncombined multi-coil data.
    pub fn row_facts(&self) -> ViewFacts {
        ViewFacts {
            num_echoes: self.filter_state.num_echoes.trim().parse().ok(),
            multi_coil: Some(self.multi_coil()),
            ..Default::default()
        }
    }

    /// The rows of `page` when they come from the registry; `None` for a page still built by
    /// hand.
    pub(super) fn registry_rows(&self, page: Page, ctx: &RowContext, config: Option<&PipelineConfig>) -> Option<Vec<PipelineRow>> {
        let layout = LAYOUTS.iter().find(|l| l.page == page);
        if layout.is_none() && page != Page::Masking {
            return None;
        }
        // A form that builds no config (a value the run refuses) still shows its rows, as its
        // stored settings have them.
        let stored;
        let config = match config {
            Some(c) => c,
            None => {
                stored = self.form.settings.config();
                &stored
            }
        };
        // The Masking page is the mask builder's ([`super::mask_builder`]).
        let Some(layout) = layout else { return Some(self.masking_rows(config)) };
        let visible = self.visible_settings(config);
        let shows = |path: &str| visible.contains(path);
        let is_on = |path: &str| field_of_path(path).is_some_and(|f| self.field_toggle(f));
        let mut rows = Vec::new();
        let mut headed = HashSet::new();
        // A block's heading, once, then its settings under it.
        let mut block_row = |rows: &mut Vec<PipelineRow>, b: &Block, s: &'static Setting| {
            let depth = depth_of(setting(b.under)) + 1;
            if headed.insert(b.prefix) {
                rows.push(PipelineRow::Note { text: format!("{}{}", "  ".repeat(depth), b.heading).into() });
            }
            self.push_rows(rows, s, depth + 1);
        };
        let foreign = |b: &Block| schema::settings().iter().any(|t| t.path.starts_with(b.prefix) && t.section != layout.section);
        let mut order: Vec<&'static Setting> = layout.first.iter().map(|p| setting(p))
            .chain(schema::settings().iter().filter(|s| !layout.first.contains(&s.path))).collect();
        if let Some(arrange) = layout.arrange {
            arrange(self, &mut order);
        }
        for s in order {
            if s.section != layout.section || !layout.only.is_none_or(|f| f(s.path)) || !shows(s.path)
                || PICKERS.iter().any(|(.., other)| *other == s.path)
            {
                continue;
            }
            if let Some(b) = layout.blocks.iter().find(|b| s.path.starts_with(b.prefix)) {
                if is_on(b.under) {
                    block_row(&mut rows, b, s);
                }
                continue;
            }
            let depth = layout.depth(s);
            for (_, before) in layout.before.iter().filter(|(p, _)| *p == s.path) {
                rows.extend(before(self, ctx));
            }
            if !layout.shown_if.iter().filter(|(p, _)| *p == s.path).all(|(_, f)| f(self)) {
                continue;
            }
            match layout.reworded.iter().find(|(p, when, ..)| *p == s.path && when(self)) {
                Some(&(_, _, label, help)) => self.push_rows_worded(&mut rows, s, depth, label, help.unwrap_or(tui_help(s))),
                None => self.push_rows(&mut rows, s, depth),
            }
            for (_, after) in layout.after.iter().filter(|(p, _)| *p == s.path) {
                rows.extend(after(self, ctx));
            }
            // A block of another section follows the setting it is under.
            for b in layout.blocks.iter().filter(|b| b.under == s.path && foreign(b) && is_on(b.under)) {
                for t in schema::settings().iter().filter(|t| t.path.starts_with(b.prefix) && shows(t.path)) {
                    block_row(&mut rows, b, t);
                }
            }
        }
        for (path, under) in layout.repeats {
            if is_on(under) && shows(path) {
                self.push_rows(&mut rows, setting(path), depth_of(setting(under)) + 1);
            }
        }
        Some(rows)
    }
}

impl App {
    /// The rows of a setting, by its value type: one; for an echo reference given as an echo
    /// number, the select and the number's row under it; for a setting with a value that turns it
    /// off, a checkbox and, while it is on, the value's row under it.
    pub(super) fn push_rows(&self, rows: &mut Vec<PipelineRow>, s: &'static Setting, depth: usize) {
        self.push_rows_worded(rows, s, depth, s.label, tui_help(s));
    }

    /// [`Self::push_rows`], the setting's own row labelled `label` with `help`.
    fn push_rows_worded(&self, rows: &mut Vec<PipelineRow>, s: &'static Setting, depth: usize, label: &'static str, help: &'static str) {
        let field = row_field(s.path, 0).unwrap_or_else(|| panic!("{} has no row field", s.path));
        let label = indented(depth, label);
        rows.push(match s.value_type {
            _ if s.off.is_some() => PipelineRow::Toggle { label, field, help },
            ValueType::Bool => PipelineRow::Toggle { label, field, help },
            ValueType::Enum { .. } | ValueType::EchoReference { .. } | ValueType::EchoTime { .. }
            | ValueType::NumberOrChoice { .. } => {
                let (options, help) = options(s.path);
                PipelineRow::AlgoSelect { label, field, options, help }
            }
            ValueType::Int | ValueType::Float | ValueType::IntVec3 | ValueType::FloatVec3 | ValueType::IntList
            | ValueType::Text => PipelineRow::Param { label, field, help },
            _ => unreachable!("{}: no widget for its value type yet", s.path),
        });
        let chosen = |options: &[schema::EchoOption]| options.get(self.field_select(field)).map(|o| o.kind);
        let value_row = match &s.value_type {
            _ if s.off.is_some() => self.field_toggle(field),
            ValueType::EchoReference { options } => matches!(chosen(options), Some(EchoKind::EchoNumber { .. })),
            ValueType::EchoTime { options } => matches!(chosen(options), Some(EchoKind::Time)),
            ValueType::NumberOrChoice { .. } => self.field_select(field) == 0,
            _ => false,
        };
        if let (true, Some((label, help))) = (value_row, s.tui_value) {
            let field = row_field(s.path, 1).unwrap_or_else(|| panic!("{} has no value row field", s.path));
            rows.push(PipelineRow::Param { label: indented(depth + 1, label), field, help });
        }
    }
}

/// The field of the `nth` row editing a setting: its own row (0), or its value's row (1). A row of
/// a choice the TUI makes about the setting ([`TUI_CHOICES`]) is neither.
fn row_field(path: &str, nth: usize) -> Option<&'static str> {
    FIELD_PATHS.iter().filter(|(f, p)| *p == path && !TUI_CHOICES.contains(f)).nth(nth).map(|(f, _)| *f)
}

/// Whether `field` is the value row of the setting at `path` (an echo reference's number, the
/// sigma of a checkbox with a value).
pub fn is_value_row(field: &str, path: &str) -> bool {
    row_field(path, 1) == Some(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::command::config_from_app;

    /// A text row shows what was typed; the run takes it when it reads as the setting's value,
    /// the default when it is blank, and refuses it when it reads as no value (see
    /// [`unreadable`]).
    #[test]
    fn typed_text_stands_for_its_value() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        *app.field_text_mut("swi_strength").unwrap() = "2.5".into();
        assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.5);
        *app.field_text_mut("swi_strength").unwrap() = "2.".into();
        assert_eq!(app.field_text("swi_strength"), "2.");
        assert_eq!(config_from_app(&app).unwrap().swi.strength, 2.0);
        *app.field_text_mut("swi_strength").unwrap() = " ".into();
        assert_eq!(config_from_app(&app).unwrap().swi.strength, qsmxt_engine::SwiConfig::default().strength);
        *app.field_text_mut("swi_strength").unwrap() = "x".into();
        assert!(config_from_app(&app).is_err(), "unreadable text is refused, not run as the default");
    }

    /// Type `text` on `field`'s row and check that the config is refused with `message`, as the
    /// command line refuses such a value, rather than run with the setting's default.
    fn unreadable(app: &mut App, field: &str, text: &str, message: &str) {
        *app.field_text_mut(field).unwrap() = text.into();
        let err = config_from_app(app).expect_err(&format!("{field} = {text:?} is refused")).to_string();
        assert!(err.contains(message), "{field}: {err}");
    }

    #[test]
    fn unreadable_geometry_value_is_refused() {
        let mut app = App::new_for_trial();
        unreadable(&mut app, "tukey_window", "abc", "--tukey-window abc: not a number");
    }

    #[test]
    fn unreadable_swi_and_smwi_values_are_refused() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        unreadable(&mut app, "swi_strength", "1.2.3", "--swi-strength 1.2.3: not a number");
        *app.field_text_mut("swi_strength").unwrap() = String::new();
        app.form.settings.set("pipeline.do_smwi", true);
        unreadable(&mut app, "smwi_power", "abc", "--smwi-power abc: not a number");
    }

    #[test]
    fn unreadable_motion_value_is_refused() {
        let mut app = App::new_for_trial();
        app.toggle_field("motion_correction");
        unreadable(&mut app, "motion_min_ncc", "abc", "--motion-min-ncc abc: not a number");
    }

    #[test]
    fn unreadable_background_value_is_refused() {
        let mut app = App::new_for_trial();
        unreadable(&mut app, "vsharp_max_radius", "abc", "--vsharp-max-radius abc: not a number");
    }

    /// The Reference page has no typed row: a method and a region, both chosen from lists.
    #[test]
    fn the_reference_page_has_nothing_typed() {
        let typed: Vec<_> = schema::settings().iter()
            .filter(|s| s.section == Section::Reference && s.off.is_none())
            .filter(|s| matches!(s.value_type, ValueType::Int | ValueType::Float | ValueType::IntVec3
                | ValueType::FloatVec3 | ValueType::IntList | ValueType::Text))
            .map(|s| s.path).collect();
        assert!(typed.is_empty(), "test an unreadable value of {typed:?}");
    }

    #[test]
    fn unreadable_relaxometry_value_is_refused() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_r2primemap", true);
        unreadable(&mut app, "r2primenet_patch", "128 128", "--r2primenet-patch 128 128: not three whole numbers");
    }

    #[test]
    fn unreadable_segmentation_value_is_refused() {
        let mut app = App::new_for_trial();
        app.toggle_field("do_segmentation");
        unreadable(&mut app, "synthseg_sigma", "abc", "--synthseg-sigma abc: not a number");
    }

    #[test]
    fn unreadable_multi_orientation_value_is_refused() {
        let mut app = App::new_for_trial();
        app.filter_state.orientation_group = "acq".into();
        unreadable(&mut app, "orientation_lambda", "abc", "--multi-orientation-lambda abc: not a number");
        // Blank is no regularisation, as its placeholder says.
        *app.field_text_mut("orientation_lambda").unwrap() = String::new();
        assert!(config_from_app(&app).is_ok());
    }

    #[test]
    fn unreadable_field_map_values_are_refused() {
        let mut app = App::new_for_trial();
        unreadable(&mut app, "romeo_template", "abc", "--romeo-template abc: not an echo number");
        *app.field_text_mut("romeo_template").unwrap() = "1".into();
        app.set_field_select("unwrapping_algorithm", 1);
        app.set_field_select("laplacian_solver", 1);
        unreadable(&mut app, "laplacian_fft_pad", "32 32", "--laplacian-fft-pad 32 32: not one whole number, or three");
    }

    #[test]
    fn unreadable_inversion_values_are_refused() {
        let mut app = App::new_for_trial();
        app.pipeline_state.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap();
        unreadable(&mut app, "nltv_lambda", "abc", "--nltv-lambda abc: not a number");
        *app.field_text_mut("nltv_lambda").unwrap() = String::new();
        let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
        app.set_field_select("nltv_te", ms);
        unreadable(&mut app, "nltv_te_ms", "abc", "--nltv-te abc: not a time in ms");
    }

    #[test]
    fn unreadable_separation_value_is_refused() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_chi_separation", true);
        app.form.settings.set("separation.algorithm", "decompose");
        unreadable(&mut app, "sep_decompose_chi_bound", "abc", "--decompose-chi-bound abc: not a number");
    }

    /// A typed number outside its setting's range, or one the config cannot hold, is refused as
    /// the run refuses it, rather than run as the setting's default: a number, a count, a triple.
    #[test]
    fn out_of_range_values_are_refused() {
        let mut app = App::new_for_trial();
        app.set_field_select("mask_preset", 1); // BET
        unreadable(&mut app, "bet_smoothness", "-1", "--bet-smoothness -1: must be greater than 0");
        *app.field_text_mut("bet_smoothness").unwrap() = String::new();
        unreadable(&mut app, "bet_iterations", "-1", "--bet-iterations -1: must be at least 0");
        *app.field_text_mut("bet_iterations").unwrap() = String::new();
        unreadable(&mut app, "homogeneity_nbox", "0", "--homogeneity-nbox 0: must be at least 1");
        *app.field_text_mut("homogeneity_nbox").unwrap() = String::new();
        app.set_field_select("inhomogeneity_method", 1);
        unreadable(&mut app, "n4_mesh", "1 0 1", "--n4-mesh 1 0 1: must be at least 1");
        *app.field_text_mut("n4_mesh").unwrap() = "2 2 2".into();
        assert_eq!(config_from_app(&app).unwrap().n4.mesh, [2, 2, 2]);
    }

    /// An out-of-range number in a row that does not apply does not block the run.
    #[test]
    fn an_out_of_range_hidden_value_does_not_block() {
        let mut app = App::new_for_trial();
        unreadable(&mut app, "homogeneity_nbox", "0", "--homogeneity-nbox 0");
        app.toggle_field("inhomogeneity_correction");
        assert!(config_from_app(&app).is_ok(), "the correction off: its boxes do not apply");
    }

    /// Unreadable text in a row that does not apply does not block the run: its setting is left
    /// at its default, as for any value of a hidden setting.
    #[test]
    fn unreadable_text_of_a_hidden_setting_does_not_block() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        unreadable(&mut app, "swi_strength", "abc", "--swi-strength abc");
        app.toggle_field("do_swi");
        assert!(config_from_app(&app).is_ok(), "SWI off: its strength does not apply");
        // A time typed under a named echo time, and a method not chosen.
        app.pipeline_state.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap();
        let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
        app.set_field_select("nltv_te", ms);
        unreadable(&mut app, "nltv_te_ms", "abc", "--nltv-te abc");
        app.set_field_select("nltv_te", 1);
        assert!(config_from_app(&app).is_ok(), "a named echo time: the time typed does not apply");
        app.set_field_select("nltv_te", ms);
        assert!(config_from_app(&app).is_err());
        app.pipeline_state.qsm_algorithm = 0;
        assert!(config_from_app(&app).is_ok(), "another method: NLTV's echo time does not apply");
    }

    /// A stored setting that no longer applies is left at its default, though the form keeps it.
    #[test]
    fn a_hidden_setting_is_left_at_its_default() {
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_swi", true);
        app.set_field_select("swi_scaling", 3);
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
        app.toggle_field("do_swi");
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Tanh);
        app.toggle_field("do_swi");
        assert_eq!(config_from_app(&app).unwrap().swi.scaling, qsmxt_engine::SwiScaling::Negative);
    }

    /// An echo number the run cannot take is refused, as the command line refuses it, rather
    /// than run with another reference; with motion correction off it does not apply.
    #[test]
    fn an_unreadable_echo_number_is_an_error() {
        let mut app = App::new_for_trial();
        app.toggle_field("motion_correction");
        app.set_field_select("motion_reference", 2);
        *app.field_text_mut("motion_reference_echo").unwrap() = "x".into();
        let err = config_from_app(&app).expect_err("an unreadable echo number");
        assert!(err.to_string().contains("--motion-reference x"), "{err}");
        *app.field_text_mut("motion_reference_echo").unwrap() = "0".into();
        assert!(config_from_app(&app).is_err(), "echoes count from 1");
        app.toggle_field("motion_correction");
        assert!(config_from_app(&app).is_ok(), "motion correction off: the number does not apply");
    }

    /// The echo number typed stays while a named echo is chosen, and is the reference again once
    /// the echo number is.
    #[test]
    fn the_echo_number_survives_a_named_choice() {
        use qsmxt_engine::MotionReference;
        let mut app = App::new_for_trial();
        app.toggle_field("motion_correction");
        app.set_field_select("motion_reference", 2);
        *app.field_text_mut("motion_reference_echo").unwrap() = "3".into();
        let reference = |app: &App| config_from_app(app).unwrap().motion_correction.reference;
        assert_eq!(reference(&app), MotionReference::Echo(3));
        app.set_field_select("motion_reference", 1);
        assert_eq!(reference(&app), MotionReference::Last);
        app.set_field_select("motion_reference", 2);
        assert_eq!(app.field_text("motion_reference_echo"), "3");
        assert_eq!(reference(&app), MotionReference::Echo(3));
    }

    /// ROMEO's template echo is typed from 1, as the command line takes it, and the config counts
    /// from 0; the FFT pad is one value for every axis, or three.
    #[test]
    fn rows_type_values_as_the_command_line_does() {
        let mut app = App::new_for_trial();
        assert_eq!(app.field_text("romeo_template"), "1");
        *app.field_text_mut("romeo_template").unwrap() = "3".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.romeo.template, 2);
        app.set_field_select("unwrapping_algorithm", 1);
        app.set_field_select("laplacian_solver", 1);
        assert_eq!(app.field_text("laplacian_fft_pad"), "64");
        *app.field_text_mut("laplacian_fft_pad").unwrap() = "32".into();
        assert_eq!(config_from_app(&app).unwrap().field_mapping.laplacian_fft_pad, [32; 3]);
    }

    /// An echo time is a select with a time in ms under it while a given time is chosen: the row
    /// starts at the method's default time, and the time typed waits under a named echo time.
    #[test]
    fn an_echo_time_keeps_its_time_under_a_named_one() {
        use qsmxt_engine::EchoTimeChoice;
        let mut app = App::new_for_trial();
        app.pipeline_state.qsm_algorithm = QSM_ALGO_OPTIONS.iter().position(|a| *a == "nltv").unwrap();
        let te = |app: &App| config_from_app(app).unwrap().inversion.nltv.te;
        let ms = options("inversion.nltv.te").0.iter().position(|o| *o == "ms").unwrap();
        assert!(options("inversion.nltv.te").1[0].ends_with("(default)"), "NLTV's default is the first echo time");
        app.set_field_select("nltv_te", ms);
        assert_eq!(app.field_text("nltv_te_ms"), "20");
        assert_eq!(te(&app), EchoTimeChoice::Seconds(0.02));
        *app.field_text_mut("nltv_te_ms").unwrap() = "7".into();
        assert_eq!(te(&app), EchoTimeChoice::Seconds(0.007));
        app.set_field_select("nltv_te", 1);
        assert_eq!(te(&app), EchoTimeChoice::Last);
        app.set_field_select("nltv_te", ms);
        assert_eq!((app.field_text("nltv_te_ms").as_ref(), te(&app)), ("7", EchoTimeChoice::Seconds(0.007)));
    }

    /// DECOMPOSE's χ bound is a number or a named choice: the select offers the number first, its
    /// row under it while it is chosen, and the number typed waits under the named choice.
    #[test]
    fn a_number_or_choice_keeps_its_number_under_a_choice() {
        use qsmxt_engine::ChiBoundChoice;
        let mut app = App::new_for_trial();
        app.form.settings.set("pipeline.do_chi_separation", true);
        app.form.settings.set("separation.algorithm", "decompose");
        let bound = |app: &App| config_from_app(app).unwrap().separation.decompose.chi_bound;
        assert_eq!(options("separation.decompose.chi_bound").0, ["ppm", "echo-spacing"]);
        assert_eq!(app.field_text("sep_decompose_chi_bound"), "0.5");
        *app.field_text_mut("sep_decompose_chi_bound").unwrap() = "0.3".into();
        assert_eq!(bound(&app), ChiBoundChoice::Ppm(0.3));
        app.set_field_select("sep_decompose_chi_bound_mode", 1);
        assert_eq!(bound(&app), ChiBoundChoice::EchoSpacing);
        app.set_field_select("sep_decompose_chi_bound_mode", 0);
        assert_eq!((app.field_text("sep_decompose_chi_bound").as_ref(), bound(&app)), ("0.3", ChiBoundChoice::Ppm(0.3)));
    }

    /// Posterior smoothing is a checkbox with its sigma under it: off is a sigma of 0, and the
    /// sigma typed is kept while it is off and taken again when it is back on.
    #[test]
    fn a_checkbox_with_a_value_keeps_its_value_while_off() {
        let mut app = App::new_for_trial();
        app.toggle_field("do_segmentation");
        let sigma = |app: &App| config_from_app(app).unwrap().segmentation.sigma_smoothing;
        assert!(app.field_toggle("synthseg_smoothing"));
        *app.field_text_mut("synthseg_sigma").unwrap() = "1.5".into();
        assert_eq!(sigma(&app), 1.5);
        app.toggle_field("synthseg_smoothing");
        assert!(!app.field_toggle("synthseg_smoothing"));
        assert_eq!(sigma(&app), 0.0);
        app.toggle_field("synthseg_smoothing");
        assert_eq!((app.field_text("synthseg_sigma").as_ref(), sigma(&app)), ("1.5", 1.5));
    }
}
