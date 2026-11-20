//! Tabs and their pages: the tab indices, the Reconstruction sections and sub-tabs.

use super::*;

// Tab indices. Defined as named constants (usable directly in `match` patterns) so the tab
// order can be changed in ONE place — reorder `TAB_NAMES` and update these — without hunting
// down hardcoded `(tab, field)` literals across the file.

pub const TAB_INPUT: usize = 0;
/// The pipeline to start from ([`crate::pipelines_tab`]).
pub const TAB_PIPELINE: usize = 1;

/// Where a Reconstruction section stands for the current settings ([`App::section_state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionState {
    On,
    /// Its output is not made (χ-separation, SWI, SMWI, …, DICOM export, or QSM itself).
    Off,
    /// The settings skip its stage: no field mapping, no background removal, no orientation groups.
    NotApplicable,
}

/// Every reconstruction setting, its sections ([`Page::RECONSTRUCTION`]) in a sidebar.
pub const TAB_RECONSTRUCTION: usize = 2;
pub const TAB_EXECUTION: usize = 3;
pub const TAB_METHODS: usize = 4;

/// A page of a tab: a section of the Reconstruction tab's sidebar, or a sub-tab, switched with
/// `[` and `]`. A tab with a single page (Execution) shows neither. The Methods tab's two pages
/// are views, not rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Geometry,
    Masking,
    Motion,
    /// The QSM group's switch: whether QSM is reconstructed at all.
    ComputeQsm,
    FieldMap,
    Background,
    Inversion,
    MultiOrientation,
    Reference,
    Separation,
    /// SMWI: weighted by χ, so in the QSM group.
    Smwi,
    /// The combined magnitude, published or not.
    Magnitude,
    /// SWI: from the phase alone, so an other output.
    Swi,
    Relaxometry,
    Segmentation,
    Export,
    Execution,
    /// The Methods tab's methods text.
    MethodsText,
    /// The Methods tab's pipeline graph ([`crate::pipeline_view`]).
    Pipeline,
}

impl Page {
    /// The sub-tab's title.
    pub fn label(self) -> &'static str {
        match self {
            Page::Geometry => "Geometry",
            Page::Masking => "Masking",
            Page::Motion => "Motion",
            Page::ComputeQsm => "Compute QSM",
            Page::FieldMap => "Field map",
            Page::Background => "Background",
            Page::Inversion => "Inversion",
            Page::MultiOrientation => "COSMOS/STI",
            Page::Reference => "Reference",
            Page::Separation => "χ-separation",
            Page::Smwi => "SMWI",
            Page::Magnitude => "Magnitude",
            Page::Swi => "SWI",
            Page::Relaxometry => "Relaxometry",
            Page::Segmentation => "Segmentation",
            Page::Export => "DICOM",
            Page::Execution => "Execution",
            Page::MethodsText => "Methods text",
            Page::Pipeline => "Graph",
        }
    }

    /// Whether the page's rows are the pipeline form's ([`PipelineFormState::visible_rows`])
    /// rather than built by the App from both forms.
    pub fn is_pipeline(self) -> bool {
        !matches!(self, Page::Magnitude | Page::Swi | Page::Smwi | Page::Relaxometry | Page::Segmentation | Page::Export | Page::Execution
            | Page::MethodsText | Page::Pipeline)
    }

    /// The Reconstruction tab's sections, in sidebar order.
    pub const RECONSTRUCTION: [Page; 16] = [
        Page::Geometry, Page::Masking, Page::Motion, Page::ComputeQsm, Page::FieldMap, Page::Background,
        Page::Inversion, Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi, Page::Magnitude, Page::Swi,
        Page::Relaxometry, Page::Segmentation, Page::Export,
    ];

    /// The sidebar group a Reconstruction section is in. The QSM group holds QSM's stages and
    /// the outputs made from χ (χ-separation needs the QSM and its local field; SMWI weights by
    /// χ); the other outputs need no QSM.
    pub fn group(self) -> &'static str {
        match self {
            Page::Geometry | Page::Masking | Page::Motion => "Preprocessing",
            Page::ComputeQsm | Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation
            | Page::Reference | Page::Separation | Page::Smwi => "QSM",
            Page::Magnitude | Page::Swi | Page::Relaxometry | Page::Segmentation => "Other outputs",
            Page::Export => "Export",
            Page::Execution => "Execution",
            Page::MethodsText | Page::Pipeline => "Methods",
        }
    }

    /// Every page, in tab order.
    #[cfg(test)]
    pub const ALL: [Page; 19] = [
        Page::Geometry, Page::Masking, Page::Motion, Page::ComputeQsm, Page::FieldMap, Page::Background,
        Page::Inversion, Page::MultiOrientation, Page::Reference, Page::Separation, Page::Smwi, Page::Magnitude, Page::Swi,
        Page::Relaxometry, Page::Segmentation, Page::Export, Page::Execution, Page::MethodsText,
        Page::Pipeline,
    ];
}

pub const TAB_NAMES: [&str; 5] = [
    "Input",
    "Pipeline",
    "Reconstruction",
    "Execution",
    "Methods",
];

impl App {
    // ─── Row-based tabs: pages, rows, focus and named field access ───

    /// Whether a tab is drawn and driven as named rows ([`PipelineRow`]).
    pub fn is_rows_tab(tab: usize) -> bool {
        matches!(tab, TAB_RECONSTRUCTION | TAB_EXECUTION)
    }

    /// A tab's pages. The Reconstruction tab lists every section whatever the settings, so the
    /// sidebar keeps its shape; one that does not apply says why ([`App::section_state`]).
    pub fn pages(&self, tab: usize) -> Vec<Page> {
        match tab {
            TAB_RECONSTRUCTION => Page::RECONSTRUCTION.to_vec(),
            TAB_EXECUTION => vec![Page::Execution],
            TAB_METHODS => vec![Page::MethodsText, Page::Pipeline],
            _ => Vec::new(),
        }
    }

    /// The active sub-tab: the one last chosen on this tab while it still applies, else the
    /// tab's default ([`App::default_page`]), else the first.
    pub fn page(&self) -> Option<Page> {
        let pages = self.pages(self.active_tab);
        let chosen = self.sub_page.get(self.active_tab).copied().flatten();
        chosen.or_else(|| Self::default_page(self.active_tab)).filter(|p| pages.contains(p))
            .or_else(|| pages.first().copied())
    }

    /// The page a tab opens on before one is chosen, when it is not the first: Reconstruction
    /// opens on Masking, the section most runs need to look at.
    fn default_page(tab: usize) -> Option<Page> {
        (tab == TAB_RECONSTRUCTION).then_some(Page::Masking)
    }

    /// Whether a Reconstruction section's stage or output is part of the run now, for the
    /// sidebar: on, off (its output is not made), or not applicable to the current settings.
    pub fn section_state(&self, page: Page) -> SectionState {
        let on = |b: bool| if b { SectionState::On } else { SectionState::Off };
        match page {
            Page::ComputeQsm => on(self.qsm_on()),
            Page::FieldMap | Page::Background | Page::Inversion | Page::MultiOrientation | Page::Reference => {
                let ctx = self.row_context();
                if !self.qsm_runs(&ctx) {
                    SectionState::Off
                } else if (page == Page::FieldMap && !self.field_mapping_runs_for(ctx.num_echoes).0)
                    || (page == Page::Background && !self.inversion_needs_bg_removal())
                    || (page == Page::MultiOrientation && !ctx.orientation_group)
                {
                    SectionState::NotApplicable
                } else {
                    SectionState::On
                }
            }
            Page::Separation => on(self.form.settings.toggle("pipeline.do_chi_separation")),
            Page::Smwi => on(self.form.settings.toggle("pipeline.do_smwi")),
            Page::Magnitude => on(self.form.settings.toggle("pipeline.do_magnitude")),
            Page::Swi => on(self.form.settings.toggle("pipeline.do_swi")),
            Page::Relaxometry => on(["pipeline.do_t2starmap", "pipeline.do_r2starmap", "pipeline.do_r2map", "pipeline.do_r2primemap"].iter().any(|f| self.output_made(f))),
            Page::Segmentation => on(self.output_made("pipeline.do_segmentation") || self.form.settings.toggle("pipeline.do_analysis")),
            Page::Export => on(self.form.export_dicom),
            _ => SectionState::On,
        }
    }

    /// Whether QSM is reconstructed now (Compute QSM, or another output needs it).
    pub fn qsm_on(&self) -> bool {
        self.qsm_runs(&self.row_context())
    }

    /// The badge a Reconstruction section carries for the rules' findings on its rows: `Some(true)`
    /// when one is offered as a change (⚑), `Some(false)` for information only (ⓘ).
    pub fn section_marker(&self, view: &[qsmxt_engine::recommendations::Recommendation], page: Page) -> Option<bool> {
        let mut any = None;
        for row in self.page_rows(page) {
            if let Some(m) = App::row_marker(view, &row) {
                if m.actionable { return Some(true); }
                any = Some(false);
            }
        }
        any
    }

    /// Move to the next (`delta` 1) or previous (-1) sub-tab of the active tab, wrapping. Focus
    /// stays on the sub-tab strip if it was there, else goes to the first row.
    pub fn cycle_page(&mut self, delta: isize) {
        let pages = self.pages(self.active_tab);
        if pages.len() < 2 { return; }
        let on_strip = self.on_subtabs();
        let cur = self.page().and_then(|p| pages.iter().position(|&q| q == p)).unwrap_or(0);
        let next = pages[(cur as isize + delta).rem_euclid(pages.len() as isize) as usize];
        self.sub_page[self.active_tab] = Some(next);
        self.reset_view();
        self.sync_pipeline_mode();
        self.subtab_focus = on_strip;
    }

    /// Whether the active tab shows a sub-tab strip (a row-based tab with more than one page, or
    /// the Methods tab).
    pub fn has_subtabs(&self) -> bool {
        (Self::is_rows_tab(self.active_tab) || self.active_tab == TAB_METHODS) && self.pages(self.active_tab).len() > 1
    }

    /// Whether the active tab's pages are a sidebar (the Reconstruction tab on a wide terminal).
    pub fn sidebar(&self) -> bool {
        matches!(self.active_tab, TAB_RECONSTRUCTION | TAB_METHODS) && self.sidebar_layout.get()
    }

    /// Open the active tab's page `i` (clamped), keeping focus where it is.
    pub(super) fn goto_page_index(&mut self, i: usize) {
        let pages = self.pages(self.active_tab);
        let Some(&page) = pages.get(i.min(pages.len().saturating_sub(1))) else { return };
        let on_strip = self.on_subtabs();
        self.sub_page[self.active_tab] = Some(page);
        self.reset_view();
        self.sync_pipeline_mode();
        self.subtab_focus = on_strip;
    }

    /// Whether focus is on the sub-tab strip or the sidebar rather than on a row (or the tab bar).
    pub fn on_subtabs(&self) -> bool {
        self.subtab_focus && self.has_subtabs() && !self.tab_focus
    }

    /// Put focus and scrolling back at the top: on every change of tab or sub-tab.
    pub fn reset_view(&mut self) {
        self.subtab_focus = false;
        self.active_field = 0;
        self.form_scroll_offset = 0;
        self.editing = false;
        let ps = &mut self.pipeline_state;
        ps.focus = 0;
        ps.scroll_offset = 0;
        ps.editing = false;
        ps.mask_value_editing = false;
    }
}
