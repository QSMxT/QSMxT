use crate::config::*;
use crate::enums::*;
use crate::masking::*;
use crate::plan::{Pass, Plan, Stage};
use crate::record::{Choice, DatasetRecord, RunRecord, Topic};
use serde::{Deserialize, Serialize};

mod citations;
use citations::*;

/// PRELUDE's settings that differ from their defaults, as prose ("6 phase partitions").
fn prelude_changes(fm: &FieldMappingConfig, d: &FieldMappingConfig) -> Changed {
    let (p, pd) = (&fm.prelude, &d.prelude);
    let mut changed = Changed::default();
    changed.add(p.num_phase_partitions != pd.num_phase_partitions,
        || format!("{} phase partition{}", p.num_phase_partitions, if p.num_phase_partitions == 1 { "" } else { "s" }));
    changed.add(p.labelling != pd.labelling, || match p.labelling {
        PreludeLabelling::Auto => "regions labelled slice by slice above 64 voxels in-plane".to_string(),
        PreludeLabelling::Slices => "regions labelled slice by slice".to_string(),
        PreludeLabelling::Volume => "regions labelled over the whole volume".to_string(),
    });
    changed
}

/// The voxels a susceptibility map is defined on (`referencing::defined_support` in the main
/// crate), named once in full and as "the reconstruction mask" after that.
const RECON_MASK: &str =
    "the reconstruction mask (the brain mask less any rim eroded by background field removal)";
const RECON_MASK_AGAIN: &str = "the reconstruction mask";

// ─── Settings that differ from the defaults ───
//
// A methods sentence names its method and cites it. A parameter value is stated only when the
// run's setting differs from QSMxT's default for it: the defaults are what the citation and the
// QSMxT version already pin down, and listing them buries the choices the user actually made.

/// Whether two floats differ by more than rounding.
fn differs(a: f64, b: f64) -> bool {
    (a - b).abs() > 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// The non-default settings of one sentence, as prose fragments ("deconvolution enabled").
#[derive(Default)]
struct Changed(Vec<String>);

impl Changed {
    /// Add `text` when `differs` holds.
    fn add(&mut self, differs: bool, text: impl FnOnce() -> String) {
        if differs {
            self.0.push(text());
        }
    }

    /// Add every field of `cfg` that differs from `default` (see [`changed_fields`]).
    fn add_fields<T: serde::Serialize>(&mut self, cfg: &T, default: &T, skip: &[&str]) {
        self.0.extend(changed_fields(cfg, default, skip));
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `" with a, b and c"`, or nothing.
    fn with(&self) -> String {
        self.joined(" with ")
    }

    /// `", with a, b and c"`, or nothing — for the end of a sentence that already has commas.
    fn comma_with(&self) -> String {
        self.joined(", with ")
    }

    /// `"; a, b and c"`, or nothing — for inside a citation's parentheses.
    fn semi(&self) -> String {
        self.joined("; ")
    }

    /// `" (a, b and c)"`, or nothing.
    fn paren(&self) -> String {
        if self.0.is_empty() { String::new() } else { format!(" ({})", join_list(&self.0)) }
    }

    fn joined(&self, lead: &str) -> String {
        if self.0.is_empty() { String::new() } else { format!("{lead}{}", join_list(&self.0)) }
    }
}

/// How a parameter reads in prose.
enum Label {
    /// `"λ = 0.001"`.
    Eq(&'static str),
    /// `"λ = 0.001"` plus a unit: `"radius = 5 mm"`.
    Unit(&'static str, &'static str),
    /// `"500 iterations"`.
    Count(&'static str),
    /// `"at most 500 iterations"`.
    AtMost(&'static str),
    /// `"merit enabled"` / `"merit disabled"`.
    Toggle(&'static str),
}

/// The prose label of an algorithm parameter, by its config field name. Anything not listed reads
/// as its field name with spaces, which is still unambiguous next to the algorithm it belongs to.
fn param_label(field: &str) -> Label {
    use Label::*;
    match field {
        "tol" | "tolerance" => Eq("tolerance"),
        "cg_tol" => Eq("conjugate-gradient tolerance"),
        "tol_update" => Eq("update tolerance"),
        "tol_delta" => Eq("convergence tolerance"),
        "max_iter" | "max_iterations" => AtMost("iterations"),
        "cg_max_iter" => AtMost("conjugate-gradient iterations"),
        "max_iter_l1" => AtMost("L1 iterations"),
        "max_iter_l2" => AtMost("L2 iterations"),
        "lsmr_iter" => Count("LSMR iterations"),
        "max_iter_newton" => AtMost("Newton iterations"),
        "n_cg" => AtMost("conjugate-gradient iterations"),
        "iterations" => Count("iterations"),
        "erosions" => Count("mask erosions"),
        "continuation_steps" => Count("continuation steps"),
        "inner_iterations" => Count("inner iterations"),
        "denoise_iterations" => Count("denoising iterations"),
        "max_evaluations" => AtMost("evaluations"),
        "max_samples" => AtMost("samples"),
        "levels" => Count("pyramid levels"),
        "chain_levels" => Count("pyramid levels for chained registrations"),
        "chain" => Toggle("chained registration"),
        "lambda" => Eq("λ"),
        "lambda1" => Eq("λ1"),
        "lambda_csf" => Eq("λ (CSF)"),
        "cg_max_iter_lsqr" => AtMost("LSQR conjugate-gradient iterations"),
        "cg_tol_lsqr" => Eq("LSQR conjugate-gradient tolerance"),
        "cg_max_iter_sa" => AtMost("streaking-removal conjugate-gradient iterations"),
        "cg_tol_sa" => Eq("streaking-removal conjugate-gradient tolerance"),
        "d2_thr" => Eq("streaking cone threshold"),
        "prc_w_i_min" => Eq("lower Laplacian-weight percentile"),
        "prc_w_i_max" => Eq("upper Laplacian-weight percentile"),
        "prc_w_g_min" => Eq("lower gradient-weight percentile"),
        "prc_w_g_max" => Eq("upper gradient-weight percentile"),
        "unwrap_phase" => Toggle("echo-phase unwrapping"),
        "max_function_evaluations" => AtMost("function evaluations"),
        "function_tolerance" => Eq("function tolerance"),
        "step_tolerance" => Eq("step tolerance"),
        "optimality_tolerance" => Eq("optimality tolerance"),
        "finite_difference_step" => Eq("finite-difference step"),
        "typical_x" => Eq("typical x"),
        "n_inner" => Count("alternating passes"),
        "chi_bound" => Unit("χ bound", "ppm"),
        "alpha" => Eq("α"),
        "alpha0" => Eq("α0"),
        "alpha1" => Eq("α1"),
        "alpha_l2" => Eq("α (L2)"),
        "beta" => Eq("β"),
        "delta" => Eq("δ"),
        "mu" => Eq("μ"),
        "mu0" => Eq("μ0"),
        "mu1" => Eq("μ1"),
        "mu2" => Eq("μ2"),
        "mu3" => Eq("μ3"),
        "mu1_l2" => Eq("μ1 (L2)"),
        "muh" => Eq("μ (harmonic)"),
        "mu_min" => Eq("minimum μ"),
        "rho" => Eq("ρ"),
        "tau" => Eq("τ"),
        "gamma" => Eq("γ (ρ increase factor)"),
        "tol_newton" => Eq("Newton tolerance"),
        "weight" => Eq("data weight"),
        "space" => Eq("dipole kernel"),
        "n_pad" => Unit("zero-padding", "voxels"),
        "pad" => Unit("zero-padding", "voxels per side"),
        "threshold" => Eq("threshold"),
        "radius" | "smv_radius" => Unit("SMV radius", "mm"),
        "tik_reg" => Eq("Tikhonov weight"),
        "smv" => Toggle("SMV preprocessing"),
        "smv_shrink_mask" => Toggle("SMV mask shrinking"),
        "merit" => Toggle("MERIT"),
        "linear" => Toggle("the linear data term"),
        "zeropad" => Unit("zero-padding", "voxels at the end of each axis"),
        "reltol" => Eq("relative tolerance"),
        "percentage" => Eq("gradient-mask percentage"),
        "data_weighting" => Eq("data weighting mode"),
        "precond" => Eq("preconditioner weight"),
        "step_size" => Eq("step size"),
        "reg" => Eq("regularization operator"),
        "pad_mm" | "ilsqr_pad_mm" => Unit("zero-padding", "mm per side"),
        "dr_pos" => Eq("Dr+"),
        "dr_neg" => Eq("Dr−"),
        "dr_pos_3t" => Eq("Dr+ at 3 T"),
        "r_const_3t" => Eq("relaxometric constant at 3 T"),
        "sigma_mm" => Unit("σ", "mm"),
        "nbox" => Count("segmentation boxes per dimension"),
        "shrink_factor" => Eq("shrink factor"),
        "convergence_threshold" => Eq("convergence threshold"),
        "mesh" => Eq("initial B-spline mesh"),
        "spline_order" => Eq("B-spline order"),
        "bias_field_fwhm" => Eq("bias field FWHM"),
        "wiener_noise" => Eq("Wiener filter noise"),
        "histogram_bins" => Count("histogram bins"),
        "estimate_offset" => Toggle("phase-offset estimation"),
        "denoise" => Toggle("anisotropic-diffusion denoising"),
        "rotation_step_deg" => Unit("initial rotation step", "°"),
        "translation_step_mm" => Unit("initial translation step", "mm"),
        "min_overlap" => Eq("minimum overlap"),
        "crop" => Unit("centre crop", "voxels"),
        "topology_cleanup" => Toggle("topology cleanup"),
        "sigma_smoothing" => Unit("posterior smoothing σ", "voxels"),
        "patch" => Unit("patch", "voxels"),
        _ => Eq(""),
    }
}

/// A JSON value as prose: numbers as written, arrays as `a × b × c`, `null` as "automatic".
fn json_prose(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "automatic".to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(json_prose).collect::<Vec<_>>().join(" × "),
        serde_json::Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            n.as_f64().map_or_else(|| n.to_string(), |f| format!("{f}"))
        }
        other => other.to_string(),
    }
}

fn json_differs(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => differs(x, y),
        _ => a != b,
    }
}

/// The fields of `cfg` that differ from `default`, as prose (`"λ = 0.002"`, `"at most 50
/// iterations"`). Works on any serializable parameter struct, so every algorithm's settings are
/// covered without a sentence of its own; `skip` names fields the caller words itself.
fn changed_fields<T: serde::Serialize>(cfg: &T, default: &T, skip: &[&str]) -> Vec<String> {
    let (Ok(serde_json::Value::Object(c)), Ok(serde_json::Value::Object(d))) =
        (serde_json::to_value(cfg), serde_json::to_value(default))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in &c {
        if skip.contains(&k.as_str()) || d.get(k).is_some_and(|dv| !json_differs(v, dv)) {
            continue;
        }
        let value = json_prose(v);
        out.push(match param_label(k) {
            Label::Eq("") if v.is_boolean() => format!("{} {}", k.replace('_', " "),
                if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
            Label::Eq("") => format!("{} = {value}", k.replace('_', " ")),
            Label::Eq(l) => format!("{l} = {value}"),
            Label::Unit(l, "°") => format!("{l} = {value}°"),
            Label::Unit(l, u) => format!("{l} = {value} {u}"),
            Label::Count(noun) => format!("{value} {noun}"),
            Label::AtMost(noun) => format!("at most {value} {noun}"),
            Label::Toggle(l) => format!("{l} {}", if v.as_bool() == Some(true) { "enabled" } else { "disabled" }),
        });
    }
    out
}

/// The non-default settings of the selected dipole inversion. Deep-learning inversions have none
/// here (tiling is worded separately).
fn inversion_changes(config: &PipelineConfig, alg: QsmAlgorithm) -> Changed {
    let (i, d) = (&config.inversion, InversionConfig::default());
    let mut c = Changed::default();
    // The echo time the field is converted at is worded below, not as `te = …`.
    const TE: [&str; 1] = ["te"];
    match alg {
        QsmAlgorithm::Rts => c.add_fields(&i.rts, &d.rts, &[]),
        QsmAlgorithm::Tv => c.add_fields(&i.tv, &d.tv, &[]),
        QsmAlgorithm::Tkd => c.add_fields(&i.tkd, &d.tkd, &[]),
        QsmAlgorithm::Tsvd => c.add_fields(&i.tsvd, &d.tsvd, &[]),
        QsmAlgorithm::Tikhonov => c.add_fields(&i.tikhonov, &d.tikhonov, &[]),
        QsmAlgorithm::Nltv => c.add_fields(&i.nltv, &d.nltv, &TE),
        // mSMV's settings are worded with it (see `medi_msmv`).
        // The SMV and mSMV settings are worded with them (see `medi_smv` and `medi_msmv`); the
        // mask shrinking only applies to MEDI-SMV.
        QsmAlgorithm::Medi => {
            let smv_worded = i.medi.smv && !i.medi.msmv;
            let skip: Vec<&str> = MEDI_MSMV_FIELDS.iter().copied()
                .chain(if smv_worded { MEDI_SMV_FIELDS.as_slice() } else { &["smv_shrink_mask"] }.iter().copied())
                .chain(MEDI_CSF_FIELDS)
                .chain(TE)
                .collect();
            c.add_fields(&i.medi, &d.medi, &skip)
        }
        QsmAlgorithm::Tfi => c.add_fields(&i.tfi, &d.tfi, &TE),
        QsmAlgorithm::Ilsqr => c.add_fields(&i.ilsqr, &d.ilsqr, &[]),
        // STAR-QSM's result depends on the padding and on the echo time the field is converted
        // to phase at, so both are always given.
        QsmAlgorithm::Star => {
            let p = i.star.pad;
            c.0.push(format!("zero-padding of {} × {} × {} voxels per side", p[0], p[1], p[2]));
            c.0.push(format!("the field converted to phase at {}", echo_time_prose(i.star.te)));
        }
        QsmAlgorithm::Lsqr => c.add_fields(&i.lsqr, &d.lsqr, &[]),
        QsmAlgorithm::Ndi => {
            c.add_fields(&i.ndi, &d.ndi, &["te", "weight_norm"]);
            c.add(i.ndi.weight_norm != d.ndi.weight_norm, || match i.ndi.weight_norm {
                NdiWeightNorm::GlobalMax => "the magnitude weight normalised by the magnitude's maximum".to_string(),
                NdiWeightNorm::MaskP999 => "the magnitude weight normalised by the magnitude's 99.9th percentile within the brain mask".to_string(),
            });
        }
        QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv => c.add_fields(&i.fansi, &d.fansi, &TE),
        QsmAlgorithm::L1qsm => c.add_fields(&i.l1qsm, &d.l1qsm, &TE),
        QsmAlgorithm::Whqsm => c.add_fields(&i.whqsm, &d.whqsm, &TE),
        QsmAlgorithm::Hdqsm => c.add_fields(&i.hdqsm, &d.hdqsm, &[]),
        QsmAlgorithm::AmpPe => c.add_fields(&i.amp_pe, &d.amp_pe, &[]),
        QsmAlgorithm::Tgv => c.add_fields(&i.tgv, &d.tgv, &TE),
        _ => {}
    }
    // Stated only when it is not the method's own default (its reference code's). TGV's sentence
    // always says it.
    let default = PipelineConfig::default();
    let te = crate::stages::inversion::echo_time_choice(config, alg);
    if let Some(te) = te.filter(|&te| alg != QsmAlgorithm::Tgv && Some(te) != crate::stages::inversion::echo_time_choice(&default, alg)) {
        c.0.push(format!("the field converted to phase at {}", echo_time_prose(te)));
    }
    c
}

/// The echo time a method converts the field to phase at, as prose ("the sum of the echo times").
fn echo_time_prose(te: EchoTimeChoice) -> String {
    match te {
        EchoTimeChoice::First => "the first echo time".into(),
        EchoTimeChoice::Last => "the last echo time".into(),
        EchoTimeChoice::Spacing => "the echo spacing".into(),
        EchoTimeChoice::Sum => "the sum of the echo times".into(),
        EchoTimeChoice::Seconds(_) => format!("an echo time of {te} ms"),
    }
}

/// MEDI's mSMV settings, which [`medi_msmv`] words rather than the list of changed settings.
const MEDI_MSMV_FIELDS: [&str; 4] = ["msmv", "msmv_maxk", "msmv_vessel_radius", "msmv_prefilter"];

/// MEDI's SMV settings, which [`medi_smv`] words rather than the list of changed settings.
const MEDI_SMV_FIELDS: [&str; 3] = ["smv", "smv_radius", "smv_shrink_mask"];

/// MEDI-SMV, after MEDI's name and citation in the inversion's sentence: ", with the local field
/// high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that
/// radius"; nothing for MEDI without SMV, MEDI-mSMV ([`medi_msmv`]) or another inversion.
fn medi_smv(config: &PipelineConfig) -> String {
    let m = &config.inversion.medi;
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.smv || m.msmv {
        return String::new();
    }
    format!(", with the local field high-passed by a {} mm spherical mean value (SMV) filter {}",
        m.smv_radius,
        if m.smv_shrink_mask { "within the mask shrunk by that radius" } else { "over the full mask" })
}

/// MEDI+0's settings, which [`medi_csf`] words rather than the list of changed settings.
const MEDI_CSF_FIELDS: [&str; 4] = ["csf", "lambda_csf", "csf_r2s_threshold", "csf_erode"];

/// MEDI+0's sentence, after the inversion's: "MEDI's zero reference to the ventricular CSF
/// (MEDI+0; Liu et al., 2018) was applied.", with its changed settings, when the runs applied it.
/// QSM.rs runs MEDI without the term on fewer than three echoes (the CSF is found from R2*; the
/// plan warns), and each run records which it did ([`Topic::MediCsf`]): nothing is said when no
/// run applied it, and where the runs differ, or nothing is known of them, the configuration's
/// wording says the condition ("to runs of at least three echoes"). Nothing with the CSF term off
/// or another inversion.
fn describe_medi_csf(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let (m, d) = (&config.inversion.medi, MediConfig::default());
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.csf {
        return;
    }
    if ran.only(Topic::MediCsf, &[Choice::NotApplied]) {
        return;
    }
    let scope = if ran.only(Topic::MediCsf, &[Choice::Applied]) { "" } else { " to runs of at least three echoes" };
    doc.cite(&CITE_MEDI0);
    let mut changed = Changed::default();
    changed.add(differs(m.lambda_csf, d.lambda_csf), || format!("λ_CSF = {}", m.lambda_csf));
    changed.add(differs(m.csf_r2s_threshold, d.csf_r2s_threshold), || format!("CSF below R2* = {} Hz", m.csf_r2s_threshold));
    changed.add(m.csf_erode != d.csf_erode, || "without eroding the mask first".to_string());
    doc.say(&both(Stage::Inversion), format!(
        "MEDI's zero reference to the ventricular CSF (MEDI+0; {}{}) was applied{scope}.",
        cite_inline(&CITE_MEDI0), changed.semi()));
}

/// MEDI-mSMV, after MEDI's name and citation in the inversion's sentence: ", with maximum
/// spherical mean value (mSMV) filtering of the local field (Roberts et al., 2024)", and its
/// changed settings; nothing for plain MEDI or another inversion.
fn medi_msmv(config: &PipelineConfig, doc: &mut Draft) -> String {
    let (m, d) = (&config.inversion.medi, MediConfig::default());
    if config.inversion.algorithm != QsmAlgorithm::Medi || !m.msmv {
        return String::new();
    }
    doc.cite(&CITE_MSMV);
    let mut changed = Changed::default();
    changed.add(m.msmv_maxk != d.msmv_maxk, || format!("at most {} boundary-correction passes", m.msmv_maxk.saturating_sub(1)));
    changed.add(m.msmv_vessel_radius != d.msmv_vessel_radius, || match m.msmv_vessel_radius {
        Some(r) => format!("vessels up to {r} voxels thick"),
        None => "vessel thicknesses from the voxel size".to_string(),
    });
    changed.add(m.msmv_prefilter != d.msmv_prefilter, || format!(
        "the SMV prefilter {}", if m.msmv_prefilter { "enabled" } else { "disabled" }));
    format!(", with maximum spherical mean value (mSMV) filtering of the local field ({}{})",
        cite_inline(&CITE_MSMV), changed.semi())
}

/// QSMART's settings that differ from their defaults, its inner inversion and vessel magnitude
/// aside (said apart). The vessel sizes carry their units.
fn qsmart_changes(config: &PipelineConfig) -> Changed {
    let (q, d) = (&config.inversion.qsmart, QsmartConfig::default());
    let mut c = Changed::default();
    let mut skip = vec!["inversion", "vessel_magnitude", "vessel_size_units", "offset_grid", "frangi_c_mode", "frangi_c_factor"];
    skip.extend(QSMART_VESSEL_SIZE_KEYS);
    // A relative c replaces the absolute one, which then plays no part.
    let relative_c = q.frangi_c_mode == crate::enums::QsmartFrangiCMode::Relative;
    if relative_c {
        skip.push("frangi_c");
    }
    c.add_fields(q, &d, &skip);
    c.add(relative_c, || format!(
        "Frangi's c set to {} × the median of the vessel magnitude inside the brain mask", q.frangi_c_factor));
    c.add(q.offset_grid == crate::enums::QsmartOffsetGrid::QsmartReference,
        || "the stages' offset adjusted on the dipole grid of QSMART's adjust_offset.m".to_string());
    // In other units than the default, the sizes are said whatever they are: the default numbers
    // mean something else.
    let u = q.vessel_size_units;
    let other_units = u != d.vessel_size_units;
    c.add(other_units || q.vasc_sphere_radius != d.vasc_sphere_radius,
        || format!("a bottom-hat sphere radius of {} {u}", q.vasc_sphere_radius));
    c.add(other_units || differs(q.frangi_scale_min, d.frangi_scale_min) || differs(q.frangi_scale_max, d.frangi_scale_max)
            || differs(q.frangi_scale_ratio, d.frangi_scale_ratio),
        || format!("Frangi scales from {} to {} {u} in steps of {} {u}", q.frangi_scale_min, q.frangi_scale_max, q.frangi_scale_ratio));
    c
}

/// The non-default settings of the selected background-field remover. V-SHARP's are worded by
/// its own sentence.
fn bg_removal_changes(config: &PipelineConfig) -> Changed {
    let (b, d) = (&config.bg_removal, BgRemovalConfig::default());
    let mut c = Changed::default();
    match b.algorithm {
        BfAlgorithm::Pdf => c.add_fields(&b.pdf, &d.pdf, &[]),
        BfAlgorithm::Lbv => c.add_fields(&b.lbv, &d.lbv, &[]),
        BfAlgorithm::Ismv => {
            c.add(b.ismv.variant != d.ismv.variant, || match b.ismv.variant {
                IsmvVariant::Qsmjl => "QSM.jl's implementation".to_string(),
                IsmvVariant::Wen2014 => "the authors' implementation".to_string(),
            });
            c.add_fields(&b.ismv, &d.ismv, &["variant", "radius"]);
            // The radius is stated even at its default: it scales with the voxel size.
            c.add(true, || match (b.ismv.radius, b.ismv.variant) {
                (Some(r), _) => format!("SMV radius = {r} mm"),
                (None, IsmvVariant::Wen2014) => "an SMV radius of 6 × the smallest voxel dimension".to_string(),
                (None, IsmvVariant::Qsmjl) => "an SMV radius of 2 × the largest voxel dimension".to_string(),
            });
        }
        BfAlgorithm::Sharp => c.add_fields(&b.sharp, &d.sharp, &[]),
        BfAlgorithm::Resharp => c.add_fields(&b.resharp, &d.resharp, &[]),
        BfAlgorithm::Harperella => {
            c.add_fields(&b.harperella, &d.harperella, &["rim"]);
            c.add(b.harperella.rim != d.harperella.rim, || rim_prose(b.harperella.rim));
        }
        BfAlgorithm::Iharperella => {
            c.add_fields(&b.iharperella, &d.iharperella, &["rim"]);
            c.add(b.iharperella.rim != d.iharperella.rim, || rim_prose(b.iharperella.rim));
        }
        BfAlgorithm::Vsharp | BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => {}
    }
    c
}

/// Which Laplacian values in HARPERELLA's / iHARPERELLA's boundary rim were set to zero.
fn rim_prose(rim: HarperellaRim) -> String {
    match rim {
        HarperellaRim::Whole => "every Laplacian value in the brain's boundary rim set to zero".to_string(),
        HarperellaRim::Sti => "the 20 % largest Laplacian values in the brain's boundary rim set to zero, as in STI Suite".to_string(),
    }
}

/// The non-default rigid-registration settings, shared by motion correction and
/// multi-orientation co-registration.
fn registration_changes(config: &PipelineConfig) -> Changed {
    let mut c = Changed::default();
    c.add_fields(&config.registration, &crate::motion::RegistrationConfig::default(), &[]);
    c
}

/// The non-default settings of the selected susceptibility-source separation.
fn separation_changes(config: &PipelineConfig) -> Changed {
    let (s, d) = (&config.separation, SeparationConfig::default());
    let mut c = Changed::default();
    match s.algorithm {
        SeparationAlgorithm::R2starQsm => c.add_fields(&s.r2star_qsm, &d.r2star_qsm, &[]),
        SeparationAlgorithm::Decompose => {
            // Unwrapping and a χ bound from the echo spacing have sentences of their own.
            c.add_fields(&s.decompose, &d.decompose, &["lsqcurvefit", "unwrap_phase", "chi_bound", "fit"]);
            c.add(s.decompose.fit == crate::enums::DecomposeFit::Paper, || "the paper's fit (the \
                compartment amplitudes constrained to sum to one and R2*₀ fitted by linear least squares, \
                χ± by trust-region-reflective least squares, each voxel's magnitude scaled to its signal \
                at TE = 0, extrapolated from the first two echoes)".to_string());
            if let ChiBoundChoice::Ppm(x) = s.decompose.chi_bound {
                c.add(s.decompose.chi_bound != d.decompose.chi_bound, || format!("χ bound = {x} ppm"));
            }
            c.add_fields(&s.decompose.lsqcurvefit, &d.decompose.lsqcurvefit, &[]);
        }
        // Its QSM's iLSQR is worded with that QSM.
        SeparationAlgorithm::ChiSepIlsqr => c.add_fields(&s.chi_sep_ilsqr, &d.chi_sep_ilsqr, &["qsm_ilsqr"]),
        SeparationAlgorithm::ChiSepMedi => c.add_fields(&s.chi_sep_medi, &d.chi_sep_medi, &["qsm_ilsqr", "csf_r2star"]),
        SeparationAlgorithm::WaveSep => c.add_fields(&s.wavesep, &d.wavesep, &[]),
        SeparationAlgorithm::HcChisep => c.add_fields(&s.hc_chisep, &d.hc_chisep, &[]),
        SeparationAlgorithm::ChiSepNet => match s.chi_sepnet.route {
            ChiSepNetRoute::Toolbox => {
                c.add(true, || "the inputs resampled to 1 mm with B0 along z and the susceptibility \
                    channel computed by QSMnetME, as in the χ-separation toolbox, and the maps resampled back".to_string());
                if let Some([x, y, z]) = s.chi_sepnet.frame_patch {
                    c.add(true, || format!("the networks applied as a sliding window of {x}×{y}×{z}-voxel patches"));
                }
            }
            ChiSepNetRoute::Native => {
                c.add(true, || "the network applied on the acquired grid to this run's QSM".to_string());
                let [x, y, z] = s.chi_sepnet.patch;
                c.add(s.chi_sepnet.patch != d.chi_sepnet.patch, || format!("{x}×{y}×{z}-voxel sliding-window patches"));
            }
        },
        SeparationAlgorithm::SusepNet => {
            if let Some([x, y, z]) = s.susep_net.patch {
                c.add(true, || format!("the network applied as a sliding window of {x}×{y}×{z}-voxel patches"));
            }
        }
    }
    c
}

/// One sentence of the methods paragraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub text: String,
    /// The steps it describes, by the ids [`crate::pipeline_graph::pipeline_graph`] gives their
    /// boxes (`mask`, `invert-reliable`): several where one method does the work of several
    /// steps (TGV removes the background and inverts the field), none for a sentence about the
    /// run as a whole.
    pub steps: Vec<String>,
    /// The works it cites, by [`Reference::key`].
    pub citations: Vec<String>,
}

/// A work the methods text cites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    /// A short name for it (`dymerska2021`).
    pub key: String,
    /// The reference as the list gives it, in Markdown.
    pub text: String,
}

/// The methods text sentence by sentence, each with the steps it describes: what a front end
/// draws beside the pipeline graph, so that a box and the sentences about it can point at each
/// other. [`Methods::to_markdown`] is the text [`generate_methods_for`] writes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Methods {
    pub sentences: Vec<Sentence>,
    /// In the order the paragraph first cites them.
    pub references: Vec<Reference>,
    /// What the run departed from in its methods' reference pipelines, said before the references.
    #[serde(default, skip_serializing_if = "Note::is_empty")]
    pub note: Note,
}

/// The methods text's Note section: the departures from the chosen methods' recommendations and
/// reference pipelines that a run kept ([`crate::recommendations::note`]), one sentence each.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// From what the methods' authors recommend, or QSM.rs measured to be better (or a combination
    /// known to fail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub departures: Vec<String>,
    /// From what the methods' reference pipelines used, where their authors did not say it is
    /// needed or better.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
    /// The required changes the run made itself: outputs another setting needs, turned on because
    /// the configuration left them off ([`crate::config::PipelineConfig::required_changes`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

impl Note {
    pub fn is_empty(&self) -> bool {
        self.departures.is_empty() && self.differences.is_empty() && self.required.is_empty()
    }

    /// [`Self::required`] from `config`'s record of them, each a sentence.
    fn with_required(mut self, config: &crate::config::PipelineConfig) -> Self {
        self.required = config.required_changes.iter().map(|m| format!("{m}.")).collect();
        self
    }
}

impl Methods {
    /// The paragraph: the sentences, joined by spaces.
    pub fn paragraph(&self) -> String {
        self.sentences.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// The paragraph under a `# Methods` heading, then the Note, if there is one, under `## Note`,
    /// then the references under `## References`.
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# Methods\n\n{}\n\n", self.paragraph());
        let n = &self.note;
        if !n.is_empty() {
            out.push_str("## Note\n\n");
            for (heading, lines) in [
                ("Departures from the methods' recommendations:", &n.departures),
                ("Differences from the methods' reference pipelines:", &n.differences),
                ("Outputs the run turned on because other settings need them:", &n.required),
            ] {
                if lines.is_empty() {
                    continue;
                }
                out.push_str(&format!("{heading}\n\n"));
                for l in lines {
                    out.push_str(&format!("- {l}\n"));
                }
                out.push('\n');
            }
        }
        if !self.references.is_empty() {
            out.push_str("## References\n\n");
            for r in &self.references {
                out.push_str(&format!("- {}\n", r.text));
            }
        }
        out.truncate(out.trim_end().len());
        out.push('\n');
        out
    }
}

/// The methods text as it is written: each sentence with the steps it describes and the works
/// cited since the one before (a work is cited before the sentence that names it), and the
/// reference list.
#[derive(Default)]
struct Draft {
    sentences: Vec<(String, Vec<Stage>, Vec<&'static str>)>,
    references: Vec<&'static Citation>,
    cited: Vec<&'static str>,
}

impl Draft {
    /// Cite `cite` in the next sentence, and list it if it is not listed yet.
    fn cite(&mut self, cite: &'static Citation) {
        if !self.references.iter().any(|c| c.key == cite.key) {
            self.references.push(cite);
        }
        if !self.cited.contains(&cite.key) {
            self.cited.push(cite.key);
        }
    }

    /// A sentence about `steps`, citing what was cited since the last one.
    fn say(&mut self, steps: &[Stage], text: impl Into<String>) {
        self.sentences.push((text.into(), steps.to_vec(), std::mem::take(&mut self.cited)));
    }

    /// The steps each sentence names: those `plan` has and its graph draws, or none without a
    /// plan.
    fn finish(self, config: &PipelineConfig, plan: Option<&Plan>) -> Methods {
        self.finish_drawn(|stage| plan.is_some_and(|p| {
            p.has(*stage) && !crate::pipeline_graph::left_out(p, config, *stage, None)
        }))
    }

    /// The steps each sentence names: those `drawn` says the graph draws.
    fn finish_drawn(self, drawn: impl Fn(&Stage) -> bool) -> Methods {
        let sentences = self.sentences.into_iter().map(|(text, stages, cites)| {
            let mut steps: Vec<String> = Vec::new();
            for name in stages.iter().filter(|s| drawn(s)).map(Stage::name) {
                if !steps.contains(&name) {
                    steps.push(name);
                }
            }
            Sentence { text, steps, citations: cites.into_iter().map(str::to_string).collect() }
        }).collect();
        let references = self.references.iter()
            .map(|c| Reference { key: c.key.to_string(), text: c.text.to_string() })
            .collect();
        Methods { sentences, references, note: Note::default() }
    }
}

/// A step two-pass runs once per pass, in both passes.
fn both(step: fn(Pass) -> Stage) -> [Stage; 2] {
    [step(Pass::Main), step(Pass::Reliable)]
}

/// The steps that reconstruct the susceptibility map, in both passes: the ones that run in the
/// reconstruction box on the padded grid.
fn reconstruction() -> Vec<Stage> {
    [both(Stage::BackgroundRemoval), both(Stage::Inversion), both(Stage::Tgv), both(Stage::Qsmart)].concat()
}

/// The steps two-pass adds: the reliable-phase mask, the reconstruction on it, and the
/// combination of the two maps.
fn two_pass_steps() -> Vec<Stage> {
    vec![Stage::Mask(Pass::Reliable), Stage::BackgroundRemoval(Pass::Reliable), Stage::Inversion(Pass::Reliable),
         Stage::Tgv(Pass::Reliable), Stage::Qsmart(Pass::Reliable), Stage::TwoPassCombination]
}

/// Generate a methods description and citation list from a pipeline configuration.
/// Uses "qsmxt" as the tool name by default.
pub fn generate_methods(config: &PipelineConfig) -> String {
    generate_methods_for(config, "qsmxt")
}

/// DECOMPOSE's χ bound from the echo spacing: in ppm where every run that recorded it
/// ([`Topic::ChiBound`]) had the same, and otherwise as where it came from.
fn chi_bound_sentence(ran: Ran) -> String {
    let reasons: std::collections::BTreeSet<&str> = ran.0.and_then(|r| r.step("chi_separation"))
        .map(|s| s.decisions.iter().filter(|d| d.topic == Topic::ChiBound).map(|d| d.reason.as_str()).collect())
        .unwrap_or_default();
    match reasons.iter().next() {
        Some(one) if reasons.len() == 1 => one.to_string(),
        Some(_) => "The χ search was bounded from each scan's field strength and echo spacing.".to_string(),
        None => "The χ search was bounded from the field strength and echo spacing.".to_string(),
    }
}

/// Generate methods for a specific tool (e.g. "qsmxt" or "QSMbly"): [`Methods::to_markdown`].
pub fn generate_methods_for(config: &PipelineConfig, tool: &str) -> String {
    let config = &config.normalised().0;
    let mut methods = draft(config, tool, Ran::default()).finish(config, None);
    methods.note = crate::recommendations::config_note(config).with_required(config);
    methods.to_markdown()
}

/// The methods text for a run of `plan` (made by [`crate::plan::plan`] from `config` and the
/// scan's facts), sentence by sentence, each naming the steps of `plan`'s pipeline graph it
/// describes. Its [`Methods::to_markdown`] is [`generate_methods_for`]'s text.
pub fn methods_structured(config: &PipelineConfig, plan: &Plan, tool: &str) -> Methods {
    let config = &config.normalised_as_planned(plan);
    let mut methods = draft(config, tool, Ran::default()).finish(config, Some(plan));
    methods.note = crate::recommendations::config_note(config).with_required(config);
    methods
}

/// The methods text of a run of `plan` that did what `record` says ([`crate::run::run`]'s
/// record): it says what was done, as [`methods_structured`] does for the plan, where the run
/// decided something the configuration does not (a supplied mask used, or not found and computed
/// instead; R2′ measured rather than predicted; a step that had nothing to work on). A run that
/// decided nothing gets [`methods_structured`]'s text, less multi-orientation and DICOM export,
/// which are said only of a run whose record shows its group was combined or its maps exported.
pub fn methods_for_run(config: &PipelineConfig, plan: &Plan, record: &RunRecord, tool: &str) -> Methods {
    let config = &config.normalised_as_planned(plan);
    let merged = DatasetRecord::from(record);
    let mut methods = draft(config, tool, Ran(Some(&merged))).finish_drawn(|stage| {
        plan.has(*stage) && !crate::pipeline_graph::left_out(plan, config, *stage, record.resampled)
    });
    methods.note = crate::recommendations::note(&merged.recommendations, 1).with_required(config);
    methods
}

/// The methods text of a dataset's runs of `config`, merged into `record` ([`crate::record::merge`]):
/// a choice every run made is said as for one run; where the runs differ, the configuration's
/// wording ("where available") stands. Each sentence names the steps some run did something in.
pub fn methods_for_dataset(config: &PipelineConfig, record: &DatasetRecord, tool: &str) -> Methods {
    let mut config = config.normalised().0;
    // χ-sepnet's toolbox route: the R2′ maps the scans that could measure one made.
    if record.count(Topic::ChiSeparationR2prime, Choice::Measured) > 0 {
        config.pipeline.do_r2primemap = true;
        crate::config::enforce_separation_dependencies(&mut config);
    }
    let config = &config;
    let mut methods = draft(config, tool, Ran(Some(record)))
        .finish_drawn(|stage| record.step(&stage.name()).is_some_and(|s| s.ran() > 0));
    methods.note = crate::recommendations::note(&record.recommendations, record.runs).with_required(config);
    methods
}

/// Where χ-sepnet's toolbox route got its R2′: its own estimate unless it is given a measured one.
fn chisepnet_r2prime_sentences(doc: &mut Draft, config: &PipelineConfig, ran: Ran) {
    let (measured, estimated) = (ran.count(Topic::ChiSeparationR2prime, Choice::Measured),
                                 ran.count(Topic::ChiSeparationR2prime, Choice::Predicted));
    if measured > 0 && estimated == 0 {
        doc.say(&[Stage::ChiSeparation], "χ-separation used the measured R2'.".to_string());
        return;
    }
    doc.cite(&CITE_R2PRIMENET);
    let unknown = measured == 0 && estimated == 0;
    let always = config.separation.r2prime_strategy == R2PrimeStrategy::R2primenet;
    let estimate = "estimated from R2* with R2PRIMEnet (Kim et al., 2025) in the network's training frame";
    doc.say(&[Stage::ChiSeparation], if measured > 0 || (unknown && !always) {
        format!("χ-separation used R2' measured as R2* - R2 where a multi-echo spin-echo acquisition \
                 was available, and otherwise its own R2', {estimate}.")
    } else {
        format!("χ-separation used its own R2', {estimate}.")
    });
    // Only a map a run did make, and that was estimated: a measured one is what the route used.
    if config.pipeline.do_r2primemap && (unknown || ran.count(Topic::R2primeSource, Choice::Predicted) > 0) {
        doc.say(&[Stage::ChiSeparation], if measured > 0 || (unknown && !always) {
            "Where R2' was estimated, the R2' map in the outputs was not an input to χ-separation.".to_string()
        } else {
            "The R2' map in the outputs was not an input to χ-separation.".to_string()
        });
    }
    // The R2′ sentences say this where a map was made; the route's own estimate is off 3 T too.
    let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0 || unknown;
    if config.separation.r2primenet.ignore_field_mismatch && off_field && !config.pipeline.do_r2primemap {
        doc.say(&[Stage::ChiSeparation],
            "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data acquired at other \
             field strengths.".to_string());
    }
}

/// What the runs the text describes did ([`DatasetRecord`]; one run's is a dataset of one). With
/// nothing known, the text says what the configuration asks for.
#[derive(Clone, Copy, Default)]
struct Ran<'a>(Option<&'a DatasetRecord>);

impl Ran<'_> {
    /// Some run decided about `topic`, and every one that did chose one of `choices`.
    fn only(&self, topic: Topic, choices: &[Choice]) -> bool {
        self.0.is_some_and(|r| r.only(topic, choices))
    }

    /// The runs the text describes (none when nothing is known of them).
    fn runs(&self) -> usize {
        self.0.map_or(0, |r| r.runs)
    }

    /// The runs that chose `choice` about `topic`.
    fn count(&self, topic: Topic, choice: Choice) -> usize {
        self.0.map_or(0, |r| r.count(topic, choice))
    }

    /// The runs whose final maps were exported as DICOM series.
    fn dicom_exported(&self) -> usize {
        self.0.map_or(0, |r| r.dicom_exported)
    }

    /// The plan of some run had `stage`, and no run did anything in it.
    fn none_ran(&self, stage: Stage) -> bool {
        self.0.is_some_and(|r| r.none_ran(&stage.name()))
    }

    /// The runs that did something in `stage`.
    fn ran(&self, stage: Stage) -> usize {
        self.0.and_then(|r| r.step(&stage.name())).map_or(0, |s| s.ran())
    }
}

fn draft(config: &PipelineConfig, tool: &str, ran: Ran) -> Draft {
    let mut doc = Draft::default();

    let (cite, author_year) = match tool {
        "QSMbly" | "qsmbly" => (&CITE_QSMBLY, "Stewart, 2026"),
        _ => (&CITE_QSMXT, "Stewart, 2026"),
    };

    doc.cite(cite);
    if config.pipeline.do_qsm {
        doc.say(&[], format!("QSM processing was performed using {} ({}).", tool, author_year));
    } else {
        doc.say(&[], format!("MRI processing was performed using {} ({}).", tool, author_year));
    }
    // The toolbox pipeline the settings were chosen from, while its choices hold; the settings
    // below say where they depart from it.
    if let Some(p) = config.reference_pipeline.as_deref()
        .filter(|id| crate::pipelines::chosen(config, id))
        .and_then(qsm_core::pipeline::reference_pipelines::reference_pipeline) {
        doc.say(&[], format!("Processing followed the {} pipeline.", p.toolbox));
        // UK Biobank's brain mask comes from its T1-weighted images, which QSMxT does not use.
        if p.id == "uk-biobank" {
            doc.say(&[], "The brain mask was made from the gradient-echo images as described below, \
                in place of the UK Biobank pipeline's mask from its T1-weighted images.".to_string());
        }
    }

    // The k-space Tukey window comes before every other step.
    if config.pipeline.tukey_window > 0.0 {
        doc.say(&[Stage::TukeyWindow], format!(
            "Before any other processing, each echo's complex image was multiplied in k-space by a Tukey \
             window with a taper ratio of {}.", config.pipeline.tukey_window));
    }

    // Masking (inhomogeneity correction is described inline)
    describe_masking(config, ran, &mut doc);

    // QSM-specific pipeline
    if config.pipeline.do_qsm {
        describe_reconstruction_grid(config, ran, &mut doc);
        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => {
                // Routed by QSM.rs's `InversionInput`: one echo goes in as wrapped phase (TGV
                // unwraps it itself), several are field-mapped first and go in as the total field.
                // Neither route runs a separate background-removal stage.
                doc.cite(&CITE_TGV);
                doc.say(&both(Stage::Tgv), format!(
                    "QSM was computed using the Total Generalized Variation (TGV) algorithm \
                     (Langkammer et al., 2015), with background field removal and dipole inversion \
                     performed jointly. For single-echo data TGV was applied to the wrapped phase of \
                     the echo and also performed the phase unwrapping; for multi-echo data TGV was \
                     applied to the B0 field map, computed as described below, expressed as phase at \
                     {}. The regularisation weights are in radians at that echo time \
                     and the field strength, and the susceptibility was converted to ppm with them.",
                    echo_time_prose(config.inversion.tgv.te)));
                let changed = inversion_changes(config, QsmAlgorithm::Tgv);
                if !changed.is_empty() {
                    doc.say(&both(Stage::Tgv), format!("TGV was run{}.", changed.with()));
                }
                describe_field_mapping(config, ran, &mut doc);
            }
            QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {
                // `InversionInput::WrappedPhase`: no field mapping and no background removal ran.
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                doc.cite(cite);
                doc.say(&both(Stage::Inversion), format!(
                    "QSM was reconstructed with {} ({}) from the wrapped phase of each echo, with \
                     phase unwrapping, background field removal and dipole inversion performed by the \
                     network.",
                    name, cite_inline(cite),
                ));
                say_tiling(config, Stage::Inversion, &mut doc);
            }
            QsmAlgorithm::Qsmart => {
                // QSMART consumes a pre-computed total field, so the field-mapping
                // stage (phase offset, unwrapping, B0 estimation) still runs and must
                // be described.
                describe_field_mapping(config, ran, &mut doc);
                let (inv_name, inv_cite) = inversion_name_cite(config.inversion.qsmart.inversion);
                doc.cite(&CITE_QSMART);
                doc.cite(inv_cite);
                doc.say(&both(Stage::Qsmart), format!(
                    "QSM was computed using QSMART (Yaghmaie et al., 2021), with spatially dependent \
                     filtering for background field removal, {} dipole inversion ({}), and Frangi \
                     vesselness-based tissue/vasculature separation{}.",
                    inv_name, cite_inline(inv_cite), qsmart_changes(config).comma_with(),
                ));
                // The magnitude vessels were detected on.
                match config.inversion.qsmart.vessel_magnitude {
                    QsmartVesselMagnitude::MeanN4 => {
                        doc.cite(&CITE_N4);
                        doc.say(&both(Stage::Qsmart), "Vessels were detected on the mean magnitude over echoes, \
                            bias-corrected with N4 (Tustison et al., 2010).".to_string());
                    }
                    QsmartVesselMagnitude::Combined => {
                        doc.say(&both(Stage::Qsmart), format!("Vessels were detected on the {}RSS-combined magnitude.",
                            if config.masking.inhomogeneity_correction { "inhomogeneity-corrected " } else { "" }));
                    }
                }
            }
            _ => {
                describe_field_mapping(config, ran, &mut doc);

                // Background removal — only in front of a local-field inversion (QSM.rs
                // `InversionInput`); the total-field methods remove or model it themselves.
                let total_field = !crate::bridge::inversion_input(config, 2).needs_bg_removal();
                if !total_field {
                    let alg = &config.bg_removal.algorithm;
                    let (name, cite) = bg_removal_name_cite(*alg);
                    doc.cite(cite);
                    let erode = config.bg_removal.erode_voxels;
                    if erode > 0 {
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before background field removal, the brain mask was eroded by {erode} voxel{}.",
                            if erode == 1 { "" } else { "s" }));
                    }
                    if config.bg_removal.ukb_mask_trim {
                        doc.cite(&CITE_UKB_QSM);
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before and after background field removal, the mask was trimmed of voxels with \
                             unreliable phase as in the UK Biobank pipeline ({}): phase-reliability maps of the \
                             first and last echoes (the magnitude of the unit phasor averaged over a 2 mm ball) \
                             were thresholded at 0.6 and 0.5 before background removal and at 0.7 and 0.6 after \
                             it, then in each slice components smaller than 300 (after: 200) voxels were removed \
                             and holes smaller than 50 (after: 30) voxels filled, and enclosed holes were filled.",
                            cite_inline(&CITE_UKB_QSM)));
                    }
                    if config.bg_removal.polish_mask {
                        doc.say(&both(Stage::BackgroundRemoval),
                            "After background field removal, the mask was reduced to its largest connected \
                             component with its holes filled, and the local field was set to zero outside it."
                                .to_string());
                    }
                    if *alg == BfAlgorithm::Vsharp {
                        let (v, d) = (&config.bg_removal.vsharp, VsharpConfig::default());
                        let mut changed = Changed::default();
                        changed.add(differs(v.max_radius, d.max_radius) || differs(v.min_radius, d.min_radius)
                            || differs(v.radius_step, d.radius_step),
                            || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
                        changed.add(v.deconvolve != d.deconvolve, || format!(
                            "truncated-SVD deconvolution {}",
                            if v.deconvolve { "enabled" } else { "disabled" }));
                        // The threshold only exists for the deconvolution.
                        changed.add(v.deconvolve && differs(v.threshold, d.threshold),
                            || format!("a deconvolution threshold of {}", v.threshold));
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), changed.with()));
                        let resample = "the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution";
                        match v.grid {
                            VsharpGrid::Finest => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "For anisotropic voxels the V-SHARP background was estimated on an isotropic grid at the smallest acquired voxel dimension: {resample}. Isotropic acquisitions were processed on their acquired grid.")),
                            VsharpGrid::Isotropic if !differs(v.grid_spacing, d.grid_spacing) => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on an isotropic grid: {resample}.")),
                            VsharpGrid::Isotropic => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on a {} mm isotropic grid: {resample}.", v.grid_spacing)),
                            VsharpGrid::Native => {}
                        }
                    } else {
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), bg_removal_changes(config).with()));
                    }

                }

                // Dipole inversion
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                let target = if total_field { " on the total field within the brain mask" } else { "" };
                doc.cite(cite);
                let msmv = medi_msmv(config, &mut doc) + &medi_smv(config);
                let changes = inversion_changes(config, config.inversion.algorithm);
                doc.say(&both(Stage::Inversion), format!("Dipole inversion was performed{} using {} ({}){msmv}{}.",
                    target, name, cite_inline(cite),
                    if msmv.is_empty() { changes.with() } else { changes.comma_with() }));
                describe_medi_csf(config, ran, &mut doc);
                // Deep-learning tiling (memory-bounded, approximate) is noted when enabled — only
                // for the tileable networks (a stray --tile-size on a classical algorithm is ignored).
                if config.inversion.algorithm.tileable() {
                    say_tiling(config, Stage::Inversion, &mut doc);
                } else if config.runs_a_tileable_network() {
                    say_tiling(config, Stage::BackgroundRemoval, &mut doc);
                }
            }
        }

        // Two-pass wraps background removal and inversion, so it is described after both and
        // before referencing, which runs on the combined map.
        describe_two_pass(config, ran, &mut doc);
        // HEIDI after the inversion, and after two-pass on the combined map.
        describe_heidi(config, ran, &mut doc);

        // Referencing
        match config.qsm.reference {
            QsmReference::Mean => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility within \
                     {RECON_MASK}."));
            }
            QsmReference::None => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], "No susceptibility referencing was applied.".to_string());
            }
            QsmReference::Region => {
                let region = config.qsm.reference_region.as_deref().unwrap_or("");
                let pretty = region.replace('-', " ").replace(',', ", ");
                let plural = if region.contains(',') { "s" } else { "" };
                let labels = match config.segmentation.custom_dseg_tool.as_deref() {
                    Some(tool) => format!("'{tool}' segmentation label{plural}"),
                    None => format!("SynthSeg label{plural}"),
                };
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility in the \
                     {pretty} ({labels}), computed within {RECON_MASK}."
                ));
            }
        }
        if says_per_echo(config) {
            doc.say(&[Stage::Reference],
                "For multi-echo data, the susceptibility map reported is the mean across echoes of the \
                 echoes' maps, over the voxels where every echo's map was defined; each echo's map is \
                 reported too.".to_string());
        }
    }

    // A supplied segmentation, every run's or some runs'.
    let dseg_tool = config.segmentation.custom_dseg_tool.as_deref();
    let dseg_supplied = ran.count(Topic::SegmentationSource, Choice::Supplied) > 0;
    let dseg_all_supplied = ran.only(Topic::SegmentationSource, &[Choice::Supplied]);
    if config.pipeline.do_segmentation && dseg_supplied {
        let from = match dseg_tool { Some(t) if t != "*" => format!("'{t}' "), _ => String::new() };
        doc.say(&[Stage::Segmentation], if dseg_all_supplied {
            format!("Segmentations were sourced from the {from}BIDS derivatives.")
        } else {
            format!("Where available, segmentations were sourced from the {from}BIDS derivatives; \
                     otherwise they were generated as follows.")
        });
    }
    if config.pipeline.do_segmentation && !dseg_all_supplied && !ran.none_ran(Stage::Segmentation) {
        doc.cite(&CITE_SYNTHSEG);
        let v2 = config.segmentation.version == SynthSegVersion::V2;
        if v2 {
            doc.cite(&CITE_SYNTHSEG2);
        }
        // The label table counts the background.
        let structures = config.segmentation.version.to_core().n_labels() - 1;
        let (s, d) = (&config.segmentation, SegmentationConfig::default());
        let mut changed = Changed::default();
        changed.add(s.flip_averaging != d.flip_averaging, || format!(
            "left-right flip averaging of the posteriors {}",
            if s.flip_averaging { "enabled" } else { "disabled" }));
        // The version names the weights (and the label count says what they produce); the
        // bring-your-own tool is not a SynthSeg setting.
        if let Some(patch) = s.patch {
            changed.add(true, || format!(
                "the network applied as a sliding window of {p}×{p}×{p}-voxel patches overlapping by at \
                 least half a patch, their posteriors Gaussian-weighted", p = patch.div_ceil(16) * 16));
        }
        changed.add_fields(s, &d, &["version", "flip_averaging", "custom_dseg_tool", "patch"]);
        doc.say(&[Stage::Segmentation], format!(
            "The brain was parcellated from the GRE magnitude using SynthSeg {} ({}), yielding {} \
             FreeSurfer-labelled structures{}.",
            config.segmentation.version,
            if v2 { "Billot, Greve, et al., 2023; Billot, Magdamo, et al., 2023" } else { "Billot et al., 2023" },
            structures, changed.comma_with(),
        ));
    }
    if config.pipeline.do_analysis && !ran.none_ran(Stage::Analysis) {
        doc.say(&[Stage::Analysis], 
            "Each quantitative map was then summarised within every segmented structure (mean, \
             standard deviation, median, minimum, maximum and 5th/95th percentiles over the \
             structure's voxels), excluding voxels the map does not cover.".to_string());
    }

    if config.pipeline.do_smwi && !ran.none_ran(Stage::Smwi) {
        doc.cite(&CITE_SMWI);
        let (s, d) = (&config.smwi, SmwiConfig::default());
        let mut mask = Changed::default();
        mask.add(differs(s.threshold_ppm, d.threshold_ppm), || format!("|χ| threshold {:.2} ppm", s.threshold_ppm));
        mask.add(differs(s.power, d.power), || format!("mask raised to the power {}", s.power));
        let slab = if s.mip_window != d.mip_window { format!(" over {} slices", s.mip_window) } else { String::new() };
        doc.say(&[Stage::Smwi], format!(
            "Susceptibility map-weighted images were computed (Gho et al., 2014) by weighting the \
             magnitude with a mask derived from the susceptibility map{}, separately for \
             paramagnetic and diamagnetic sources, each followed by a minimum-intensity \
             projection{slab}.",
            mask.paren(),
        ));
    }

    // SWI
    if config.pipeline.do_swi {
        let (swi, d) = (&config.swi, SwiConfig::default());
        let mut changed: Vec<String> = Vec::new();
        match swi.unwrap {
            _ if swi.unwrap == d.unwrap => {}
            SwiUnwrap::Romeo => {
                changed.push("ROMEO phase unwrapping (Dymerska et al., 2021)".into());
                doc.cite(&CITE_ROMEO);
            }
            SwiUnwrap::LaplacianSlice => changed.push("slice-wise Laplacian phase unwrapping".into()),
            SwiUnwrap::Laplacian => changed.push("Laplacian phase unwrapping".into()),
        }
        if swi.hp_sigma.iter().zip(d.hp_sigma).any(|(a, b)| differs(*a, b)) {
            changed.push(format!(
                "a high-pass filter σ of {} voxels",
                swi.hp_sigma.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "),
            ));
        }
        if swi.scaling != d.scaling {
            changed.push(format!("{} phase-mask scaling", swi.scaling));
        }
        if differs(swi.strength, d.strength) {
            changed.push(format!("phase-mask strength {}", swi.strength));
        }
        let mip = if swi.mip_window != d.mip_window {
            format!("Minimum-intensity projections were taken over {}-slice slabs.", swi.mip_window)
        } else {
            "Minimum-intensity projections of these images were also computed.".to_string()
        };
        let using = if changed.is_empty() { String::new() } else { format!(" using {}", join_list(&changed)) };
        // ROMEO and slice-wise unwrapping filter in-plane whatever the σ; Laplacian does so when
        // σ has no slice component (the default).
        let phase = if swi.filtered_phase {
            let in_plane = swi.unwrap != SwiUnwrap::Laplacian || swi.hp_sigma[2] == 0.0;
            format!(" The filtered phase that weights these images, CLEAR-SWI's phase after {} Gaussian \
                     high-pass filter within a head mask from the first echo's magnitude, was also saved, in Hz \
                     for multi-echo data and in radians for single-echo data.",
                    if in_plane { "an in-plane" } else { "a" })
        } else {
            String::new()
        };
        doc.cite(&CITE_CLEARSWI);
        doc.say(&[Stage::Swi], format!(
            "Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021){using}. {mip}{phase}"
        ));
    }

    // T2*/R2*
    if config.pipeline.do_t2starmap && config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_t2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    }

    // R2 / R2'. Worth stating how R2' was obtained, not just that it was: a measured R2' and an
    // estimated one support different claims, and only one of them is a measurement.
    if config.pipeline.do_r2map || config.pipeline.do_r2primemap {
        if config.pipeline.do_r2map && ran.only(Topic::R2Source, &[Choice::Supplied]) {
            doc.say(&[Stage::R2], "R2 maps were sourced from the supplied BIDS derivatives.".to_string());
        } else if config.pipeline.do_r2map && !ran.only(Topic::R2Source, &[Choice::Unavailable]) {
            doc.cite(&CITE_EPG);
            doc.say(&[Stage::R2], 
                "R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended \
                 Phase Graph dictionary (Weigel, 2015).".to_string());
        }
        if config.pipeline.do_r2primemap {
            // R2PRIMEnet's own settings; the field-strength override gets a sentence below.
            let mut r2pn = Changed::default();
            r2pn.add_fields(&config.separation.r2primenet, &R2primenetConfig::default(),
                &["ignore_field_mismatch"]);
            let r2pn = r2pn.semi();
            // What the runs that made an R2′ did, where they all did the same.
            let measured = ran.count(Topic::R2primeSource, Choice::Measured) > 0;
            let predicted = ran.count(Topic::R2primeSource, Choice::Predicted) > 0;
            let none_made = ran.only(Topic::R2primeSource, &[Choice::Unavailable]);
            match config.separation.custom_r2prime_tool.as_deref() {
                _ if none_made => {}
                // `auto` that only measured, or only predicted.
                None if config.separation.r2prime_strategy == R2PrimeStrategy::Auto && measured != predicted => {
                    if measured {
                        doc.say(&[Stage::R2], "R2' was computed as R2* - R2.".to_string());
                    } else {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., 2025{r2pn}).",
                        ));
                    }
                }
                Some(_) => doc.say(&[Stage::R2], 
                    "R2' maps were taken from the supplied BIDS derivatives.".to_string()),
                None => match config.separation.r2prime_strategy {
                    R2PrimeStrategy::Mese => doc.say(&[Stage::R2], 
                        "R2' was computed as R2* - R2.".to_string()),
                    R2PrimeStrategy::R2primenet => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., \
                             2025{r2pn}).",
                        ));
                    }
                    R2PrimeStrategy::Auto => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], if config.separation.r2primenet.ignore_field_mismatch {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and estimated from R2* with R2PRIMEnet \
                             (Kim et al., 2025{r2pn}) otherwise.")
                        } else {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and otherwise, at 3 T only, estimated \
                             from R2* with R2PRIMEnet (Kim et al., 2025{r2pn}).")
                        });
                    }
                },
            }
            // Both ways QSMxT computes an R2′ (R2* − R2 and R2PRIMEnet) set negative values to zero.
            if config.separation.custom_r2prime_tool.is_none() && !none_made {
                doc.say(&[Stage::R2], "Negative R2' values were set to zero.".to_string());
            }
            // Said where a run applied it off 3 T, or nothing is known of the runs.
            let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0
                || ran.0.is_none_or(|r| r.split(Topic::R2primeSource).is_empty());
            if config.separation.custom_r2prime_tool.is_none()
                && config.separation.r2prime_strategy != R2PrimeStrategy::Mese
                && config.separation.r2primenet.ignore_field_mismatch
                && off_field && !none_made
            {
                doc.say(&[Stage::R2], 
                    "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data \
                     acquired at other field strengths."
                        .to_string());
            }
        }
    }

    // Chi-separation (susceptibility source separation)
    if config.pipeline.do_chi_separation {
        let (sep_name, sep_cite) = separation_name_cite(config.separation.algorithm);
        doc.cite(sep_cite);
        doc.say(&[Stage::ChiSeparation], format!(
            "Paramagnetic and diamagnetic susceptibility maps were computed using {} ({}) within {}{}.",
            sep_name, cite_inline(sep_cite),
            // Named in full by the referencing sentence, unless the map was left unreferenced.
            if config.pipeline.do_qsm && config.qsm.reference != QsmReference::None { RECON_MASK_AGAIN } else { RECON_MASK },
            separation_changes(config).comma_with(),
        ));
        // DECOMPOSE's signal phase comes from a QSM per echo: a per-echo QSM's, or the one QSM.
        if config.separation.algorithm == SeparationAlgorithm::Decompose {
            doc.say(&[Stage::ChiSeparation], if says_per_echo(config) && config.separation.custom_qsm_tool.is_none() {
                "Each echo's susceptibility map was used for the signal phase at that echo.".to_string()
            } else {
                "The same susceptibility map was used for the signal phase at every echo.".to_string()
            });
            if config.separation.decompose.unwrap_phase {
                doc.say(&[Stage::ChiSeparation], "The phase was unwrapped along the echoes.".to_string());
            }
            if config.separation.decompose.chi_bound == ChiBoundChoice::EchoSpacing {
                doc.say(&[Stage::ChiSeparation], chi_bound_sentence(ran));
            }
        }
        match config.separation.algorithm {
            SeparationAlgorithm::ChiSepIlsqr => doc.say(&[Stage::ChiSeparation],
                "The two sources were initialised from the susceptibility map and solved by LSQR, \
                 streaking removal and a non-negative refinement.".to_string()),
            SeparationAlgorithm::ChiSepMedi => {
                let r2star = match config.separation.chi_sep_medi.csf_r2star {
                    ChiSepCsfR2star::Arlo => {
                        doc.cite(&CITE_ARLO);
                        "ARLO R2* (Pei et al., 2015)"
                    }
                    ChiSepCsfR2star::Nnls => "R2* from a non-negative least-squares mono-exponential fit",
                };
                doc.say(&[Stage::ChiSeparation], format!(
                    "The two sources were initialised from the susceptibility map, the data terms were \
                     weighted by a noise map from the echo magnitudes, and the susceptibility within a \
                     CSF mask from {r2star} was regularised towards uniformity."));
            }
            _ => {}
        }
        // The networks' outputs as QSM.rs returns them.
        if matches!(config.separation.algorithm, SeparationAlgorithm::ChiSepNet | SeparationAlgorithm::SusepNet) {
            doc.say(&[Stage::ChiSeparation], "Negative χ+ and |χ−| values were set to zero.".to_string());
        }
        if crate::config::chisepnet_route_can_estimate_r2prime(config) {
            chisepnet_r2prime_sentences(&mut doc, config, ran);
        }
        if crate::stages::separation::reconstructs_qsm(config) {
            let mut q = Changed::default();
            let defaults = SeparationConfig::default();
            let d = match config.separation.algorithm {
                SeparationAlgorithm::ChiSepMedi => &defaults.chi_sep_medi.qsm_ilsqr,
                _ => &defaults.chi_sep_ilsqr.qsm_ilsqr,
            };
            q.add_fields(crate::stages::separation::initial_qsm_ilsqr(config), d, &[]);
            doc.cite(&CITE_ILSQR);
            doc.say(&[Stage::ChiSeparation], format!(
                "The susceptibility map separated was reconstructed from the local field by iLSQR ({}){}.",
                cite_inline(&CITE_ILSQR), q.with()));
        }
    }

    // Multi-orientation. Described after the per-orientation pipeline because that is the
    // order it runs in: each orientation is unwrapped and background-corrected on its own,
    // and only the local fields are combined. Whether a group forms depends on the runs, so the
    // setting alone says nothing happened: the record does ([`Topic::OrientationGroup`]). Before
    // any run, or where only some runs were combined, the wording is conditional (#496).
    let combined = ran.count(Topic::OrientationGroup, Choice::Applied);
    let every_run = combined > 0 && combined == ran.runs();
    if config.multi_orientation.enabled() && (ran.0.is_none() || combined > 0) {
        let fields = if every_run {
            "Local field maps from multiple head orientations"
        } else {
            "Where a session held several head orientations of the same object, local field maps \
             from those orientations"
        };
        match config.multi_orientation.algorithm {
            MultiOrientAlgorithm::Cosmos => {
                doc.cite(&CITE_COSMOS);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by Calculation Of Susceptibility through Multiple \
                     Orientation Sampling (COSMOS; Liu et al., 2009), using each orientation's B0 \
                     direction expressed in that common frame."),
                );
            }
            MultiOrientAlgorithm::Sti => {
                doc.cite(&CITE_STI);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by susceptibility tensor imaging (STI; Liu, 2010), \
                     yielding the six independent components of the symmetric susceptibility \
                     tensor along with mean magnetic susceptibility (MMS), magnetic susceptibility \
                     anisotropy (MSA) and the principal eigenvector."),
                );
            }
        }
        // How the orientations got onto one grid is part of the method, not an implementation
        // detail: it determines where the B0 directions came from, which is half the input.
        if config.multi_orientation.register {
            let reference = config.multi_orientation.reference.trim();
            let target = if reference.is_empty() {
                "the first orientation's grid".to_string()
            } else {
                format!("the grid of the reference orientation ({reference})")
            };
            doc.say(&[], format!(
                "Orientations that were not already on a common grid were rigidly co-registered \
                 (6 degrees of freedom, normalized cross-correlation over a multi-resolution \
                 pyramid{}) onto {target} using the magnitude images, and each \
                 orientation's B0 direction was derived from the recovered rotation.",
                registration_changes(config).semi(),
            ));
        } else {
            doc.say(&[], 
                "The orientations were taken to be already co-registered onto a common grid."
                    .to_string(),
            );
        }
        if differs(config.multi_orientation.lambda, MultiOrientationConfig::default().lambda) {
            doc.say(&[], format!(
                "The multi-orientation inversion was regularized with lambda = {}.",
                config.multi_orientation.lambda
            ));
        }
        let m = &config.multi_orientation;
        if m.algorithm == MultiOrientAlgorithm::Cosmos {
            if m.magnitude_weighting {
                let d = MultiOrientationConfig::default();
                let mut cg = Changed::default();
                cg.add(differs(m.cosmos_tol, d.cosmos_tol), || format!("tolerance {}", m.cosmos_tol));
                cg.add(m.cosmos_max_iter != d.cosmos_max_iter, || format!("at most {} iterations", m.cosmos_max_iter));
                doc.say(&[], format!(
                    "Each orientation's field was weighted by its magnitude, and the weighted \
                     least-squares problem solved by conjugate gradient{}.", cg.paren()));
            }
            doc.say(&[], 
                "The COSMOS map was referenced in the same way as the single-orientation maps, \
                 over the voxels covered by every orientation's background-field-removal mask."
                    .to_string(),
            );
        }
    }

    // DICOM export. It is the front end's, after the run, and can fail without failing the run, so
    // the setting alone says nothing happened: the record does ([`RunRecord::dicom_exported`]).
    // Before any run the setting is described, as a native run exports; a browser run never does
    // (the browser profile clears the setting).
    let exported = ran.dicom_exported();
    if config.pipeline.export_dicom && (ran.0.is_none() || exported > 0) {
        doc.say(&[], if ran.0.is_none() || exported == ran.runs() {
            "Final maps were additionally exported as DICOM series.".to_string()
        } else {
            "For some of the runs, final maps were additionally exported as DICOM series.".to_string()
        });
    }

    doc
}

/// The reconstruction-grid switches, all off by default, so each is said only when on.
fn describe_reconstruction_grid(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let p = &config.pipeline;
    if p.ignore_slice_gap {
        doc.say(&[Stage::Load], "Acquisitions whose slices were separated by a gap were reconstructed as if the slices were contiguous.".to_string());
    }
    let space = match p.output_space {
        OutputSpace::Acquired => "",
        OutputSpace::Working => ", and the derivatives were written on the resampled grid",
    };
    // Resampled only because an algorithm needs axial data: the switches do not say so.
    if !p.axial_resampling && ran.count(Topic::Grid, Choice::ResampledForAlgorithm) > 0 {
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "Acquisitions whose B0 direction was more than {}° from the slice normal were resampled onto an \
             axial grid, with B0 along the slice normal, before reconstruction, as the algorithms used assume \
             it{space}.", crate::scan::AXIAL_ONLY_TOLERANCE_DEG));
    }
    // None of the runs was resampled: there is nothing to say.
    if p.axial_resampling && !ran.only(Topic::Grid, &[Choice::Acquired]) {
        let which = if differs(p.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD) {
            format!("Acquisitions more than {}° oblique", p.obliquity_threshold)
        } else {
            "Oblique acquisitions".to_string()
        };
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "{which} were resampled onto an axial grid, with B0 along the slice normal, before reconstruction{space}."));
    }
    if p.crop_to_mask {
        let margin = if differs(p.crop_margin_mm, PipelineToggles::default().crop_margin_mm) {
            format!(" with a {} mm margin", p.crop_margin_mm)
        } else {
            String::new()
        };
        doc.say(&reconstruction(), format!("Reconstruction was performed within a box around the brain mask{margin}."));
    }
    if p.fft_padding {
        doc.say(&reconstruction(), "Before the FFT-based stages, the grid was zero-padded to an FFT-efficient size.".to_string());
    }
}

/// "64" for a uniform pad, "64×64×32" otherwise.
fn describe_pad(p: [usize; 3]) -> String {
    if p[0] == p[1] && p[1] == p[2] { p[0].to_string() } else { format!("{}×{}×{}", p[0], p[1], p[2]) }
}

/// What phase was converted to ppm with: the scanner's centre frequency, which is QSMxT's rule,
/// unless runs recorded falling back to the nominal field strength ([`Topic::CentreFrequency`]):
/// all of them, or some ("where the data gave it").
fn describe_centre_frequency(ran: Ran, doc: &mut Draft) {
    let fell_back = ran.count(Topic::CentreFrequency, Choice::Unavailable);
    doc.say(&[Stage::FieldMap], if fell_back == 0 {
        "Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency)."
    } else if fell_back >= ran.runs() {
        "Phase was converted to ppm with the nominal field strength, as the data gave no centre \
         frequency (ImagingFrequency)."
    } else {
        "Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency) where the \
         data gave it, and with the nominal field strength otherwise."
    }.to_string());
}

/// Describe the field-mapping stage: phase offset / bipolar correction, phase
/// unwrapping, and B0 estimation. Shared by the standard pipeline and QSMART
/// (which consumes a pre-computed total field, so these steps still run).
/// Whether the text describes a per-echo QSM (`qsm.per_echo`, where nothing rules it out): said
/// "for multi-echo data", as a single echo's QSM is the only one there is.
fn says_per_echo(config: &PipelineConfig) -> bool {
    config.pipeline.do_qsm && config.qsm.per_echo && crate::bridge::per_echo_conflict(config).is_none()
}

fn describe_field_mapping(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    describe_centre_frequency(ran, doc);
    let fm = &config.field_mapping;
    let d = FieldMappingConfig::default();
    let m = &config.motion_correction;
    // Decided per run ([`Topic::MotionCorrection`]), worded as MEDI+0 is: applied to every run,
    // plainly; to none, that it was not; where the runs differ, or nothing is known of them, with
    // the condition.
    if m.enabled && ran.only(Topic::MotionCorrection, &[Choice::NotApplied]) {
        doc.say(&[Stage::Motion],
            "Motion correction was not applied: it needs at least two echoes with magnitude images, and no run had them.");
    } else if m.enabled {
        let scope = if ran.only(Topic::MotionCorrection, &[Choice::Applied]) { "" }
            else { ", in runs with more than one echo and magnitude images" };
        // The reference echo says what was done (which echo the others were moved onto), so it
        // is always named; the registration settings only when changed.
        let reference = match m.reference {
            crate::motion::MotionReference::First => "the first echo".to_string(),
            crate::motion::MotionReference::Last => "the last echo".to_string(),
            crate::motion::MotionReference::Echo(n) => format!("echo {n}"),
        };
        let mut changed = registration_changes(config);
        changed.add_fields(m, &crate::motion::MotionCorrectionConfig::default(),
            &["enabled", "reference", "min_ncc"]);
        doc.say(&[Stage::Motion], format!(
            "Before field mapping{scope}, every echo was rigidly co-registered to {reference} by maximising the normalised cross-correlation of the magnitude images over a multi-resolution pyramid{}, and the phase was resampled with it through the complex signal; the brain mask was restricted to voxels covered by every echo.",
            changed.paren()));
    }
    let sigma = if fm.phase_offset_sigma_mm.iter().zip(d.phase_offset_sigma_mm).any(|(a, b)| differs(*a, b)) {
        format!("; smoothing σ = {} mm",
            fm.phase_offset_sigma_mm.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "))
    } else {
        String::new()
    };
    // A per-echo QSM: each echo unwrapped on its own; nothing combines or compares the echoes.
    let per_echo = says_per_echo(config);
    if per_echo {
        doc.say(&[Stage::FieldMap, Stage::BackgroundRemoval(Pass::Main), Stage::Inversion(Pass::Main), Stage::Reference],
            "For multi-echo data, QSM was reconstructed separately for each echo: each echo's unwrapped \
             phase was divided by its echo time to give that echo's field map, and background field \
             removal, dipole inversion and referencing were applied to each echo's field map on its own.".to_string());
    }
    // MEDI's complex fit fits its own phase offset, and takes no bipolar correction.
    if fm.b0_estimation == B0Estimation::ComplexFit && !per_echo {
        describe_complex_fit(config, doc);
        return;
    }
    match (fm.phase_offset_removal && !per_echo, fm.bipolar_correction && fm.phase_offset_removal && !per_echo) {
        (true, true) => {
            doc.cite(&CITE_MCPC3DS);
            doc.cite(&CITE_BIPOLAR);
            doc.say(&[Stage::FieldMap], format!("Phase offset removal (Eckstein et al., 2018{sigma}) and bipolar gradient correction (Eckstein, 2021) were applied."));
        }
        (true, false) => {
            doc.cite(&CITE_MCPC3DS);
            doc.say(&[Stage::FieldMap], format!("Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018{sigma})."));
        }
        (false, true) => {
            doc.cite(&CITE_BIPOLAR);
            doc.say(&[Stage::FieldMap], "Bipolar gradient correction (Eckstein, 2021) was applied.".to_string());
        }
        (false, false) => {}
    }
    // Where the phase offset could not be estimated there is no field ([`Topic::FieldMask`]); said
    // when a run left any voxel out.
    if ran.count(Topic::FieldMask, Choice::Partial) > 0 {
        doc.say(&[Stage::FieldMap],
            "Voxels where the phase offset could not be estimated were excluded from the field map and all later steps.".to_string());
    }

    // Unwrapping
    match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mode = if fm.romeo.individual == d.romeo.individual {
                ""
            } else if fm.romeo.individual {
                " in individual per-echo mode"
            } else {
                " in template-based temporal mode"
            };
            let mut changed = Changed::default();
            changed.add_fields(&fm.romeo, &d.romeo, &["individual"]);
            doc.cite(&CITE_ROMEO);
            doc.say(&[Stage::FieldMap], format!("Phase unwrapping was performed using ROMEO (Dymerska et al., 2021){mode}{}.", changed.with()));
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = match fm.laplacian_solver {
                _ if fm.laplacian_solver == d.laplacian_solver => String::new(),
                LaplacianSolver::Dct => " solved as unweighted least squares with a Neumann-boundary DCT (Ghiglia & Romero, 1994)".to_string(),
                LaplacianSolver::Fft => " with an FFT Poisson solve".to_string(),
                LaplacianSolver::Mirror => " with an FFT Poisson solve on the volume mirrored along each axis".to_string(),
                LaplacianSolver::Mgpcg => {
                    doc.cite(&CITE_LBV);
                    ", solved inside the brain mask with the phase held at zero outside it by multigrid-preconditioned \
                     conjugate gradients as in QSM.jl, which removes the background field in the same step, \
                     equivalent to LBV background removal (Zhou et al., 2014)".to_string()
                }
            };
            // The phase is set to zero outside the brain mask first unless it is unwrapped raw.
            let whole = if fm.laplacian_unmasked { ", on the raw phase over the whole volume," } else { "" };
            // The padding only exists for the FFT solve.
            let pad = if fm.laplacian_solver == LaplacianSolver::Fft && fm.laplacian_fft_pad != d.laplacian_fft_pad {
                format!(" on the volume zero-padded by {} voxels", describe_pad(fm.laplacian_fft_pad))
            } else {
                String::new()
            };
            doc.cite(&CITE_LAPLACIAN_UNWRAP);
            doc.say(&[Stage::FieldMap], format!(
                "Phase unwrapping was performed separately for each echo{whole} using the Laplacian method (Schofield & Zhu, 2003){solver}{pad}."));
        }
        UnwrappingAlgorithm::Prelude => {
            doc.cite(&CITE_PRELUDE);
            doc.say(&[Stage::FieldMap], format!(
                "Phase unwrapping was performed separately for each echo using PRELUDE (Jenkinson, 2003){}.",
                prelude_changes(fm, &d).with()));
        }
        UnwrappingAlgorithm::MediRegionGrowing => {
            doc.cite(&CITE_FIT_PPM_COMPLEX);
            doc.say(&[Stage::FieldMap],
                "Phase unwrapping was performed separately for each echo using MEDI's magnitude-guided region growing (Liu et al., 2013).".to_string());
        }
    }

    // A per-echo QSM combines no echoes into a B0 field map.
    if per_echo {
        return;
    }
    // B0 estimation. The linear fit's settings are shared with the robust fit.
    let mut linear = Changed::default();
    linear.add_fields(&fm.linear_fit, &d.linear_fit, &[]);
    match fm.b0_estimation {
        B0Estimation::WeightedAvg => {
            if fm.b0_weight_type == B0WeightType::AssumedDecay {
                let t2star = if differs(fm.b0_weight_assumed_t2star_ms, d.b0_weight_assumed_t2star_ms) {
                    format!(" of {} ms", fm.b0_weight_assumed_t2star_ms)
                } else {
                    String::new()
                };
                doc.cite(&CITE_UKB_QSM);
                doc.say(&[Stage::FieldMap], format!(
                    "The B0 field map was estimated as a weighted average of the unwrapped echo phases with per-echo weights TE·exp(−TE/T2*), computed from the echo times and an assumed T2*{t2star} and identical in every voxel (Wang et al., 2022)."));
            } else {
                let mut changed = Changed::default();
                changed.add(fm.b0_weight_type != d.b0_weight_type, || format!("{} weighting", fm.b0_weight_type));
                doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated as a weighted average of the unwrapped echo phases{}.", changed.with()));
            }
        }
        B0Estimation::LinearFit => {
            doc.say(&[Stage::FieldMap], format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time{}.", linear.with()));
        }
        B0Estimation::RobustFit => {
            let (r, rd) = (&fm.robust_fit, &d.robust_fit);
            let conventional = match r.loss { RobustLoss::Tukey => 4.685, RobustLoss::Huber => 1.345 };
            let mut changed = Changed::default();
            changed.add(r.loss != rd.loss, || format!("a {} loss", if r.loss == RobustLoss::Tukey { "Tukey biweight" } else { "Huber" }));
            changed.add(r.tuning.is_some_and(|t| differs(t, conventional)), || format!("tuning constant {}", r.tuning.unwrap_or(conventional)));
            changed.0.extend(linear.0);
            doc.say(&[Stage::FieldMap], if r.iterations == 0 {
                format!("The B0 field map was estimated using magnitude-weighted linear fit of phase vs echo time, with per-echo residual statistics reported to flag echoes inconsistent with the fit{}.",
                    if changed.is_empty() { String::new() } else { format!("; {}", join_list(&changed.0)) })
            } else {
                changed.add(r.iterations != rd.iterations, || format!("{} iterations", r.iterations));
                format!("The B0 field map was estimated with a motion-robust magnitude-weighted linear fit of phase vs echo time by iteratively reweighted least squares{}.", changed.with())
            });
        }
        B0Estimation::QsmartEchofit => {
            doc.cite(&CITE_QSMART);
            let threshold = match fm.qsmart_fit_threshold() {
                qsm_core::utils::QsmartFitThreshold::Percentile(p) => format!("its {p}th percentile"),
                qsm_core::utils::QsmartFitThreshold::Absolute(t) => format!("{t}"),
            };
            // The voxels the fit marks unreliable are left out of QSMART only.
            let excluded = if config.inversion.algorithm == QsmAlgorithm::Qsmart {
                format!(", and voxels whose blurred fit residual exceeded {threshold} were excluded from the QSMART reconstruction")
            } else {
                String::new()
            };
            doc.say(&[Stage::FieldMap], format!(
                "The B0 field map was estimated by a magnitude-weighted fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021){excluded}."));
        }
        // Worded by describe_complex_fit, which returned above.
        B0Estimation::ComplexFit => {}
    }
}

/// MEDI's field map (QSM.rs `B0EstimationMethod::ComplexFit`): `Fit_ppm_complex` on the complex
/// echoes, then the fitted phase per echo spacing unwrapped and divided by the echo spacing. No
/// phase offsets are removed first (the fit has its own).
fn describe_complex_fit(config: &PipelineConfig, doc: &mut Draft) {
    let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
    let mut fit = Changed::default();
    fit.add_fields(&fm.complex_fit, &d.complex_fit, &[]);
    doc.cite(&CITE_FIT_PPM_COMPLEX);
    let unwrapping = match fm.unwrapping_algorithm {
        UnwrappingAlgorithm::Romeo => {
            let mut changed = Changed::default();
            changed.add_fields(&fm.romeo, &d.romeo, &["individual", "template"]);
            doc.cite(&CITE_ROMEO);
            format!("ROMEO (Dymerska et al., 2021){}", changed.with())
        }
        UnwrappingAlgorithm::Laplacian => {
            let solver = if fm.laplacian_solver == d.laplacian_solver {
                String::new()
            } else {
                format!(" with the {} solver", fm.laplacian_solver)
            };
            doc.cite(&CITE_LAPLACIAN_UNWRAP);
            format!("the Laplacian method (Schofield & Zhu, 2003){solver}")
        }
        UnwrappingAlgorithm::Prelude => {
            doc.cite(&CITE_PRELUDE);
            format!("PRELUDE (Jenkinson, 2003){}", prelude_changes(fm, &d).with())
        }
        UnwrappingAlgorithm::MediRegionGrowing => {
            "MEDI's magnitude-guided region growing (unwrapPhase, MEDI toolbox)".to_string()
        }
    };
    doc.say(&[Stage::FieldMap], format!(
        "The B0 field map was estimated by a magnitude-weighted nonlinear fit of the complex echo signal (Fit_ppm_complex, MEDI toolbox; Liu et al., 2013{}); the fitted phase per echo spacing was unwrapped using {unwrapping} and divided by the echo spacing.",
        fit.semi()));
}

/// Prose for a list of mask refinement ops, in the order they run.
/// An op's settings are compared with what the op is when written without arguments
/// (`erode` is one iteration, `gaussian` is 4 mm), so only the ones the user chose are stated.
fn describe_refinements(ops: &[MaskOp], doc: &mut Draft) -> Vec<String> {
    let default_of = |spec: &str| parse_mask_op(spec).expect("bare mask op parses");
    ops.iter().map(|op| match op {
        MaskOp::Erode { iterations } | MaskOp::Dilate { iterations } => {
            let (name, spec) = if matches!(op, MaskOp::Erode { .. }) { ("erosion", "erode") } else { ("dilation", "dilate") };
            if *op == default_of(spec) {
                name.to_string()
            } else {
                format!("{name} ({iterations} iteration{})", if *iterations != 1 { "s" } else { "" })
            }
        }
        MaskOp::Close { radius } if *op != default_of("close") => format!("morphological closing (radius {radius} voxels)"),
        MaskOp::Close { .. } => "morphological closing".to_string(),
        MaskOp::MinIntensity { value } => format!("removal of voxels with an intensity below {value}"),
        MaskOp::Majority => "a 3×3×3 majority filter".to_string(),
        MaskOp::LargestComponent => "the largest connected component kept".to_string(),
        MaskOp::Imfill => "every hole filled".to_string(),
        MaskOp::FillHoles { max_size: 0 } => "hole-filling".to_string(),
        MaskOp::FillHoles { max_size } => format!("hole-filling (holes of at most {} voxels)", max_size),
        MaskOp::GaussianSmooth { sigma_mm } if *op != default_of("gaussian") => format!("Gaussian smoothing (σ = {sigma_mm} mm)"),
        MaskOp::GaussianSmooth { .. } => "Gaussian smoothing".to_string(),
        MaskOp::SignalErode { threshold, depth_cap, global_erosions, bias_sigma, min_component } => {
            doc.cite(&CITE_QSMCI_SIGNAL_EROSION);
            let MaskOp::SignalErode {
                threshold: dt, depth_cap: dd, global_erosions: dg, bias_sigma: db, min_component: dm,
            } = MaskOp::signal_erode_default() else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(*global_erosions != dg, || format!("{} plain erosion{} first", global_erosions, if *global_erosions != 1 { "s" } else { "" }));
            changed.add(differs(*threshold, dt), || format!("threshold {threshold:.2} of the median bias-corrected magnitude"));
            changed.add(*depth_cap != dd, || if *depth_cap == 0 { "no depth limit".to_string() } else { format!("at most {depth_cap} voxels deep") });
            changed.add(differs(*bias_sigma, db), || format!("bias-field σ = {bias_sigma}"));
            changed.add(*min_component != dm, || format!("components under {min_component} voxels discarded"));
            format!("signal-gated erosion (removal of low-signal boundary voxels{}; QSM-CI)",
                if changed.is_empty() { String::new() } else { format!(", {}", join_list(&changed.0)) })
        }
        _ => format!("{}", op),
    }).collect()
}

/// Prose for a mask section's input image and generator op — "Otsu thresholding of the ROMEO
/// phase quality map", and so on. Shared by the main mask and the two-pass reliable mask, so the
/// same recipe never gets described two different ways.
fn describe_generator(
    section: &MaskSection, config: &PipelineConfig, doc: &mut Draft,
) -> String {
    // Input description — clarify which magnitude goes into ROMEO
    let corrected = config.masking.inhomogeneity_correction;
    let input_desc = match section.input {
        MaskingInput::PhaseQuality => {
            doc.cite(&CITE_ROMEO);
            if corrected {
                "the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude)"
            } else {
                "the ROMEO phase quality map (computed from phase data and the RSS-combined magnitude)"
            }
        }
        MaskingInput::Magnitude => {
            if corrected { "the inhomogeneity-corrected RSS-combined magnitude image" }
            else { "the RSS-combined magnitude image" }
        }
        MaskingInput::MagnitudeFirst => {
            if corrected { "the inhomogeneity-corrected first-echo magnitude image" }
            else { "the first-echo magnitude image" }
        }
        MaskingInput::MagnitudeLast => {
            if corrected { "the inhomogeneity-corrected last-echo magnitude image" }
            else { "the last-echo magnitude image" }
        }
    };

    match &section.generator {
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => {
            doc.cite(&CITE_OTSU);
            format!("Otsu thresholding (Otsu, 1979) of {}", input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => {
            format!("fixed thresholding (value={:.4}) of {}", value.unwrap_or(0.5), input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } => {
            format!("percentile thresholding ({}th percentile) of {}", value.unwrap_or(75.0), input_desc)
        }
        MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } => {
            doc.cite(&CITE_OTSU);
            format!("thresholding of {} at STI Suite's default level (Otsu's threshold of the middle slice; Otsu, 1979)", input_desc)
        }
        MaskOp::Bet { fractional_intensity, voxel_scale } => {
            doc.cite(&CITE_BET);
            let MaskOp::Bet { fractional_intensity: df, .. } = parse_mask_op("bet").expect("bare bet parses")
                else { unreachable!() };
            let mut changed = Changed::default();
            changed.add(differs(*fractional_intensity, df), || format!("f={fractional_intensity:.2}"));
            // The surface settings, from the shared BET config.
            let (b, d) = (&config.bet, crate::config::BetConfig::default());
            changed.add(differs(b.smoothness, d.smoothness), || format!("smoothness {}", b.smoothness));
            changed.add(differs(b.gradient_threshold, d.gradient_threshold), || format!("gradient threshold {}", b.gradient_threshold));
            changed.add(b.iterations != d.iterations, || format!("{} iterations", b.iterations));
            changed.add(b.subdivisions != d.subdivisions, || format!("icosphere subdivision level {}", b.subdivisions));
            // A scaled run did not see the acquisition's own geometry, which changes what BET's
            // millimetre-scale surface model was fitted to.
            changed.add(differs(*voxel_scale, crate::masking::bet_default_voxel_scale()),
                || format!("voxel sizes scaled by {voxel_scale}x"));
            // BET as MEDI's BET.m runs bet2: on the array, so on the voxels as they are stored.
            let orientation = match b.orientation {
                crate::enums::BetOrientation::Fsl => "",
                crate::enums::BetOrientation::AsStored => ", in its stored voxel order as MEDI's BET.m runs it",
            };
            format!("BET brain extraction (Smith, 2002{}) of {}{orientation}", changed.semi(), input_desc)
        }
        MaskOp::Rs2Net { tta, tile_step } => {
            doc.cite(&CITE_RS2NET);
            let mut changed = Changed::default();
            changed.add(differs(*tile_step, crate::masking::rs2_net_default_tile_step()),
                || format!("sliding-window patches stepped by {:.0}% of the patch", tile_step * 100.0));
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("RS2-Net deep-learning rodent brain extraction (Lin et al., 2024{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::MindGrab { tta } => {
            doc.cite(&CITE_MINDGRAB);
            let mut changed = Changed::default();
            changed.add(*tta, || "left-right flip test-time augmentation".to_string());
            format!("MindGrab deep-learning brain extraction (Fani et al., 2025{}) of {}", changed.semi(), input_desc)
        }
        MaskOp::HdBet { patch, tta, tile_step } => {
            doc.cite(&CITE_HDBET);
            let mut changed = Changed::default();
            if let Some(p) = patch {
                changed.add(true, || format!("{}x{}x{}-voxel sliding-window patches", p[0], p[1], p[2]));
                changed.add(differs(*tile_step, crate::masking::hd_bet_default_tile_step()),
                    || format!("windows stepped by {:.0}% of the patch", tile_step * 100.0));
            }
            changed.add(*tta, || "mirroring test-time augmentation".to_string());
            format!("HD-BET deep-learning brain extraction (Isensee et al., 2019{}) of {}", changed.semi(), input_desc)
        }
        _ => format!("{} of {}", section.generator, input_desc),
    }
}

/// HEIDI's sentence, when it ran: after the inversion (each echo's, for a per-echo QSM), or on a
/// two-pass run's combined map.
fn describe_heidi(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let i = &config.inversion;
    if !config.pipeline.do_qsm || !i.heidi.enabled {
        return;
    }
    doc.cite(&CITE_HEIDI);
    // As [`describe_two_pass`] decides whether two-pass ran.
    let two_pass = config.masking.two_pass
        && !(ran.count(Topic::TwoPass, Choice::NotApplied) > 0 && ran.ran(Stage::Mask(Pass::Reliable)) == 0);
    let what = match (i.algorithm, two_pass) {
        (_, true) => "the combined two-pass map".to_string(),
        (QsmAlgorithm::Lsqr, false) => "the LSQR map, as published".to_string(),
        (alg, false) => format!("the {} map", words::inversion(alg)),
    };
    let mut changed = Changed::default();
    changed.add_fields(&i.heidi, &InversionConfig::default().heidi, &["enabled", "te"]);
    if i.heidi.te != InversionConfig::default().heidi.te {
        changed.0.push(format!("the field converted to phase at {}", echo_time_prose(i.heidi.te)));
    }
    let stages: &[Stage] = if two_pass { &[Stage::TwoPassCombination] } else { &[Stage::Inversion(Pass::Main)] };
    doc.say(stages, format!(
        "Streaking was reduced with HEIDI (Homogeneity Enabled Incremental Dipole Inversion) ({}) on {what}: its well-conditioned k-space coefficients \
         were kept, and the dipole cone was filled by total variation weighted by the field's \
         homogeneity{}.",
        cite_inline(&CITE_HEIDI), changed.comma_with()));
}

/// Prose for two-pass artefact reduction, including the reliable-pass mask recipe.
///
/// Named as "two-pass" throughout, matching both the paper and the `_desc-singlepass_` derivative
/// that carries the other pass's map.
fn describe_two_pass(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let not_applied = ran.count(Topic::TwoPass, Choice::NotApplied);
    if !config.masking.two_pass || (not_applied > 0 && ran.ran(Stage::Mask(Pass::Reliable)) == 0) {
        return;
    }
    doc.cite(&CITE_TWOPASS);

    let sections = config.masking.resolved_two_pass_sections();
    let mut descs = Vec::new();
    for section in &sections {
        let mut parts = vec![describe_generator(section, config, doc)];
        let refinements = describe_refinements(&section.refinements, doc);
        if !refinements.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinements)));
        }
        descs.push(parts.join(", "));
    }
    let generated = if descs.len() == 1 {
        format!("generated using {}", descs[0])
    } else {
        let (op, how) = match config.masking.resolved_two_pass_combine() {
            MaskCombine::Or => ("OR", "union"),
            MaskCombine::And => ("AND", "intersection"),
        };
        format!("generated by combining {} mask sections ({op} operation, i.e. their {how}): {}",
                descs.len(), join_list(&descs))
    };
    let refined = describe_refinements(&config.masking.refinements, doc);
    let refined = if refined.is_empty() { String::new() } else { format!(" and refined by {}", join_list(&refined)) };

    if config.masking.two_pass_mode == crate::enums::TwoPassMode::Local {
        let separate_bgremove = !crate::bridge::skips_bgremove(config) && config.bg_removal.algorithm != BfAlgorithm::Iqfm;
        let repeated = if crate::bridge::skips_bgremove(config) { "The reconstruction was" } else { "Background field removal and dipole inversion were" };
        doc.say(&two_pass_steps(), format!(
            "Two-pass artefact reduction (Stewart et al., 2022) was applied with holes only at strong \
             susceptibility sources, taken from a second mask {generated}{refined}: small holes were filled, \
             thin vessel-like holes were separated off by a morphological opening, and only holes with a core \
             of very low phase quality inside the first reconstruction's mask were kept, each with its own \
             rim. {repeated} repeated against this mask.",
        ));
        if separate_bgremove {
            doc.cite(&CITE_LBV);
            doc.say(&two_pass_steps(), "Voxels next to the holes that background field removal left without a value \
                                        were filled by LBV background field removal (Zhou et al., 2014) on the same mask.".to_string());
        }
        doc.say(&two_pass_steps(), "The second susceptibility map was used near the holes and blended linearly \
                                    into the first with distance from them; the first map was used in the holes \
                                    and everywhere else.".to_string());
    } else {
        doc.say(&two_pass_steps(), format!(
            "Two-pass artefact reduction (Stewart et al., 2022) was applied: background field removal \
             and dipole inversion were repeated against a second mask {generated}{refined}, whose holes \
             around strong susceptibility sources were left unfilled, and the two susceptibility maps \
             were combined by taking the second reconstruction wherever it was defined and the first \
             elsewhere.",
        ));
    }
    if not_applied > 0 {
        doc.say(&two_pass_steps(), "It was not applied where the brain mask was sourced from the BIDS derivatives, \
                                    which leaves no recipe for the second mask.".to_string());
    }
}

fn describe_masking(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    if config.masking.sections.is_empty() {
        return;
    }

    // Prefer a bring-your-own mask from BIDS derivatives, where available. Every run having used
    // one, or none, says which.
    let tool = config.masking.custom_mask_tool.as_deref();
    let all_supplied = tool.is_some() && ran.only(Topic::MaskSource, &[Choice::Supplied]);
    let none_supplied = ran.only(Topic::MaskSource, &[Choice::Computed]);
    match tool {
        // One run: its mask was the one supplied with it.
        Some(tool) if all_supplied && ran.runs() == 1 => {
            let from = if tool == "*" { "its".to_string() } else { format!("the '{tool}'") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("The brain mask was supplied with the scan, from {from} BIDS derivatives."));
        }
        Some(tool) if all_supplied => {
            let from = if tool == "*" { String::new() } else { format!("'{tool}' ") };
            doc.say(&[Stage::Mask(Pass::Main)], format!("Brain masks were sourced from the {from}BIDS derivatives."));
        }
        _ if none_supplied => {}
        Some("*") => doc.say(&[Stage::Mask(Pass::Main)], 
            "Where available, existing brain masks were sourced from the BIDS derivatives; \
             otherwise they were generated as follows.".to_string()),
        Some(tool) => doc.say(&[Stage::Mask(Pass::Main)], format!(
            "Where available, brain masks were sourced from the '{}' BIDS derivatives; \
             otherwise they were generated as follows.", tool)),
        None => {}
    }

    // The inhomogeneity correction, when on, and which echoes it corrected: those the mask reads
    // (the RSS for RSS and phase-quality inputs, the first or last echo for those), and every echo
    // for the combined magnitude.
    if config.masking.inhomogeneity_correction {
        let mut changed = Changed::default();
        let correction = match config.masking.inhomogeneity_method {
            InhomogeneityMethod::Makehomogeneous => {
                doc.cite(&CITE_BIAS);
                changed.add_fields(&config.homogeneity, &HomogeneityConfig::default(), &[]);
                "Inhomogeneity correction (Eckstein et al., 2019"
            }
            InhomogeneityMethod::N4 => {
                doc.cite(&CITE_N4);
                let (n4, d) = (&config.n4, N4Config::default());
                if n4.max_iterations != d.max_iterations {
                    let levels: Vec<String> = n4.max_iterations.iter().map(|i| i.to_string()).collect();
                    let n = levels.len();
                    changed.0.push(format!("at most {} iterations at {n} fitting level{}",
                                           join_list(&levels), if n == 1 { "" } else { "s" }));
                }
                changed.add_fields(n4, &d, &["max_iterations"]);
                "N4 inhomogeneity correction (Tustison et al., 2010"
            }
        };
        let settings = changed.semi();

        let inputs: Vec<MaskingInput> = config.masking.sections.iter().map(|s| s.input).collect();
        let uses_rss = inputs.iter().any(|i| matches!(i, MaskingInput::Magnitude | MaskingInput::PhaseQuality));
        let uses_first = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeFirst));
        let uses_last = inputs.iter().any(|i| matches!(i, MaskingInput::MagnitudeLast));

        // Each echo the mask reads is corrected on its own, and every echo when the combined
        // magnitude (the RSS of the corrected echoes) is published.
        let all = uses_rss || config.pipeline.do_magnitude
            || crate::stages::magnitude::refinements_read_magnitude(&config.masking.refinements);
        let what = if all || (uses_first && uses_last) {
            "each echo's magnitude"
        } else if uses_first {
            "the first-echo magnitude"
        } else {
            "the last-echo magnitude"
        };
        doc.say(&[Stage::InhomogeneityCorrection], format!("{correction}{settings}) was applied to {what}."));
    }

    // No run generated a mask: there is no recipe to describe.
    if all_supplied {
        return;
    }

    let section_count = config.masking.sections.len();
    let mut section_descs = Vec::new();

    for section in &config.masking.sections {
        let mut parts = vec![describe_generator(section, config, doc)];

        // Refinements
        let refinement_descs = describe_refinements(&section.refinements, doc);

        if !refinement_descs.is_empty() {
            parts.push(format!("followed by {}", join_list(&refinement_descs)));
        }

        section_descs.push(parts.join(", "));
    }

    if section_count == 1 {
        doc.say(&[Stage::Mask(Pass::Main)], format!("A brain mask was generated using {}.", section_descs[0]));
    } else {
        let (op, how) = match config.masking.combine {
            MaskCombine::Or => ("OR", "union"),
            MaskCombine::And => ("AND", "intersection"),
        };
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "A brain mask was generated by combining {} mask sections ({} operation, i.e. their {}): {}.",
            section_count, op, how, join_list(&section_descs)
        ));
    }

    let combined_descs = describe_refinements(&config.masking.refinements, doc);
    if !combined_descs.is_empty() {
        doc.say(&[Stage::Mask(Pass::Main)], format!(
            "The {} mask was then refined by {}.",
            if section_count == 1 { "resulting" } else { "combined" },
            join_list(&combined_descs)
        ));
    }
    // The scanner's own masking of the phase ([`Topic::ScannerMaskedPhase`]): said when a run had any.
    if ran.count(Topic::ScannerMaskedPhase, Choice::Partial) > 0 {
        doc.say(&[Stage::Mask(Pass::Main)],
            "Voxels where the scanner recorded no phase (a constant value in every echo) were left out of the mask.".to_string());
    }

    // Exactly the consensus recipe — say so, and cite it.
    let consensus = bet_and_phase_mask_recipe();
    if config.masking.sections == consensus.sections
        && config.masking.combine == consensus.combine
        && config.masking.refinements == consensus.refinements
    {
        doc.cite(&CITE_QSM_CONSENSUS);
        doc.say(&[Stage::Mask(Pass::Main)], 
            "This masking follows the recommendation of the ISMRM electro-magnetic tissue \
             properties study group consensus (Bilgic et al., 2024).".to_string());
    }
}

/// Display name + citation for a background-field removal algorithm.
fn bg_removal_name_cite(alg: BfAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        BfAlgorithm::Vsharp => ("V-SHARP", &CITE_VSHARP),
        BfAlgorithm::Pdf => ("PDF", &CITE_PDF),
        BfAlgorithm::Lbv => ("LBV", &CITE_LBV),
        BfAlgorithm::Ismv => ("iSMV", &CITE_ISMV),
        BfAlgorithm::Sharp => ("SHARP", &CITE_SHARP),
        BfAlgorithm::Resharp => ("RESHARP", &CITE_RESHARP),
        BfAlgorithm::Harperella => ("HARPERELLA", &CITE_HARPERELLA),
        BfAlgorithm::Iharperella => ("iHARPERELLA", &CITE_IHARPERELLA),
        BfAlgorithm::Bfrnet => ("BFRnet (deep-learning background removal)", &CITE_BFRNET),
        BfAlgorithm::Iqfm => ("iQFM (deep-learning joint unwrapping + background removal)", &CITE_IQSM),
    }
}

/// What the pipeline graph ([`crate::pipeline_graph`]) says about a step's algorithm, in the
/// methods text's words: so a box and the sentence describing the same step cannot disagree.
pub(crate) mod words {
    use super::*;

    /// Which of `n` echoes `echoes` (1-based, ascending) are: "each echo", "the first echo", "the
    /// last echo", or "echoes 1 and 5".
    pub(crate) fn echoes(echoes: &[usize], n: usize) -> String {
        match echoes {
            e if e.len() == n && n > 1 => "each echo".to_string(),
            [1] => "the first echo".to_string(),
            [e] if *e == n => "the last echo".to_string(),
            [e] => format!("echo {e}"),
            [rest @ .., last] => format!("echoes {} and {last}",
                rest.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")),
            [] => String::new(),
        }
    }

    /// The name the methods text gives an algorithm, without what follows it in parentheses:
    /// "RTS (Rapid Two-Step)" is "RTS" in a box.
    fn short(name: &'static str) -> String {
        name.split(" (").next().unwrap_or(name).to_string()
    }

    pub(crate) fn inversion(alg: QsmAlgorithm) -> String {
        short(inversion_name_cite(alg).0)
    }

    pub(crate) fn bg_removal(alg: BfAlgorithm) -> String {
        short(bg_removal_name_cite(alg).0)
    }

    pub(crate) fn separation(alg: SeparationAlgorithm) -> String {
        short(separation_name_cite(alg).0)
    }

    /// The selected inversion's settings that differ from their defaults (`"λ = 0.002"`).
    pub(crate) fn inversion_settings(config: &PipelineConfig, alg: QsmAlgorithm) -> Vec<String> {
        inversion_changes(config, alg).0
    }

    /// The selected background removal's settings: V-SHARP's kernel radii always (they are what
    /// says which V-SHARP ran), and otherwise what differs from the defaults.
    pub(crate) fn bg_removal_settings(config: &PipelineConfig) -> Vec<String> {
        let b = &config.bg_removal;
        if b.algorithm != BfAlgorithm::Vsharp {
            return bg_removal_changes(config).0;
        }
        let (v, d) = (&b.vsharp, VsharpConfig::default());
        let mut c = Changed::default();
        c.add(true, || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
        c.add(v.deconvolve != d.deconvolve, || format!(
            "truncated-SVD deconvolution {}", if v.deconvolve { "enabled" } else { "disabled" }));
        c.add(v.deconvolve && differs(v.threshold, d.threshold), || format!("deconvolution threshold = {}", v.threshold));
        c.0
    }

    pub(crate) fn separation_settings(config: &PipelineConfig) -> Vec<String> {
        separation_changes(config).0
    }

    pub(crate) fn registration_settings(config: &PipelineConfig) -> Vec<String> {
        registration_changes(config).0
    }

    /// ROMEO's settings that differ from their defaults; Laplacian unwrapping's solver, when it
    /// is not the default one, and the raw phase, when it is unwrapped unmasked; PRELUDE's
    /// partitions and labelling, when they are not the defaults.
    pub(crate) fn unwrapping_settings(config: &PipelineConfig) -> Vec<String> {
        let (fm, d) = (&config.field_mapping, FieldMappingConfig::default());
        let mut c = Changed::default();
        match fm.unwrapping_algorithm {
            UnwrappingAlgorithm::Romeo => c.add_fields(&fm.romeo, &d.romeo, &[]),
            UnwrappingAlgorithm::Laplacian => {
                c.add(fm.laplacian_solver != d.laplacian_solver, || format!("{} solver", fm.laplacian_solver));
                c.add(fm.laplacian_unmasked != d.laplacian_unmasked, || "raw phase over the whole volume".to_string());
                c.add(fm.laplacian_keep_outside != d.laplacian_keep_outside, || "field kept over the whole volume".to_string());
            }
            UnwrappingAlgorithm::Prelude => c = prelude_changes(fm, &d),
            UnwrappingAlgorithm::MediRegionGrowing => {}
        }
        c.0
    }

    /// Each mask section as one line: its input, how the mask is made from it and how it is
    /// refined ("phase quality: Otsu threshold, dilation, hole-filling, erosion").
    pub(crate) fn mask_sections(sections: &[MaskSection]) -> Vec<String> {
        sections.iter().map(|section| {
            let input = match section.input {
                MaskingInput::PhaseQuality => "phase quality",
                MaskingInput::Magnitude => "magnitude",
                MaskingInput::MagnitudeFirst => "first-echo magnitude",
                MaskingInput::MagnitudeLast => "last-echo magnitude",
            };
            let generator = match &section.generator {
                MaskOp::Threshold { method: MaskThresholdMethod::Otsu, .. } => "Otsu threshold".to_string(),
                MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value } => format!("threshold {}", value.unwrap_or(0.5)),
                MaskOp::Threshold { method: MaskThresholdMethod::Percentile, value } =>
                    format!("{}th-percentile threshold", value.unwrap_or(75.0)),
                MaskOp::Threshold { method: MaskThresholdMethod::StiGui, .. } => "STI Suite threshold".to_string(),
                MaskOp::Bet { .. } => "BET".to_string(),
                MaskOp::HdBet { .. } => "HD-BET".to_string(),
                MaskOp::Rs2Net { .. } => "RS2-Net".to_string(),
                MaskOp::MindGrab { .. } => "MindGrab".to_string(),
                other => describe_refinements(std::slice::from_ref(other), &mut Draft::default()).join(", "),
            };
            let steps: Vec<String> = std::iter::once(generator)
                .chain(describe_refinements(&section.refinements, &mut Draft::default()))
                .collect();
            format!("{input}: {}", steps.join(", "))
        }).collect()
    }

    /// QSMART's settings that differ from their defaults (its inner inversion is said apart).
    pub(crate) fn qsmart_settings(config: &PipelineConfig) -> Vec<String> {
        let mut c = qsmart_changes(config);
        c.add(config.inversion.qsmart.vessel_magnitude == QsmartVesselMagnitude::Combined,
            || "vessels on the combined magnitude".to_string());
        c.0
    }
}

/// The network that reconstructs from the wrapped phase (iQFM, iQSM or iQSM+) a run uses, by
/// name, if any. Tiled, it only approximates its whole-volume result.
pub(crate) fn tiled_phase_network(config: &PipelineConfig) -> Option<&'static str> {
    match config.inversion.algorithm {
        QsmAlgorithm::Iqsm => Some("iQSM"),
        QsmAlgorithm::IqsmPlus => Some("iQSM+"),
        _ if config.bg_removal.algorithm == BfAlgorithm::Iqfm
            && !crate::bridge::skips_bgremove(config) => Some("iQFM"),
        _ => None,
    }
}

/// The sentence saying the networks ran on overlapping tiles, when `inversion.tile_size` is set.
/// The halo is said only when it is not the default the runner fills in. For iQFM, iQSM and
/// iQSM+ it also says that this approximates the whole-volume result.
fn say_tiling(config: &PipelineConfig, stage: fn(Pass) -> Stage, doc: &mut Draft) {
    let Some(core) = config.inversion.tile_size else { return };
    let halo = match config.inversion.tile_halo {
        Some(h) if h != DEFAULT_TILE_HALO => format!(", halo {h} voxels"),
        _ => String::new(),
    };
    doc.say(&both(stage), match tiled_phase_network(config) {
        Some(network) => format!(
            "{network} was run on overlapping tiles (tile size {core} voxels{halo}), which \
             approximates its whole-volume result.",
        ),
        None => format!("Network inference was run on overlapping tiles (tile size {core} voxels{halo})."),
    });
}

/// Display name + citation for a dipole inversion algorithm. Shared by the
/// standard pipeline's inversion sentence and QSMART's inner inversion.
/// Tgv/Qsmart are not valid dipole inversions; they fall back to iLSQR
/// defensively (not reached in practice).
fn inversion_name_cite(alg: QsmAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        QsmAlgorithm::Rts => ("RTS (Rapid Two-Step)", &CITE_RTS),
        QsmAlgorithm::Tv => ("Total Variation (TV-ADMM)", &CITE_TV),
        QsmAlgorithm::Tkd => ("TKD (Thresholded K-space Division)", &CITE_TKD),
        QsmAlgorithm::Tsvd => ("TSVD (Truncated Singular Value Decomposition)", &CITE_TKD),
        QsmAlgorithm::Tikhonov => ("Tikhonov regularization", &CITE_TIKHONOV),
        QsmAlgorithm::Nltv => ("NLTV (Nonlinear Total Variation)", &CITE_NLTV),
        QsmAlgorithm::Medi => ("MEDI (Morphology Enabled Dipole Inversion)", &CITE_MEDI),
        QsmAlgorithm::Tfi => ("TFI (Total Field Inversion)", &CITE_TFI),
        QsmAlgorithm::Ilsqr => ("iLSQR", &CITE_ILSQR),
        QsmAlgorithm::Star => ("STAR-QSM (Streaking Artifact Reduction)", &CITE_STAR),
        QsmAlgorithm::Lsqr => ("minimally regularised LSQR", &CITE_LSQR),
        QsmAlgorithm::Ndi => ("NDI (Nonlinear Dipole Inversion)", &CITE_NDI),
        QsmAlgorithm::Fansi => ("FANSI (Nonlinear TV)", &CITE_NLTV),
        QsmAlgorithm::FansiTgv => ("FANSI (Nonlinear TGV)", &CITE_NLTV),
        QsmAlgorithm::L1qsm => ("L1-QSM (L1 Data Fidelity)", &CITE_L1QSM),
        QsmAlgorithm::Whqsm => ("WH-QSM (Weak-Harmonic)", &CITE_WHQSM),
        QsmAlgorithm::Hdqsm => ("HD-QSM (Hybrid Data Fidelity)", &CITE_HDQSM),
        QsmAlgorithm::AmpPe => ("AMP-PE (Approximate Message Passing with Parameter Estimation)", &CITE_AMPPE),
        QsmAlgorithm::Xqsm => ("xQSM (deep-learning dipole inversion)", &CITE_XQSM),
        QsmAlgorithm::Qsmnet => ("QSMnet (deep-learning dipole inversion)", &CITE_QSMNET),
        QsmAlgorithm::QsmnetPlus => ("QSMnet+ (deep-learning dipole inversion)", &CITE_QSMNET_PLUS),
        QsmAlgorithm::Qsmnetme => ("QSMnetME (deep-learning dipole inversion)", &CITE_CHISEPNET),
        QsmAlgorithm::Autoqsm => ("AutoQSM (single-step deep-learning reconstruction)", &CITE_AUTOQSM),
        QsmAlgorithm::Qsmgan => ("QSMGAN (GAN-refined deep-learning dipole inversion)", &CITE_QSMGAN),
        QsmAlgorithm::Ir2qsm => ("IR2QSM (unrolled deep-learning dipole inversion)", &CITE_IR2QSM),
        QsmAlgorithm::Lpcnn => ("LPCNN (learned-proximal deep-learning dipole inversion)", &CITE_LPCNN),
        QsmAlgorithm::ModlQsm => ("MoDL-QSM (model-based deep-learning dipole inversion)", &CITE_MODLQSM),
        QsmAlgorithm::Nextqsm => ("NeXtQSM (single-step deep-learning reconstruction)", &CITE_NEXTQSM),
        QsmAlgorithm::Iqsm => ("iQSM (end-to-end deep-learning reconstruction from phase)", &CITE_IQSM),
        QsmAlgorithm::IqsmPlus => ("iQSM+ (orientation-adaptive end-to-end deep-learning reconstruction)", &CITE_IQSM_PLUS),
        QsmAlgorithm::Tgv | QsmAlgorithm::Qsmart => ("iLSQR", &CITE_ILSQR),
    }
}

/// The χ-separation method's name as the methods text gives it.
pub(crate) fn separation_name(alg: SeparationAlgorithm) -> &'static str {
    separation_name_cite(alg).0
}

fn separation_name_cite(alg: SeparationAlgorithm) -> (&'static str, &'static Citation) {
    match alg {
        SeparationAlgorithm::R2starQsm => ("R2*-QSM susceptibility source separation", &CITE_R2STARQSM),
        SeparationAlgorithm::Decompose => ("DECOMPOSE-QSM", &CITE_DECOMPOSE),
        SeparationAlgorithm::ChiSepIlsqr => ("χ-separation (iLSQR)", &CITE_CHISEP),
        SeparationAlgorithm::ChiSepMedi => ("χ-separation (MEDI)", &CITE_CHISEP),
        SeparationAlgorithm::WaveSep => ("WaveSep", &CITE_WAVESEP),
        SeparationAlgorithm::HcChisep => ("hollow-cylinder χ-separation (HC-ChiSep)", &CITE_HCCHISEP),
        SeparationAlgorithm::SusepNet => ("SUSEP-Net (deep-learning source separation)", &CITE_SUSEPNET),
        SeparationAlgorithm::ChiSepNet => ("χ-sepnet (deep-learning source separation)", &CITE_CHISEPNET),
    }
}

fn join_list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

/// `text` (a [`Methods::to_markdown`]) without its `## Note` section.
#[cfg(test)]
fn without_note(text: &str) -> String {
    match text.find("## Note\n") {
        Some(start) => {
            let end = text[start..].find("## References").map_or(text.len(), |e| start + e);
            format!("{}{}", &text[..start], &text[end..])
        }
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod masking_tests;

#[cfg(test)]
mod multiorient_methods_tests;

/// Parameter values are stated only when they differ from QSMxT's defaults.
#[cfg(test)]
mod defaults_only_tests;

#[cfg(test)]
mod structured_tests;
