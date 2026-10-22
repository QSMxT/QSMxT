//! The engine's settings schema (`qsmxt_engine::schema`) agrees with the TUI: the same labels and
//! help, the same sub-tabs, and the same settings shown for the same configuration.
//!
//! The TUI's form is explored from a fresh one by changing one choice at a time (every option
//! of every select, every checkbox, an orientation grouping and an echo limit) until no new
//! rows appear. In each form reached, the settings the TUI shows must be exactly the ones
//! `settings_view` reports visible for the config the form builds.

use std::collections::{BTreeMap, BTreeSet};

use qsmxt_engine::schema::{self, Section, Setting, ValueType, ViewFacts};

use super::app::*;
use super::command::config_from_app;

/// The config setting a TUI row edits, by the row's field name. Rows that edit no config
/// setting (the Execution tab, DICOM export's run options, the mask preset, which writes the
/// recipe) are in [`NOT_CONFIG`].
const FIELD_PATHS: &[(&str, &str)] = &[
    ("axial_resampling", "pipeline.axial_resampling"), ("obliquity_threshold", "pipeline.obliquity_threshold"),
    ("output_space", "pipeline.output_space"), ("inhomogeneity_correction", "masking.inhomogeneity_correction"),
    ("homogeneity_sigma_mm", "homogeneity.sigma_mm"), ("homogeneity_nbox", "homogeneity.nbox"),
    ("ignore_slice_gap", "pipeline.ignore_slice_gap"), ("crop_to_mask", "pipeline.crop_to_mask"),
    ("crop_margin_mm", "pipeline.crop_margin_mm"), ("fft_padding", "pipeline.fft_padding"),
    ("coil_combination_sigma", "field_mapping.coil_combination_sigma"),
    ("custom_mask_tool", "masking.custom_mask_tool"), ("two_pass", "masking.two_pass"),
    ("bet_smoothness", "bet.smoothness"), ("bet_gradient_threshold", "bet.gradient_threshold"),
    ("bet_iterations", "bet.iterations"), ("bet_subdivisions", "bet.subdivisions"),
    ("motion_correction", "motion_correction.enabled"), ("motion_reference", "motion_correction.reference"),
    ("motion_reference_echo", "motion_correction.reference"), ("motion_chain", "motion_correction.chain"),
    ("motion_chain_levels", "motion_correction.chain_levels"), ("motion_min_ncc", "motion_correction.min_ncc"),
    ("reg_levels", "registration.levels"), ("reg_max_samples", "registration.max_samples"),
    ("reg_max_iterations", "registration.max_iterations"), ("reg_max_evaluations", "registration.max_evaluations"),
    ("reg_tolerance", "registration.tolerance"), ("reg_rotation_step_deg", "registration.rotation_step_deg"),
    ("reg_translation_step_mm", "registration.translation_step_mm"), ("reg_min_overlap", "registration.min_overlap"),
    ("do_qsm", "pipeline.do_qsm"),
    ("unwrapping_algorithm", "field_mapping.unwrapping_algorithm"), ("laplacian_solver", "field_mapping.laplacian_solver"),
    ("laplacian_fft_pad", "field_mapping.laplacian_fft_pad"), ("phase_offset_removal", "field_mapping.phase_offset_removal"),
    ("phase_offset_sigma_mm", "field_mapping.phase_offset_sigma_mm"), ("bipolar_correction", "field_mapping.bipolar_correction"),
    ("romeo_individual", "field_mapping.romeo.individual"), ("romeo_template", "field_mapping.romeo.template"),
    ("romeo_correct_global", "field_mapping.romeo.correct_global"), ("romeo_phase_coherence", "field_mapping.romeo.phase_coherence"),
    ("romeo_phase_gradient_coherence", "field_mapping.romeo.phase_gradient_coherence"),
    ("romeo_phase_linearity", "field_mapping.romeo.phase_linearity"), ("romeo_mag_coherence", "field_mapping.romeo.mag_coherence"),
    ("romeo_mag_weight", "field_mapping.romeo.mag_weight"), ("romeo_mag_weight2", "field_mapping.romeo.mag_weight2"),
    ("romeo_bestpath", "field_mapping.romeo.bestpath"),
    ("romeo_temporal_uncertain", "field_mapping.romeo.temporal_uncertain_unwrapping"),
    ("romeo_temporal_uncertain_unwrapping", "field_mapping.romeo.temporal_uncertain_unwrapping"),
    ("romeo_max_seeds", "field_mapping.romeo.max_seeds"), ("romeo_merge_regions", "field_mapping.romeo.merge_regions"),
    ("romeo_correct_regions", "field_mapping.romeo.correct_regions"), ("romeo_wrap_addition", "field_mapping.romeo.wrap_addition"),
    ("b0_estimation", "field_mapping.b0_estimation"), ("b0_weight_type", "field_mapping.b0_weight_type"),
    ("b0_weight_assumed_t2star_ms", "field_mapping.b0_weight_assumed_t2star_ms"),
    ("linear_fit_estimate_offset", "field_mapping.linear_fit.estimate_offset"),
    ("linear_fit_reliability", "field_mapping.linear_fit.reliability_threshold_percentile"),
    ("linear_fit_reliability_threshold", "field_mapping.linear_fit.reliability_threshold_percentile"),
    ("robust_fit_loss", "field_mapping.robust_fit.loss"), ("robust_fit_tuning", "field_mapping.robust_fit.tuning"),
    ("robust_fit_iterations", "field_mapping.robust_fit.iterations"),
    ("robust_fit_downweight_level", "field_mapping.robust_fit.downweight_level"),
    ("robust_fit_flag_ratio", "field_mapping.robust_fit.flag_ratio"),
    ("bf_algorithm", "bg_removal.algorithm"), ("vsharp_deconvolve", "bg_removal.vsharp.deconvolve"),
    ("vsharp_threshold", "bg_removal.vsharp.threshold"), ("vsharp_max_radius", "bg_removal.vsharp.max_radius"),
    ("vsharp_min_radius", "bg_removal.vsharp.min_radius"), ("vsharp_grid", "bg_removal.vsharp.grid"),
    ("vsharp_grid_spacing", "bg_removal.vsharp.grid_spacing"), ("pdf_tol", "bg_removal.pdf.tol"), ("lbv_tol", "bg_removal.lbv.tol"),
    ("ismv_tol", "bg_removal.ismv.tol"), ("ismv_max_iter", "bg_removal.ismv.max_iter"), ("ismv_radius", "bg_removal.ismv.radius"),
    ("sharp_threshold", "bg_removal.sharp.threshold"), ("sharp_radius", "bg_removal.sharp.radius"),
    ("resharp_radius", "bg_removal.resharp.radius"), ("resharp_tik_reg", "bg_removal.resharp.tik_reg"),
    ("resharp_tol", "bg_removal.resharp.tol"), ("resharp_max_iter", "bg_removal.resharp.max_iter"),
    ("harperella_radius", "bg_removal.harperella.radius"), ("harperella_max_iter", "bg_removal.harperella.max_iter"),
    ("harperella_tol", "bg_removal.harperella.tol"), ("iharperella_max_iter", "bg_removal.iharperella.max_iter"),
    ("iharperella_tol", "bg_removal.iharperella.tol"),
    ("qsm_algorithm", "inversion.algorithm"),
    ("rts_delta", "inversion.rts.delta"), ("rts_mu", "inversion.rts.mu"), ("rts_rho", "inversion.rts.rho"),
    ("rts_tol", "inversion.rts.tol"), ("rts_max_iter", "inversion.rts.max_iter"), ("rts_lsmr_iter", "inversion.rts.lsmr_iter"),
    ("tv_lambda", "inversion.tv.lambda"), ("tv_rho", "inversion.tv.rho"), ("tv_tol", "inversion.tv.tol"),
    ("tv_max_iter", "inversion.tv.max_iter"), ("tkd_threshold", "inversion.tkd.threshold"),
    ("tsvd_threshold", "inversion.tsvd.threshold"), ("tgv_iterations", "inversion.tgv.iterations"),
    ("tgv_erosions", "inversion.tgv.erosions"), ("tgv_alpha1", "inversion.tgv.alpha1"), ("tgv_alpha0", "inversion.tgv.alpha0"),
    ("tgv_step_size", "inversion.tgv.step_size"),
    ("tikhonov_lambda", "inversion.tikhonov.lambda"), ("nltv_lambda", "inversion.nltv.lambda"), ("nltv_mu", "inversion.nltv.mu"),
    ("nltv_tol", "inversion.nltv.tol"), ("nltv_max_iter", "inversion.nltv.max_iter"), ("nltv_newton_iter", "inversion.nltv.max_iter_newton"),
    ("medi_smv", "inversion.medi.smv"), ("medi_smv_radius", "inversion.medi.smv_radius"),
    ("medi_smv_shrink_mask", "inversion.medi.smv_shrink_mask"),
    ("medi_msmv", "inversion.medi.msmv"), ("medi_msmv_maxk", "inversion.medi.msmv_maxk"),
    ("medi_msmv_vessel_radius", "inversion.medi.msmv_vessel_radius"), ("medi_msmv_prefilter", "inversion.medi.msmv_prefilter"), ("medi_lambda", "inversion.medi.lambda"),
    ("medi_percentage", "inversion.medi.percentage"), ("medi_max_iter", "inversion.medi.max_iter"),
    ("medi_cg_max_iter", "inversion.medi.cg_max_iter"), ("medi_cg_tol", "inversion.medi.cg_tol"), ("medi_tol", "inversion.medi.tol"),
    ("medi_merit", "inversion.medi.merit"), ("medi_data_weighting", "inversion.medi.data_weighting"),
    ("tfi_lambda", "inversion.tfi.lambda"), ("tfi_precond", "inversion.tfi.precond"), ("tfi_percentage", "inversion.tfi.percentage"),
    ("tfi_max_iter", "inversion.tfi.max_iter"), ("tfi_cg_max_iter", "inversion.tfi.cg_max_iter"), ("tfi_cg_tol", "inversion.tfi.cg_tol"),
    ("tfi_tol", "inversion.tfi.tol"), ("tfi_data_weighting", "inversion.tfi.data_weighting"),
    ("ilsqr_tol", "inversion.ilsqr.tol"), ("ilsqr_max_iter", "inversion.ilsqr.max_iter"), ("ilsqr_pad_mm", "inversion.ilsqr.pad_mm"),
    ("lsqr_residual_weighting", "inversion.lsqr.residual_weighting"), ("lsqr_fit_global_offset", "inversion.lsqr.fit_global_offset"),
    ("lsqr_tol", "inversion.lsqr.tol"), ("lsqr_max_iter", "inversion.lsqr.max_iter"),
    ("heidi_cone_threshold", "inversion.heidi.cone_threshold"), ("heidi_gradient_threshold", "inversion.heidi.gradient_threshold"),
    ("heidi_apply_laplacian_correction", "inversion.heidi.apply_laplacian_correction"),
    ("heidi_laplacian_threshold", "inversion.heidi.laplacian_threshold"), ("heidi_gradient_mask_floor", "inversion.heidi.gradient_mask_floor"),
    ("heidi_continuation_steps", "inversion.heidi.continuation_steps"), ("heidi_inner_iterations", "inversion.heidi.inner_iterations"),
    ("heidi_mu_min", "inversion.heidi.mu_min"), ("heidi_tol", "inversion.heidi.tol"), ("heidi_denoise", "inversion.heidi.denoise"),
    ("heidi_denoise_iterations", "inversion.heidi.denoise_iterations"), ("heidi_denoise_time_step", "inversion.heidi.denoise_time_step"),
    ("heidi_denoise_conductance", "inversion.heidi.denoise_conductance"),
    ("qsmart_inversion", "inversion.qsmart.inversion"), ("qsmart_ilsqr_max_iter", "inversion.qsmart.ilsqr_max_iter"),
    ("qsmart_ilsqr_pad_mm", "inversion.qsmart.ilsqr_pad_mm"), ("qsmart_sdf_sigma1_stage1", "inversion.qsmart.sdf_sigma1_stage1"),
    ("qsmart_sdf_sigma2_stage1", "inversion.qsmart.sdf_sigma2_stage1"), ("qsmart_sdf_sigma1_stage2", "inversion.qsmart.sdf_sigma1_stage2"),
    ("qsmart_sdf_sigma2_stage2", "inversion.qsmart.sdf_sigma2_stage2"),
    ("qsmart_sdf_lower_lim", "inversion.qsmart.sdf_lower_lim"), ("qsmart_sdf_curv_constant", "inversion.qsmart.sdf_curv_constant"),
    ("qsmart_vasc_sphere_radius", "inversion.qsmart.vasc_sphere_radius"), ("qsmart_frangi_scale_min", "inversion.qsmart.frangi_scale_min"),
    ("qsmart_frangi_scale_max", "inversion.qsmart.frangi_scale_max"), ("qsmart_frangi_scale_ratio", "inversion.qsmart.frangi_scale_ratio"),
    ("qsmart_frangi_c", "inversion.qsmart.frangi_c"),
    ("qsmart_vessel_magnitude", "inversion.qsmart.vessel_magnitude"), ("qsmart_vessel_units", "inversion.qsmart.vessel_size_units"),
    ("qsmart_fit_threshold", "field_mapping.qsmart_fit_threshold"),
    ("ndi_tau", "inversion.ndi.tau"), ("ndi_alpha", "inversion.ndi.alpha"), ("ndi_max_iter", "inversion.ndi.max_iter"),
    ("ndi_auto_stop", "inversion.ndi.auto_stop"),
    ("fansi_alpha1", "inversion.fansi.alpha1"), ("fansi_mu1", "inversion.fansi.mu1"), ("fansi_mu2", "inversion.fansi.mu2"),
    ("fansi_alpha0", "inversion.fansi.alpha0"), ("fansi_mu0", "inversion.fansi.mu0"), ("fansi_max_iter", "inversion.fansi.max_iter"),
    ("fansi_tol_update", "inversion.fansi.tol_update"), ("fansi_tol_delta", "inversion.fansi.tol_delta"),
    ("l1qsm_alpha1", "inversion.l1qsm.alpha1"), ("l1qsm_mu1", "inversion.l1qsm.mu1"), ("l1qsm_mu2", "inversion.l1qsm.mu2"),
    ("l1qsm_mu3", "inversion.l1qsm.mu3"), ("l1qsm_lambda", "inversion.l1qsm.lambda"), ("l1qsm_max_iter", "inversion.l1qsm.max_iter"),
    ("l1qsm_tol_update", "inversion.l1qsm.tol_update"), ("l1qsm_tol_delta", "inversion.l1qsm.tol_delta"),
    ("whqsm_alpha1", "inversion.whqsm.alpha1"), ("whqsm_mu1", "inversion.whqsm.mu1"), ("whqsm_mu2", "inversion.whqsm.mu2"),
    ("whqsm_beta", "inversion.whqsm.beta"), ("whqsm_muh", "inversion.whqsm.muh"), ("whqsm_max_iter", "inversion.whqsm.max_iter"),
    ("whqsm_tol_update", "inversion.whqsm.tol_update"), ("whqsm_tol_delta", "inversion.whqsm.tol_delta"),
    ("hdqsm_alpha_l2", "inversion.hdqsm.alpha_l2"), ("hdqsm_mu1_l2", "inversion.hdqsm.mu1_l2"), ("hdqsm_mu2", "inversion.hdqsm.mu2"),
    ("hdqsm_max_iter_l1", "inversion.hdqsm.max_iter_l1"), ("hdqsm_max_iter_l2", "inversion.hdqsm.max_iter_l2"),
    ("hdqsm_tol_update", "inversion.hdqsm.tol_update"),
    ("amp_pe_wave_order", "inversion.amp_pe.wave_order"), ("amp_pe_nlevel", "inversion.amp_pe.nlevel"),
    ("amp_pe_wave_pec", "inversion.amp_pe.wave_pec"), ("amp_pe_simulated_te", "inversion.amp_pe.simulated_te"),
    ("amp_pe_max_linearization_ite", "inversion.amp_pe.max_linearization_ite"), ("amp_pe_tikhonov_beta", "inversion.amp_pe.tikhonov_beta"),
    ("amp_pe_damp_rate_sig", "inversion.amp_pe.damp_rate_sig"), ("amp_pe_damp_rate_par", "inversion.amp_pe.damp_rate_par"),
    ("amp_pe_max_pe_spar_ite", "inversion.amp_pe.max_pe_spar_ite"), ("amp_pe_max_pe_est_ite", "inversion.amp_pe.max_pe_est_ite"),
    ("amp_pe_cvg_thd", "inversion.amp_pe.cvg_thd"),
    ("dl_tiling", "inversion.tile_size"), ("dl_tile_size", "inversion.tile_size"), ("dl_tile_halo", "inversion.tile_halo"),
    ("orientation_algorithm", "multi_orientation.algorithm"), ("orientation_lambda", "multi_orientation.lambda"),
    ("cosmos_magnitude_weighting", "multi_orientation.magnitude_weighting"), ("cosmos_tol", "multi_orientation.cosmos_tol"),
    ("cosmos_max_iter", "multi_orientation.cosmos_max_iter"), ("orientation_registration", "multi_orientation.register"),
    ("multi_orientation_force", "multi_orientation.force"), ("qsm_reference", "qsm.reference"),
    ("do_chi_separation", "pipeline.do_chi_separation"), ("separation_algorithm", "separation.algorithm"),
    ("sep_r2starqsm_r_const_3t", "separation.r2star_qsm.r_const_3t"), ("sep_decompose_n_inner", "separation.decompose.n_inner"),
    ("sep_decompose_chi_bound", "separation.decompose.chi_bound"), ("sep_decompose_max_lm_iter", "separation.decompose.max_lm_iter"),
    ("sep_ilsqr_dr_pos", "separation.chi_sep_ilsqr.dr_pos"), ("sep_ilsqr_dr_neg", "separation.chi_sep_ilsqr.dr_neg"),
    ("sep_ilsqr_lambda1", "separation.chi_sep_ilsqr.lambda1"), ("sep_ilsqr_percentage", "separation.chi_sep_ilsqr.percentage"),
    ("sep_ilsqr_r2p_min", "separation.chi_sep_ilsqr.r2p_min"), ("sep_ilsqr_r2p_max", "separation.chi_sep_ilsqr.r2p_max"),
    ("sep_ilsqr_max_iter", "separation.chi_sep_ilsqr.max_iter"), ("sep_ilsqr_tol", "separation.chi_sep_ilsqr.tol"),
    ("sep_ilsqr_cg_max_iter", "separation.chi_sep_ilsqr.cg_max_iter"), ("sep_ilsqr_cg_tol", "separation.chi_sep_ilsqr.cg_tol"),
    ("sep_medi_lambda_para", "separation.chi_sep_medi.lambda_para"), ("sep_medi_lambda_dia", "separation.chi_sep_medi.lambda_dia"),
    ("sep_medi_lambda_cpl", "separation.chi_sep_medi.lambda_cpl"), ("sep_medi_dr_pos", "separation.chi_sep_medi.dr_pos"),
    ("sep_medi_dr_neg", "separation.chi_sep_medi.dr_neg"), ("sep_medi_percentage", "separation.chi_sep_medi.percentage"),
    ("sep_medi_cg_tol", "separation.chi_sep_medi.cg_tol"), ("sep_medi_cg_max_iter", "separation.chi_sep_medi.cg_max_iter"),
    ("sep_medi_max_iter", "separation.chi_sep_medi.max_iter"), ("sep_medi_tol", "separation.chi_sep_medi.tol"),
    ("sep_wavesep_dr_pos", "separation.wavesep.dr_pos"), ("sep_wavesep_dr_neg", "separation.wavesep.dr_neg"),
    ("sep_wavesep_alpha", "separation.wavesep.alpha"), ("sep_wavesep_lambda", "separation.wavesep.lambda"),
    ("sep_wavesep_wavelet_order", "separation.wavesep.wavelet_order"), ("sep_wavesep_max_iter", "separation.wavesep.max_iter"),
    ("sep_wavesep_tol", "separation.wavesep.tol"), ("sep_hcchisep_dr_pos_3t", "separation.hc_chisep.dr_pos_3t"),
    ("sep_hcchisep_bin_hz", "separation.hc_chisep.bin_hz"), ("sep_chisepnet_patch", "separation.chi_sepnet.patch"), ("sep_susepnet_patch", "separation.susep_net.patch"), ("custom_qsm_tool", "separation.custom_qsm_tool"),
    ("do_swi", "pipeline.do_swi"), ("swi_unwrap", "swi.unwrap"), ("swi_scaling", "swi.scaling"), ("swi_strength", "swi.strength"),
    ("swi_hp_sigma", "swi.hp_sigma"), ("swi_mip_window", "swi.mip_window"), ("do_smwi", "pipeline.do_smwi"),
    ("smwi_threshold", "smwi.threshold_ppm"), ("smwi_power", "smwi.power"), ("smwi_mip_window", "smwi.mip_window"),
    ("do_t2starmap", "pipeline.do_t2starmap"), ("do_r2starmap", "pipeline.do_r2starmap"), ("do_r2map", "pipeline.do_r2map"),
    ("custom_r2_tool", "separation.custom_r2_tool"), ("do_r2primemap", "pipeline.do_r2primemap"),
    ("custom_r2prime_tool", "separation.custom_r2prime_tool"), ("r2prime_strategy", "separation.r2prime_strategy"),
    ("r2primenet_ignore_field_mismatch", "separation.r2primenet.ignore_field_mismatch"), ("r2primenet_patch", "separation.r2primenet.patch"),
    ("do_segmentation", "pipeline.do_segmentation"), ("segmentation_source", "segmentation.custom_dseg_tool"),
    ("custom_dseg_tool", "segmentation.custom_dseg_tool"), ("synthseg_version", "segmentation.version"),
    ("synthseg_crop", "segmentation.crop"), ("synthseg_patch", "segmentation.patch"), ("synthseg_flip_averaging", "segmentation.flip_averaging"),
    ("synthseg_topology_cleanup", "segmentation.topology_cleanup"), ("synthseg_smoothing", "segmentation.sigma_smoothing"),
    ("synthseg_sigma", "segmentation.sigma_smoothing"), ("do_analysis", "pipeline.do_analysis"), ("export_dicom", "pipeline.export_dicom"),
];

/// Rows that edit no config setting of their own.
const NOT_CONFIG: &[&str] = &[
    // Expands into the mask recipe (masking.sections and friends), which the recipe rows edit.
    "mask_preset",
    // Run options of `qsmxt run`, not settings of the config.
    "source_dicom", "dicom_outputs", "execution_mode", "dry_run", "n_procs", "mem_limit_gb", "slurm_account",
    "slurm_partition", "slurm_time", "slurm_mem", "slurm_cpus", "slurm_submit", "debug", "force", "clean_intermediates",
];

fn path_of(field: &str) -> Option<&'static str> {
    FIELD_PATHS.iter().find(|(f, _)| *f == field).map(|(_, p)| *p)
}

/// The config settings a row edits (none for a note, a header that edits nothing, the run button).
fn row_paths(app: &App, row: &PipelineRow) -> Vec<&'static str> {
    use PipelineRow::*;
    let recipe = |section: usize| {
        if section == MASK_COMBINED_SECTION { "masking.refinements" }
        else if is_two_pass_section(section) { "masking.two_pass_sections" }
        else { "masking.sections" }
    };
    match row {
        AlgoSelect { field, .. } | Param { field, .. } | Toggle { field, .. } => {
            if NOT_CONFIG.contains(field) {
                return vec![];
            }
            let path = path_of(field).unwrap_or_else(|| panic!("TUI row {field} is not mapped to a schema setting"));
            // The TUI's reference picker lists the regions with the two methods.
            if *field == "qsm_reference" && app.field_select(field) >= 2 {
                return vec![path, "qsm.reference_region"];
            }
            vec![path]
        }
        MaskOpInput { section } | MaskOpGenerator { section } | MaskOpGeneratorParam { section }
        | MaskOpThresholdValue { section } | MaskOpHdBetStep { section } | MaskOpBetVoxelScale { section }
        | MaskOpEntry { section, .. } | MaskOpSignalErodeParam { section, .. } | MaskOpAddStep { section } => vec![recipe(*section)],
        MaskOpAddSection { two_pass } => vec![if *two_pass { "masking.two_pass_sections" } else { "masking.sections" }],
        MaskCombineOp => vec!["masking.combine"],
        MaskCombinedHeader => vec!["masking.refinements"],
        MaskTwoPassHeader => vec!["masking.two_pass_sections"],
        Separator | Note { .. } | MaskSectionHeader { .. } | RunButton => vec![],
    }
}

fn section_of(page: Page) -> Option<Section> {
    Some(match page {
        Page::Geometry => Section::Geometry,
        Page::Masking => Section::Masking,
        Page::Motion => Section::Motion,
        Page::QsmOff | Page::Inversion => Section::Inversion,
        Page::FieldMap => Section::FieldMap,
        Page::Background => Section::Background,
        Page::MultiOrientation => Section::MultiOrientation,
        Page::Reference => Section::Reference,
        Page::Separation => Section::Separation,
        Page::SwiSmwi => Section::SwiSmwi,
        Page::Relaxometry => Section::Relaxometry,
        Page::Segmentation => Section::Segmentation,
        Page::Export => Section::Export,
        Page::Execution => return None,
    })
}

/// One change to a form.
#[derive(Clone, Debug)]
enum Act {
    Toggle(&'static str),
    Select(&'static str, usize),
    /// An orientation grouping on the Input tab.
    Group,
    /// Limit the run to this many echoes.
    Echoes(usize),
}

fn build(acts: &[Act]) -> App {
    let mut app = App::new();
    for a in acts {
        match a {
            Act::Toggle(f) => app.toggle_field(f),
            Act::Select(f, i) => app.set_field_select(f, *i),
            Act::Group => app.filter_state.orientation_group = "acq".into(),
            Act::Echoes(n) => app.filter_state.num_echoes = n.to_string(),
        }
    }
    app.sync_pipeline_mode();
    app
}

/// Every form reached from a fresh one by changing one thing at a time, as long as each change
/// shows a row not seen before: the changes that make each (shortest first). Worked out once.
fn explored() -> &'static [Vec<Act>] {
    static FORMS: std::sync::OnceLock<Vec<Vec<Act>>> = std::sync::OnceLock::new();
    FORMS.get_or_init(|| {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut tried: BTreeSet<String> = BTreeSet::new();
        let mut queue: std::collections::VecDeque<Vec<Act>> =
            [vec![], vec![Act::Group], vec![Act::Echoes(1)], vec![Act::Group, Act::Echoes(1)]].into();
        let mut out = Vec::new();
        while let Some(acts) = queue.pop_front() {
            let app = build(&acts);
            let pages = app.all_pages();
            let mut new = acts.is_empty();
            for (_, page, rows) in &pages {
                for r in rows {
                    let key = match r {
                        PipelineRow::AlgoSelect { field, .. } | PipelineRow::Param { field, .. }
                        | PipelineRow::Toggle { field, .. } => format!("{page:?}|{field}"),
                        other => format!("{page:?}|{other:?}"),
                    };
                    new |= seen.insert(key);
                }
            }
            if !new {
                continue;
            }
            for (_, _, rows) in &pages {
                for r in rows {
                    let next: Vec<Act> = match *r {
                        PipelineRow::AlgoSelect { field, options, .. } if !NOT_CONFIG.contains(&field) || field == "mask_preset" =>
                            (0..options.len()).map(|i| Act::Select(field, i)).collect(),
                        PipelineRow::Toggle { field, .. } if !NOT_CONFIG.contains(&field) => vec![Act::Toggle(field)],
                        _ => vec![],
                    };
                    for a in next {
                        let mut n = acts.clone();
                        n.push(a);
                        if tried.insert(format!("{n:?}")) {
                            queue.push_back(n);
                        }
                    }
                }
            }
            out.push(acts);
        }
        out
    })
}

fn explore() -> impl Iterator<Item = (&'static Vec<Act>, App)> {
    explored().iter().map(|acts| (acts, build(acts)))
}

fn facts(app: &App) -> ViewFacts {
    ViewFacts { num_echoes: app.filter_state.num_echoes.trim().parse().ok(), multi_coil: None }
}

/// The settings the TUI shows now, with the sections it shows each in.
fn tui_shown(app: &App) -> BTreeMap<&'static str, BTreeSet<Section>> {
    let mut out: BTreeMap<&'static str, BTreeSet<Section>> = BTreeMap::new();
    // The Input tab's grouping, always there (not a row).
    out.entry("multi_orientation.group_by").or_default().insert(Section::Input);
    for (_, page, rows) in app.all_pages() {
        for r in &rows {
            for p in row_paths(app, r) {
                let e = out.entry(p).or_default();
                e.extend(section_of(page));
            }
        }
    }
    out
}

fn by_path() -> BTreeMap<&'static str, Setting> {
    schema::schema().into_iter().map(|s| (s.path, s)).collect()
}

#[test]
fn every_form_shows_the_settings_the_engine_says_apply() {
    let schema = by_path();
    assert!(explored().len() > 100, "explored only {} forms", explored().len());
    let mut failures = Vec::new();
    for (acts, app) in explore() {
        let config = config_from_app(&app).expect("the form builds a config");
        let shown = tui_shown(&app);
        let engine: BTreeSet<&str> = schema::settings_view(&config, &facts(&app))
            .into_iter().filter(|v| v.visible).map(|v| v.path).collect();
        let tui: BTreeSet<&str> = shown.keys().copied().collect();
        let only_tui: Vec<_> = tui.difference(&engine).collect();
        let only_engine: Vec<_> = engine.difference(&tui).collect();
        if !only_tui.is_empty() || !only_engine.is_empty() {
            failures.push(format!("after {acts:?}:\n  TUI only: {only_tui:?}\n  engine only: {only_engine:?}"));
        }
        for (p, sections) in &shown {
            let s = &schema[p];
            for sec in sections {
                if *sec != s.section && !s.also_in.contains(sec) {
                    failures.push(format!("{p} shows on {sec:?}, the schema puts it on {:?} {:?}", s.section, s.also_in));
                }
            }
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "{} mismatches, first ones:\n{}", failures.len(), failures[..failures.len().min(8)].join("\n"));
}

/// Settings the schema words differently from their TUI row, and why.
const WORDED_DIFFERENTLY: &[(&str, &str)] = &[
    ("field_mapping.romeo.template", "the config counts echoes from 0, the TUI's row from 1"),
];

#[test]
fn every_setting_has_a_row_and_is_worded_as_the_tui_words_it() {
    let schema = by_path();
    // Every (label, help) each setting is shown with, anywhere.
    let mut worded: BTreeMap<&str, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut options: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for (_, app) in explore() {
        for (_, _, rows) in app.all_pages() {
            for r in &rows {
                let (field, label, help) = match r {
                    PipelineRow::Param { field, label, help } | PipelineRow::Toggle { field, label, help } => (*field, *label, help.to_string()),
                    PipelineRow::AlgoSelect { field, label, options: opts, help } => {
                        if let Some(p) = path_of(field) {
                            options.entry(p).or_insert_with(|| opts.iter().zip(help.iter())
                                .map(|(o, h)| (o.to_string(), h.to_string())).collect());
                        }
                        (*field, *label, String::new())
                    }
                    _ => continue,
                };
                if let Some(p) = path_of(field) {
                    worded.entry(p).or_default().insert((label.trim().to_string(), help));
                }
            }
        }
    }
    let mut failures = Vec::new();
    for (p, s) in &schema {
        if p.starts_with("masking.") && matches!(s.value_type, ValueType::MaskSections | ValueType::MaskOps)
            || *p == "masking.combine" || *p == "multi_orientation.group_by" || *p == "qsm.reference_region"
        {
            continue; // the mask builder, the Input tab and the reference picker: below
        }
        let Some(w) = worded.get(p) else {
            failures.push(format!("{p}: no TUI row shows it"));
            continue;
        };
        // The TUI's own help (the schema's `help` is worded for any front end).
        let main = (s.label.to_string(), s.tui_help.to_string());
        let value = s.tui_value.map(|(l, h)| (l.to_string(), h.to_string()));
        if !w.contains(&main) && !WORDED_DIFFERENTLY.iter().any(|(q, _)| q == p) {
            failures.push(format!("{p}: schema says {main:?}, the TUI {w:?}"));
        }
        if let Some(v) = value {
            if !w.contains(&v) {
                failures.push(format!("{p}: schema's value row says {v:?}, the TUI {w:?}"));
            }
        }
        let opts = s.value_type.options();
        if opts.is_empty() {
            continue;
        }
        let Some(tui) = options.get(p) else { continue };
        if *p == "qsm.reference" {
            // The TUI's picker: the two methods, then every region (the schema's region choice).
            let regions = schema["qsm.reference_region"].value_type.options();
            let picker: Vec<_> = opts.iter().take(2).chain(regions.iter()).collect();
            assert_eq!(picker.len(), tui.len(), "qsm.reference");
            for (o, (label, help)) in picker.iter().zip(tui) {
                if o.label != label || o.tui_help != *help {
                    failures.push(format!("{p}: option {} / {label}", o.label));
                }
            }
            continue;
        }
        if opts.len() != tui.len() {
            failures.push(format!("{p}: {} options, the TUI has {}", opts.len(), tui.len()));
            continue;
        }
        for (o, (label, help)) in opts.iter().zip(tui) {
            let help_matches = help.starts_with(o.tui_help.as_str()) && o.url.as_ref().is_none_or(|u| help.contains(u.as_str()));
            if o.label != label || !help_matches {
                failures.push(format!("{p}: option {:?} {:?} vs the TUI's {label:?} {help:?}", o.label, o.tui_help));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_tui_row_that_edits_the_config_is_a_setting() {
    let schema = by_path();
    for (field, path) in FIELD_PATHS {
        assert!(schema.contains_key(path), "{field} → {path} is not in the schema");
    }
    // And every row is mapped (row_paths panics on an unmapped one).
    for (_, app) in explore() {
        for (_, _, rows) in app.all_pages() {
            for r in &rows {
                row_paths(&app, r);
            }
        }
    }
}

#[test]
fn every_schema_option_is_a_qsmxt_run_option() {
    use clap::CommandFactory;
    let cli = crate::cli::Cli::command();
    let run = cli.get_subcommands().find(|c| c.get_name() == "run").expect("qsmxt run");
    let longs: BTreeSet<String> = run.get_arguments().filter_map(|a| a.get_long()).map(|l| format!("--{l}")).collect();
    for s in schema::schema() {
        if let Some(f) = s.cli_flag {
            assert!(longs.contains(f), "{}: {f} is not a qsmxt run option", s.path);
        }
    }
}

/// What [`schema::get`] reads at `path`, from a config already serialised.
fn value_at(config: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let (parent, key) = path.rsplit_once('.').unwrap_or(("", path));
    let obj = parent.split('.').filter(|k| !k.is_empty()).try_fold(config, |v, k| v.get(k))?;
    obj.as_object().map(|o| o.get(key).cloned().unwrap_or(serde_json::Value::Null))
}

/// The command the TUI prints runs what the TUI runs: parsed back into a config, it agrees with
/// the TUI's own config on every setting that applies. (Settings that do not apply are left out
/// of the command; they do not change the run.)
#[test]
fn the_printed_command_reproduces_the_tuis_run() {
    use clap::{CommandFactory, FromArgMatches};
    // Built once: building clap's command is most of the cost of a parse.
    let mut parser = crate::cli::Cli::command();
    parser.build();
    let settings = schema::schema();
    let mut failures = Vec::new();
    for (acts, app) in explore() {
        let config = config_from_app(&app).unwrap();
        let cmd = qsmxt_engine::generate_command(&config);
        let argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        let cli = parser.try_get_matches_from_mut(&argv).and_then(|m| crate::cli::Cli::from_arg_matches(&m))
            .unwrap_or_else(|e| panic!("{e}\n{cmd}"));
        let crate::cli::Command::Run(run) = cli.command else { panic!("expected run") };
        let mut rebuilt = crate::pipeline::config::PipelineConfig::default();
        crate::pipeline::config::apply_run_overrides(&mut rebuilt, &run.pipeline).unwrap();
        let f = facts(&app);
        let (mut a, mut b) = (schema::prune_hidden(&config, &f), schema::prune_hidden(&rebuilt, &f));
        // An unset reliable mask means the main mask without its hole filling: compare the masks.
        for c in [&mut a, &mut b] {
            if c.masking.two_pass {
                c.masking.two_pass_sections = Some(c.masking.resolved_two_pass_sections());
            }
        }
        // `schema::get` of each setting, from one serialisation of each config.
        let (a, b) = (serde_json::to_value(&a).unwrap(), serde_json::to_value(&b).unwrap());
        let differ: Vec<_> = settings.iter()
            .filter(|s| value_at(&a, s.path) != value_at(&b, s.path)).map(|s| s.path).collect();
        if !differ.is_empty() {
            failures.push(format!("{differ:?} after {acts:?}\n  {cmd}"));
        }
        // The two leaks this used to allow: the TUI no longer passes either while it is hidden.
        let fm = &config.field_mapping;
        if fm.b0_estimation != qsmxt_engine::B0Estimation::WeightedAvg {
            assert_eq!(fm.b0_weight_type, qsmxt_engine::B0WeightType::PhaseSNR, "after {acts:?}");
        }
        if fm.unwrapping_algorithm == qsmxt_engine::UnwrappingAlgorithm::Laplacian {
            assert!(fm.romeo.individual, "after {acts:?}");
        }
    }
    assert!(failures.is_empty(), "{} forms, e.g.:\n{}", failures.len(), failures[..failures.len().min(4)].join("\n"));
}

/// The mask editor's step types are the mask schema's, and the ones it offers as a section's
/// first step are the ones that start one.
#[test]
fn the_mask_editors_steps_are_the_mask_schemas() {
    let m = schema::mask_schema();
    let step = |label: &str| m.steps.iter().find(|s| s.label == label).unwrap_or_else(|| panic!("{label} is not a mask step"));
    for t in MASK_GENERATOR_TYPES {
        assert!(step(t).starts_section, "{t}");
        // The default the editor adds is the schema's.
        let op = PipelineFormState::default_mask_op(t).unwrap();
        assert_eq!(serde_json::to_value(&op).unwrap(), step(t).default, "{t}");
    }
    for t in MASK_OP_TYPES {
        step(t);
    }
    for t in PipelineFormState::default().available_op_types(0) {
        assert!(!step(t).starts_section, "{t} is offered as a refinement");
    }
    // Every step type the schema has, the editor can make.
    for s in &m.steps {
        assert!(MASK_GENERATOR_TYPES.contains(&s.label) || MASK_OP_TYPES.contains(&s.label), "{}", s.label);
    }
    // Every input the schema has, the editor offers, in its order.
    let inputs: Vec<_> = m.inputs.iter().map(|i| i.value).collect();
    assert_eq!(inputs, PipelineFormState::MASK_INPUT_SOURCES);
}
