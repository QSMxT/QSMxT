//! The page layout table: which settings each page built from the registry lists, in what order,
//! and the rows and conditions the TUI adds to them.

use super::*;

/// Rows a page puts before a setting's row, from the form and the rows' context: a note, a
/// separator, or a choice the TUI offers that is no setting of its own.
pub(super) type Before = fn(&App, &RowContext) -> Vec<PipelineRow>;

/// A condition of the TUI's on a setting's row, from the form.
pub(super) type ShownIf = fn(&App) -> bool;

/// A page whose rows come from the registry.
pub(super) struct PageLayout {
    pub(super) page: Page,
    pub(super) section: Section,
    /// For a page that shows part of a section (SWI and SMWI share one): whether it shows the
    /// setting at this path.
    pub(super) only: Option<fn(&str) -> bool>,
    /// Settings of another section repeated at the end of the page, each under a setting of the
    /// page: `(path, under)`, shown while `under` is on and the setting applies.
    pub(super) repeats: &'static [(&'static str, &'static str)],
    /// Rows before a setting's row, while the setting applies.
    pub(super) before: &'static [(&'static str, Before)],
    /// Rows after a setting's rows, while they show.
    pub(super) after: &'static [(&'static str, Before)],
    /// A condition of the TUI's on a setting's row besides the setting applying.
    pub(super) shown_if: &'static [(&'static str, ShownIf)],
    /// Settings shown as a block under a setting of the page they have no parent in.
    pub(super) blocks: &'static [Block],
    /// Settings indented other than by their parents: `(path, depth)`.
    pub(super) depths: &'static [(&'static str, usize)],
    /// Settings listed first, in this order, ahead of the registry's.
    pub(super) first: &'static [&'static str],
    /// The page's own order where it is not the registry's, given the form.
    pub(super) arrange: Option<fn(&App, &mut Vec<&'static Setting>)>,
    /// Settings worded otherwise while a condition of the TUI's holds: `(path, when, label, help)`
    /// (the registry's help when `help` is `None`).
    pub(super) reworded: &'static [(&'static str, ShownIf, &'static str, Option<&'static str>)],
}

/// Settings shown as a block under a setting of a page they have no parent in: those whose path
/// starts with `prefix`, shown while `under` is on, after a note naming the block (`heading`).
/// A block of the page's own section stays where the registry puts it; one of another section
/// follows `under`'s row.
#[derive(Clone, Copy)]
pub(super) struct Block {
    pub(super) prefix: &'static str,
    pub(super) under: &'static str,
    pub(super) heading: &'static str,
}

impl Block {
    pub(super) const fn with_under(self, under: &'static str) -> Block {
        Block { under, ..self }
    }
}

/// The rigid registration serves motion correction and the co-registration of orientations, so
/// it is no one setting's.
pub(super) const REGISTRATION: Block =
    Block { prefix: "registration.", under: "motion_correction.enabled", heading: "Rigid registration (advanced):" };

impl PageLayout {
    /// How deep `s` is indented on the page: as [`PageLayout::depths`] says, else one deeper
    /// than its parent on the same page.
    pub(super) fn depth(&self, s: &Setting) -> usize {
        match (self.depths.iter().find(|(p, _)| *p == s.path), s.parent.map(setting)) {
            (Some(&(_, d)), _) => d,
            (None, Some(p)) if p.section == s.section => 1 + self.depth(p),
            _ => 0,
        }
    }
}

/// A page of a section's settings, in the registry's order and indentation.
pub(super) const fn plain(page: Page, section: Section) -> PageLayout {
    PageLayout {
        page, section, only: None, repeats: &[], before: &[], after: &[], shown_if: &[], blocks: &[], depths: &[], first: &[],
        arrange: None, reworded: &[],
    }
}

pub(super) fn is_swi(path: &str) -> bool {
    path == "pipeline.do_swi" || path.starts_with("swi.")
}

/// Where the segmentation comes from: a choice of the TUI's, which is whether a dseg tool is
/// named (`segmentation.custom_dseg_tool`).
pub(super) const SEGMENTATION_SOURCE_OPTIONS: &[&str] = &["synthseg", "custom dseg"];
pub(super) const SEGMENTATION_SOURCE_HELP: &[&str] = &[
    "SynthSeg: whole-brain parcellation straight off the GRE magnitude — no T1w, no registration. Needs a deep-learning build",
    "A dseg from BIDS derivatives (e.g. FreeSurfer's, made elsewhere); SynthSeg runs where none is found",
];

pub(super) const LAYOUTS: &[PageLayout] = &[
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
            ("qsm.per_echo", |app, ctx| (!app.field_mapping_runs_for(ctx.num_echoes).1)
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
            ("separation.custom_qsm_tool", |app, _| if app.field_toggle("pipeline.do_chi_separation") { vec![PipelineRow::Separator] } else { vec![] }),
        ],
        after: &[("separation.algorithm", |_, _| vec![PipelineRow::Separator])],
        // The supplied χ map shows here for χ-separation; the statistics and SMWI show it on
        // their own pages.
        shown_if: &[("separation.custom_qsm_tool", |app| app.field_toggle("pipeline.do_chi_separation"))],
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

pub(super) fn qsmart_chosen(app: &App) -> bool {
    app.form.settings.get("inversion.algorithm") == "qsmart"
}

/// The Inversion page's order: each method's settings, NLTV's echo time last; under QSMART, its
/// inner inversion's settings after the choice of it (iLSQR's tolerance, QSMART's own iLSQR
/// settings, then iLSQR's precision); then HEIDI's and the tiling's.
pub(super) fn arrange_inversion(app: &App, order: &mut Vec<&'static Setting>) {
    pub(super) fn take(order: &mut Vec<&'static Setting>, f: impl Fn(&str) -> bool) -> Vec<&'static Setting> {
        let (taken, kept) = order.iter().partition(|s| f(s.path));
        *order = kept;
        taken
    }
    pub(super) fn put_after(order: &mut Vec<&'static Setting>, after: &str, items: Vec<&'static Setting>) {
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
pub(super) fn arrange_separation(_: &App, order: &mut Vec<&'static Setting>) {
    if let Some(i) = order.iter().position(|s| s.path == "separation.chi_sep_medi.csf_r2star") {
        let csf = order.remove(i);
        let at = order.iter().position(|s| s.path == "separation.chi_sep_medi.cg_max_iter").map_or(order.len(), |i| i + 1);
        order.insert(at, csf);
    }
}
