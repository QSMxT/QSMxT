//! What each enabled output needs from a run's data, checked before any processing.
//!
//! The policy (#193, #273): an output the user asked for that this run's data cannot produce is
//! an **error**, raised for every run before the first one starts, naming the run, the output and
//! what is missing. An output another option turned on (chi-separation turning on R2*, the
//! statistics turning on segmentation, ...) is a **warning** that names the option that implied
//! it, and the run continues without it. Nothing is skipped silently.
//!
//! The same functions gate the stages in the runner, so the check and the processing cannot
//! disagree about what a run can do.

use std::path::PathBuf;

use crate::bids::discovery::QsmRun;
use crate::pipeline::config::*;

/// How far echo spacings may deviate, in seconds, before ARLO's single-spacing Simpson integral
/// or the EPG dictionary's `TEₙ = n·ESP` train no longer describe the acquisition: 0.1 ms. It
/// covers the rounding of echo times in DICOM headers and BIDS sidecars, not genuinely unequal
/// spacing. The same tolerance QSM.rs applies (`ARLO_TE_SPACING_TOLERANCE_S`,
/// `EPG_TE_TOLERANCE_S`).
pub const TE_SPACING_TOLERANCE_S: f64 = 1e-4;

/// An output the pipeline can be asked for, or that another can turn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Swi,
    Smwi,
    Segmentation,
    T2star,
    R2star,
    R2,
    R2prime,
    ChiSeparation,
    Analysis,
    RegionReference,
}

impl Output {
    /// The option that asks for it, as the user would type it.
    pub fn flag(self) -> &'static str {
        match self {
            Output::Swi => "--do-swi",
            Output::Smwi => "--do-smwi",
            Output::Segmentation => "--do-segmentation",
            Output::T2star => "--do-t2starmap",
            Output::R2star => "--do-r2starmap",
            Output::R2 => "--do-r2map",
            Output::R2prime => "--do-r2primemap",
            Output::ChiSeparation => "--do-chisep",
            Output::Analysis => "--do-analysis",
            Output::RegionReference => "--qsm-reference <region>",
        }
    }

    fn enabled(self, config: &PipelineConfig) -> bool {
        let p = &config.pipeline;
        match self {
            Output::Swi => p.do_swi,
            Output::Smwi => p.do_smwi,
            Output::Segmentation => p.do_segmentation,
            Output::T2star => p.do_t2starmap,
            Output::R2star => p.do_r2starmap,
            Output::R2 => p.do_r2map,
            Output::R2prime => p.do_r2primemap,
            Output::ChiSeparation => p.do_chi_separation,
            Output::Analysis => p.do_analysis,
            Output::RegionReference => p.do_qsm && config.qsm.reference == QsmReference::Region,
        }
    }

    /// Every output, in the order the checks report them: an output before the ones built on it.
    const ALL: [Output; 10] = [
        Output::Swi, Output::Segmentation, Output::T2star, Output::R2star, Output::R2,
        Output::R2prime, Output::ChiSeparation, Output::Smwi, Output::Analysis,
        Output::RegionReference,
    ];
}

/// The option that turned `out` on, when another one did; `None` when only the user can have.
///
/// Decided from the final config rather than remembered from how it was built, so a config the
/// TUI saved with the implied flags already written into it reads the same as the command line
/// that produced it. An output that is both asked for and implied counts as implied: whatever
/// implies it is itself checked, and fails if it was asked for and cannot be made.
pub fn implied_by(config: &PipelineConfig, out: Output) -> Option<String> {
    let p = &config.pipeline;
    let s = &config.separation;
    let chisep = || format!("--do-chisep (--chisep {})", s.algorithm);
    let r2prime_based = matches!(s.algorithm,
        SeparationAlgorithm::ChiSepIlsqr | SeparationAlgorithm::ChiSepMedi
        | SeparationAlgorithm::WaveSep | SeparationAlgorithm::HcChisep
        | SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet);
    let computes_r2prime = p.do_r2primemap && s.custom_r2prime_tool.is_none();
    match out {
        Output::Segmentation if config.segmentation.custom_dseg_tool.is_none() => {
            if p.do_analysis {
                Some("--do-analysis".into())
            } else if Output::RegionReference.enabled(config) {
                Some(format!("--qsm-reference {}",
                    config.qsm.reference_region.as_deref().unwrap_or("<region>")))
            } else {
                None
            }
        }
        Output::R2star => {
            if p.do_chi_separation && s.algorithm == SeparationAlgorithm::R2starQsm {
                Some(chisep())
            } else if computes_r2prime {
                Some(implied_by(config, Output::R2prime).unwrap_or_else(|| "--do-r2primemap".into()))
            } else {
                None
            }
        }
        Output::R2 if computes_r2prime && s.custom_r2_tool.is_none()
            && s.r2prime_strategy != R2PrimeStrategy::R2primenet =>
        {
            Some(implied_by(config, Output::R2prime).unwrap_or_else(|| "--do-r2primemap".into()))
        }
        Output::R2prime if p.do_chi_separation && r2prime_based && s.custom_r2prime_tool.is_none() => {
            Some(chisep())
        }
        _ => None,
    }
}

/// The facts about one run that decide what it can produce. Built once from the discovered run;
/// the bring-your-own derivatives the config names are looked up here, not per stage.
pub struct RunFacts<'a> {
    pub n_echoes: usize,
    pub has_magnitude: bool,
    pub echo_times: &'a [f64],
    pub n_coils: usize,
    pub mese_echo_times: Option<&'a [f64]>,
    pub field_strength: f64,
    pub custom_dseg: Option<PathBuf>,
    pub custom_qsm: Option<PathBuf>,
    pub custom_r2: Option<PathBuf>,
    pub custom_r2prime: Option<PathBuf>,
}

impl<'a> RunFacts<'a> {
    pub fn of(run: &'a QsmRun, config: &PipelineConfig) -> Self {
        use crate::pipeline::runner::find_custom_derivative as find;
        let s = &config.separation;
        RunFacts {
            n_echoes: run.echoes.len(),
            has_magnitude: run.has_magnitude,
            echo_times: &run.echo_times,
            n_coils: run.coils.as_ref().map_or(1, |c| c.len()),
            mese_echo_times: run.mese.as_ref().map(|m| m.echo_times.as_slice()),
            field_strength: run.magnetic_field_strength,
            custom_dseg: config.segmentation.custom_dseg_tool.as_deref()
                .and_then(|t| find(run, t, "*_dseg.nii*", &[])),
            custom_qsm: s.custom_qsm_tool.as_deref()
                .and_then(|t| find(run, t, "*_Chimap.nii*", &["desc-"])),
            custom_r2: s.custom_r2_tool.as_deref()
                .and_then(|t| find(run, t, "*_R2map.nii*", &[])),
            custom_r2prime: s.custom_r2prime_tool.as_deref()
                .and_then(|t| find(run, t, "*_R2primemap.nii*", &[])),
        }
    }
}

fn ms(tes: &[f64]) -> String {
    let v: Vec<String> = tes.iter().map(|t| format!("{:.2}", t * 1e3)).collect();
    format!("[{}] ms", v.join(", "))
}

/// Largest deviation of a spacing from `reference` (the mean spacing, or the first one).
fn spacing_deviation(tes: &[f64], first_as_reference: bool) -> (f64, f64) {
    let mut sorted = tes.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let diffs: Vec<f64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    let reference = if first_as_reference {
        diffs[0]
    } else {
        diffs.iter().sum::<f64>() / diffs.len() as f64
    };
    let dev = diffs.iter().map(|d| (d - reference).abs()).fold(0.0, f64::max);
    (reference, dev)
}

/// Why ARLO cannot fit R2* on this run's GRE echoes, if it cannot.
pub fn arlo_problem(f: &RunFacts) -> Option<String> {
    if !f.has_magnitude {
        return Some("needs magnitude images, and this run has none".into());
    }
    if f.n_echoes < 3 {
        return Some(format!("needs at least 3 echoes, and this run has {}", f.n_echoes));
    }
    if !qsm_core::utils::use_arlo(f.echo_times, TE_SPACING_TOLERANCE_S) {
        let (_, dev) = spacing_deviation(f.echo_times, false);
        return Some(format!(
            "needs equally spaced echo times (ARLO integrates with one spacing), and this run's \
             echo times {} have spacings {:.3} ms apart",
            ms(f.echo_times), dev * 1e3));
    }
    None
}

/// Why the EPG fit cannot measure R2 from this run's MESE acquisition, if it cannot.
pub fn mese_problem(f: &RunFacts) -> Option<String> {
    let Some(tes) = f.mese_echo_times else {
        return Some("needs a multi-echo spin-echo (MESE) acquisition, and this run has none".into());
    };
    if tes.len() < 3 {
        return Some(format!(
            "needs at least 3 spin echoes for the EPG fit, and the MESE has {}", tes.len()));
    }
    let (esp, dev) = spacing_deviation(tes, true);
    if dev > TE_SPACING_TOLERANCE_S {
        return Some(format!(
            "the EPG fit needs equally spaced spin echoes, and the MESE's echo times {} are not",
            ms(tes)));
    }
    let first = tes.iter().copied().fold(f64::INFINITY, f64::min);
    if (first - esp).abs() > TE_SPACING_TOLERANCE_S {
        return Some(format!(
            "the EPG fit models echoes at TEₙ = n·ESP, and the MESE's first echo ({:.2} ms) is not \
             at its echo spacing ({:.2} ms)", first * 1e3, esp * 1e3));
    }
    None
}

/// Why R2 cannot be had for this run, if it cannot.
pub fn r2_problem(f: &RunFacts, config: &PipelineConfig) -> Option<String> {
    match config.separation.custom_r2_tool.as_deref() {
        Some(tool) if f.custom_r2.is_none() => Some(format!(
            "--use-custom-r2 {tool} names no R2 map for this run under derivatives/{tool}")),
        Some(_) => None,
        None => mese_problem(f),
    }
}

/// Why R2' cannot be had for this run, if it cannot.
pub fn r2prime_problem(f: &RunFacts, config: &PipelineConfig) -> Option<String> {
    let s = &config.separation;
    if let Some(tool) = s.custom_r2prime_tool.as_deref() {
        return f.custom_r2prime.is_none().then(|| format!(
            "--use-custom-r2prime {tool} names no R2' map for this run under derivatives/{tool}"));
    }
    if let Some(why) = arlo_problem(f) {
        return Some(format!("needs R2*, which {why}"));
    }
    let r2 = r2_problem(f, config);
    match s.r2prime_strategy {
        R2PrimeStrategy::Mese => r2.map(|why| format!(
            "--r2prime-strategy mese measures R2' as R2* − R2, and R2 {why}")),
        R2PrimeStrategy::Auto if r2.is_none() => None,
        strategy => {
            let field = crate::pipeline::runner::r2primenet_off_field(
                f.field_strength, strategy, s.r2primenet.ignore_field_mismatch)?;
            Some(match (strategy, r2) {
                (R2PrimeStrategy::Auto, Some(r2)) => format!(
                    "has no R2 to subtract (R2 {r2}), so R2PRIMEnet would estimate it, but {field}; \
                     pass --r2primenet-ignore-field-mismatch to run it anyway (uncalibrated)"),
                _ => format!(
                    "--r2prime-strategy r2primenet: {field}; pass \
                     --r2primenet-ignore-field-mismatch to run it anyway (uncalibrated)"),
            })
        }
    }
}

/// Why a segmentation cannot be had for this run, if it cannot.
pub fn segmentation_problem(f: &RunFacts) -> Option<String> {
    if f.custom_dseg.is_some() || f.has_magnitude {
        return None;
    }
    Some("needs magnitude images to segment (or a dseg via --use-custom-dseg), and this run has no \
          magnitude".into())
}

/// Why chi-separation cannot run on this run, if it cannot.
pub fn chisep_problem(f: &RunFacts, config: &PipelineConfig) -> Option<String> {
    let s = &config.separation;
    if let Some(tool) = s.custom_qsm_tool.as_deref() {
        if f.custom_qsm.is_none() {
            return Some(format!(
                "--use-custom-qsm {tool} names no Chimap for this run under derivatives/{tool}"));
        }
    } else if !config.pipeline.do_qsm {
        return Some("needs a susceptibility map: drop --no-qsm or pass --use-custom-qsm".into());
    }
    let magnitude = || (!f.has_magnitude)
        .then(|| "needs magnitude images, and this run has none".to_string());
    let r2prime = || r2prime_problem(f, config).map(|why| format!("needs R2', which {why}"));
    // The local field is written by background-field removal, which only the field-based
    // inversions run, and only when the pipeline reconstructs χ itself.
    let local_field = || {
        if s.custom_qsm_tool.is_some() && !config.pipeline.do_qsm {
            Some("needs the local field, which a bring-your-own QSM does not come with".to_string())
        } else if crate::pipeline::runner::skips_bgremove(config) {
            Some(format!("needs the local field, and --qsm-algorithm {} has no background-field \
                removal step to produce one", config.inversion.algorithm))
        } else {
            None
        }
    };
    match s.algorithm {
        // Fits R2* itself from the magnitude when no R2* map is given, with ARLO either way.
        SeparationAlgorithm::R2starQsm => arlo_problem(f).map(|why| format!("needs R2*, which {why}")),
        SeparationAlgorithm::Decompose => magnitude(),
        SeparationAlgorithm::ChiSepIlsqr | SeparationAlgorithm::ChiSepMedi =>
            local_field().or_else(r2prime).or_else(magnitude),
        SeparationAlgorithm::HcChisep => r2prime().or_else(magnitude),
        SeparationAlgorithm::SusepNet | SeparationAlgorithm::ChiSepNet =>
            local_field().or_else(r2prime),
        SeparationAlgorithm::WaveSep => r2prime(),
    }
}

/// Why this run cannot produce `out`, if it cannot.
pub fn problem(f: &RunFacts, config: &PipelineConfig, out: Output) -> Option<String> {
    match out {
        Output::Swi => (!f.has_magnitude)
            .then(|| "needs magnitude images, and this run has none".into()),
        Output::Smwi => match config.separation.custom_qsm_tool.as_deref() {
            _ if !f.has_magnitude => Some("needs magnitude images, and this run has none".into()),
            Some(tool) if !config.pipeline.do_qsm && f.custom_qsm.is_none() => Some(format!(
                "needs a susceptibility map, and --use-custom-qsm {tool} names no Chimap for this \
                 run under derivatives/{tool}")),
            _ => None,
        },
        Output::Segmentation => segmentation_problem(f),
        Output::T2star | Output::R2star => arlo_problem(f),
        Output::R2 => r2_problem(f, config),
        Output::R2prime => r2prime_problem(f, config),
        Output::ChiSeparation => chisep_problem(f, config),
        Output::Analysis | Output::RegionReference => segmentation_problem(f)
            .map(|why| format!("needs a segmentation, which {why}")),
    }
}

/// Whether the runner should run the stage for `out` on this run: enabled, and possible.
pub fn wanted(f: &RunFacts, config: &PipelineConfig, out: Output) -> bool {
    out.enabled(config) && problem(f, config, out).is_none()
}

/// Problems that stop a run whatever was asked for.
fn input_problems(f: &RunFacts) -> Option<String> {
    (f.n_coils > 1 && f.n_echoes < 2).then(|| format!(
        "MCPC-3D-S coil combination needs at least 2 echoes, and this run has {} echo with {} \
         uncombined coils. Combine the coils first (on the scanner, or with another tool).",
        f.n_echoes, f.n_coils))
}

/// The result of checking every run: one error line per requested output a run cannot produce,
/// one warning per implied output it cannot.
#[derive(Debug, Default)]
pub struct Check {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Check one run against the config.
pub fn check_run(run: &QsmRun, config: &PipelineConfig, out: &mut Check) {
    let f = RunFacts::of(run, config);
    if let Some(why) = input_problems(&f) {
        out.errors.push(format!("{}: {why}", run.key));
    }
    for o in Output::ALL {
        if !o.enabled(config) {
            continue;
        }
        let Some(why) = problem(&f, config, o) else { continue };
        match implied_by(config, o) {
            Some(by) => out.warnings.push(format!(
                "{}: {} (turned on by {by}) is skipped: it {why}", run.key, o.flag())),
            None => out.errors.push(format!("{}: {} {why}", run.key, o.flag())),
        }
    }
}

/// Check every run before any is processed. Warnings are logged; errors, all of them at once,
/// fail the command.
pub fn check_runs(runs: &[QsmRun], config: &PipelineConfig) -> crate::Result<()> {
    let mut check = Check::default();
    for run in runs {
        check_run(run, config, &mut check);
    }
    for w in &check.warnings {
        log::warn!("{w}");
    }
    if check.errors.is_empty() {
        return Ok(());
    }
    Err(crate::error::QsmxtError::Config(format!(
        "{} requested output(s) cannot be produced from the data, so nothing was processed:\n  {}\n\
         Drop the option, or narrow the runs with --include/--exclude.",
        check.errors.len(), check.errors.join("\n  "))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::discovery::{EchoFiles, MeseRun, QsmRun};
    use crate::bids::entities::AcquisitionKey;

    fn key() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".into(), session: None, acquisition: None, reconstruction: None,
            inversion: None, run: None, suffix: "MEGRE".into(),
        }
    }

    fn run(echo_times: &[f64], has_magnitude: bool) -> QsmRun {
        QsmRun {
            key: key(),
            coils: None,
            echoes: (1..=echo_times.len()).map(|n| EchoFiles {
                echo_number: n as u32,
                phase_json: PathBuf::from("p.json"),
                phase_nifti: PathBuf::from("/nonexistent/sub-01/anat/p.nii"),
                magnitude_json: None,
                magnitude_nifti: None,
            }).collect(),
            magnetic_field_strength: 3.0,
            echo_times: echo_times.to_vec(),
            b0_dir: None,
            dims: (4, 4, 4),
            has_magnitude,
            mese: None,
        }
    }

    fn with_mese(mut r: QsmRun, tes: &[f64]) -> QsmRun {
        let mut k = key();
        k.suffix = "MESE".into();
        r.mese = Some(MeseRun {
            key: k, magnitude_niftis: vec![], echo_times: tes.to_vec(), dims: (4, 4, 4),
            affine: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        });
        r
    }

    const EVEN: [f64; 4] = [0.005, 0.010, 0.015, 0.020];

    fn check(r: &QsmRun, c: &PipelineConfig) -> Check {
        let mut out = Check::default();
        check_run(r, c, &mut out);
        out
    }

    /// The final config, as `apply_run_overrides` leaves it.
    fn enforced(mut c: PipelineConfig) -> PipelineConfig {
        qsmxt_config::config::enforce_analysis_dependencies(&mut c);
        qsmxt_config::config::enforce_smwi_dependencies(&mut c);
        qsmxt_config::config::enforce_separation_dependencies(&mut c);
        qsmxt_config::config::enforce_reference_dependencies(&mut c);
        c
    }

    #[test]
    fn swi_on_a_phase_only_run_is_refused() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        let out = check(&run(&EVEN, false), &c);
        assert_eq!(out.errors.len(), 1, "{out:?}");
        assert!(out.errors[0].contains("sub-01") && out.errors[0].contains("--do-swi")
            && out.errors[0].contains("magnitude"), "{}", out.errors[0]);
    }

    #[test]
    fn smwi_on_a_phase_only_run_is_refused() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_smwi = true;
        let out = check(&run(&EVEN, false), &enforced(c));
        assert!(out.errors.iter().any(|e| e.contains("--do-smwi")), "{out:?}");
    }

    /// The statistics imply the segmentation, so that is the warning, naming why; the statistics
    /// themselves were asked for, so they are the error.
    #[test]
    fn analysis_on_a_phase_only_run_warns_about_segmentation_and_refuses_the_analysis() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_analysis = true;
        let out = check(&run(&EVEN, false), &enforced(c));
        assert_eq!(out.warnings.len(), 1, "{out:?}");
        assert!(out.warnings[0].contains("--do-segmentation")
            && out.warnings[0].contains("turned on by --do-analysis")
            && out.warnings[0].contains("magnitude"), "{}", out.warnings[0]);
        assert_eq!(out.errors.len(), 1, "{out:?}");
        assert!(out.errors[0].contains("--do-analysis"), "{}", out.errors[0]);
    }

    #[test]
    fn segmentation_asked_for_on_a_phase_only_run_is_an_error() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_segmentation = true;
        let out = check(&run(&EVEN, false), &enforced(c));
        assert!(out.errors.iter().any(|e| e.contains("--do-segmentation")), "{out:?}");
        assert!(out.warnings.is_empty(), "{out:?}");
    }

    #[test]
    fn r2star_needs_three_equally_spaced_echoes_with_magnitude() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2starmap = true;
        let two = check(&run(&[0.005, 0.010], true), &c);
        assert!(two.errors.len() == 1 && two.errors[0].contains("at least 3 echoes"), "{two:?}");
        // A long echo appended to an even train: ARLO would integrate it with the wrong spacing.
        let uneven = check(&run(&[0.005, 0.010, 0.015, 0.030], true), &c);
        assert!(uneven.errors.len() == 1 && uneven.errors[0].contains("equally spaced"), "{uneven:?}");
        let ok = check(&run(&EVEN, true), &c);
        assert!(ok.errors.is_empty() && ok.warnings.is_empty(), "{ok:?}");
        // Rounding in a sidecar is not unequal spacing.
        let rounded = check(&run(&[0.00492, 0.00984, 0.01477, 0.01968], true), &c);
        assert!(rounded.errors.is_empty(), "{rounded:?}");
    }

    #[test]
    fn t2star_is_refused_on_the_same_terms() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_t2starmap = true;
        let out = check(&run(&[0.005, 0.010, 0.030], true), &c);
        assert!(out.errors.iter().any(|e| e.contains("--do-t2starmap")), "{out:?}");
    }

    /// R2*-based chi-separation turns R2* on: R2* is then a warning, and chi-separation, which
    /// was asked for, is the error.
    #[test]
    fn r2star_qsm_chisep_on_unequal_echoes_is_refused_and_its_r2star_warned() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::R2starQsm;
        let out = check(&run(&[0.005, 0.010, 0.030], true), &enforced(c));
        assert!(out.warnings.iter().any(|w| w.contains("--do-r2starmap")
            && w.contains("turned on by --do-chisep")), "{out:?}");
        assert!(out.errors.len() == 1 && out.errors[0].contains("--do-chisep")
            && out.errors[0].contains("equally spaced"), "{out:?}");
    }

    #[test]
    fn epg_needs_an_equally_spaced_mese_starting_at_its_echo_spacing() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2map = true;
        let no_mese = check(&run(&EVEN, true), &c);
        assert!(no_mese.errors.len() == 1 && no_mese.errors[0].contains("MESE"), "{no_mese:?}");
        let offset = check(&with_mese(run(&EVEN, true), &[0.015, 0.025, 0.035, 0.045]), &c);
        assert!(offset.errors.len() == 1 && offset.errors[0].contains("TEₙ = n·ESP"), "{offset:?}");
        let uneven = check(&with_mese(run(&EVEN, true), &[0.010, 0.020, 0.040]), &c);
        assert!(uneven.errors.len() == 1 && uneven.errors[0].contains("equally spaced"), "{uneven:?}");
        let ok = check(&with_mese(run(&EVEN, true), &[0.010, 0.020, 0.030, 0.040]), &c);
        assert!(ok.errors.is_empty(), "{ok:?}");
    }

    /// With no MESE, `auto` estimates R2' with R2PRIMEnet: the R2 that R2'-based separation turned
    /// on is skipped with a warning, and the separation itself can still run at 3 T.
    #[test]
    fn r2prime_chisep_without_a_mese_warns_about_r2_and_runs_on_r2primenet_at_3t() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        let c = enforced(c);
        let out = check(&run(&EVEN, true), &c);
        assert!(out.errors.is_empty(), "{out:?}");
        assert!(out.warnings.len() == 1 && out.warnings[0].contains("--do-r2map"), "{out:?}");

        // At 7 T R2PRIMEnet is off its training field, so there is no R2' at all.
        let mut seven = run(&EVEN, true);
        seven.magnetic_field_strength = 7.0;
        let out = check(&seven, &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--do-chisep"), "{out:?}");
        assert!(out.warnings.iter().any(|w| w.contains("--do-r2primemap")), "{out:?}");
    }

    #[test]
    fn mese_strategy_without_a_mese_refuses_r2prime_asked_for_by_name() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        let out = check(&run(&EVEN, true), &enforced(c));
        assert!(out.errors.iter().any(|e| e.contains("--do-r2primemap")
            && e.contains("--r2prime-strategy mese")), "{out:?}");
    }

    #[test]
    fn a_named_custom_derivative_that_is_not_there_is_an_error() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2map = true;
        c.separation.custom_r2_tool = Some("mytool".into());
        let out = check(&run(&EVEN, true), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--use-custom-r2 mytool"), "{out:?}");
    }

    #[test]
    fn a_single_echo_uncombined_coil_run_is_refused_whatever_was_asked() {
        let mut r = run(&[0.005], true);
        r.coils = Some(vec![
            crate::bids::discovery::CoilFiles { coil_number: 1, echoes: r.echoes.clone() },
            crate::bids::discovery::CoilFiles { coil_number: 2, echoes: r.echoes.clone() },
        ]);
        let out = check(&r, &PipelineConfig::default());
        assert!(out.errors.len() == 1 && out.errors[0].contains("MCPC-3D-S")
            && out.errors[0].contains("2 uncombined coils"), "{out:?}");
    }

    #[test]
    fn check_runs_reports_every_problem_at_once() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        c.pipeline.do_r2starmap = true;
        let err = check_runs(&[run(&[0.005, 0.010], false)], &c).unwrap_err().to_string();
        assert!(err.contains("--do-swi") && err.contains("--do-r2starmap"), "{err}");
        assert!(check_runs(&[run(&EVEN, true)], &c).is_ok());
    }

    /// χ-sep-iLSQR fits the local field, which an end-to-end inversion never writes.
    #[test]
    fn field_based_chisep_needs_an_inversion_with_background_removal() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        let out = check(&with_mese(run(&EVEN, true), &[0.01, 0.02, 0.03]), &enforced(c));
        assert!(out.errors.len() == 1 && out.errors[0].contains("local field"), "{out:?}");
    }
}
