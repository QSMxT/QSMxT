//! What each enabled output needs from a run's data, checked before any processing.
//!
//! The policy (#193, #273): an output the user asked for that this run's data cannot produce is
//! an **error**, raised for every run before the first one starts, naming the run, the output and
//! what is missing. An output another option turned on (chi-separation turning on R2*, the
//! statistics turning on segmentation, ...) is a **warning** that names the option that implied
//! it, and the run continues without it. Nothing is skipped silently.
//!
//! The same functions gate the stages in `qsmxt`'s runner, so the check and the processing cannot
//! disagree about what a run can do.

use crate::config::*;
use crate::enums::*;
use crate::error::EngineError;
use crate::scan::ScanFacts;

/// How far spin-echo spacings may deviate, in seconds, before the EPG dictionary's `TEₙ = n·ESP`
/// train no longer describes the acquisition: QSM.rs's own tolerance (0.1 ms, the rounding of
/// echo times in DICOM headers and BIDS sidecars), so a run refused here is exactly one QSM.rs
/// would refuse with an `InputError` mid-run.
const EPG_TOLERANCE_S: f64 = qsm_core::utils::EPG_TE_TOLERANCE_S;

/// How far an echo spacing may differ from the first, relative to it, before the noise map of
/// MEDI, TFI and PDF (MEDI toolbox `Fit_ppm_complex`, which assumes one spacing) is refused: the
/// tolerance of qsm-core's dispatcher (`pipeline::inversion::echo_magnitudes`).
const FIT_PPM_COMPLEX_SPACING_TOLERANCE: f64 = 1e-3;

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

    /// The setting that asks for it, by its path in the configuration: what a front end that
    /// shows settings rather than flags names.
    pub fn setting(self) -> &'static str {
        match self {
            Output::Swi => "pipeline.do_swi",
            Output::Smwi => "pipeline.do_smwi",
            Output::Segmentation => "pipeline.do_segmentation",
            Output::T2star => "pipeline.do_t2starmap",
            Output::R2star => "pipeline.do_r2starmap",
            Output::R2 => "pipeline.do_r2map",
            Output::R2prime => "pipeline.do_r2primemap",
            Output::ChiSeparation => "pipeline.do_chi_separation",
            Output::Analysis => "pipeline.do_analysis",
            Output::RegionReference => "qsm.reference",
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

/// The setting whose option turned `out` on, when another one did ([`implied_by`], as a config
/// path).
pub fn implied_by_setting(config: &PipelineConfig, out: Output) -> Option<&'static str> {
    let by = implied_by(config, out)?;
    Some(match by.split_whitespace().next().unwrap_or("") {
        "--do-analysis" => Output::Analysis.setting(),
        "--qsm-reference" => Output::RegionReference.setting(),
        "--do-chisep" => Output::ChiSeparation.setting(),
        "--do-r2primemap" => Output::R2prime.setting(),
        _ => return None,
    })
}

/// How a problem names the settings it involves: as `qsmxt`'s command line spells them (its
/// flags), or by their labels in the settings schema, for a front end that shows settings rather
/// than flags. Every setting a message names is kept, so a problem can list them.
#[derive(Debug, Default)]
pub struct Words {
    labels: bool,
    named: std::cell::RefCell<Vec<&'static str>>,
}

impl Words {
    /// The command line's words.
    pub fn flags() -> Self {
        Words::default()
    }

    /// The settings' labels.
    pub fn labels() -> Self {
        Words { labels: true, named: Default::default() }
    }

    fn note(&self, path: &'static str) {
        let mut named = self.named.borrow_mut();
        if !named.contains(&path) {
            named.push(path);
        }
    }

    /// The setting at `path`: `flag` on the command line, else its label.
    fn setting(&self, path: &'static str, flag: &str) -> String {
        self.note(path);
        if !self.labels {
            return flag.to_string();
        }
        crate::schema::setting(path).map_or_else(|| path.to_string(), |s| s.full_label.to_string())
    }

    /// The setting at `path` set to `value`: `flag` on the command line, else its label and the
    /// option's name ("R2′ strategy “MESE”").
    fn choice(&self, path: &'static str, value: &str, flag: &str) -> String {
        self.note(path);
        if !self.labels {
            return flag.to_string();
        }
        let Some(setting) = crate::schema::setting(path) else { return format!("{path} {value}") };
        let name = setting.value_type.options().into_iter()
            .find(|o| o.value.as_str() == Some(value))
            .map_or_else(|| value.to_string(), |o| o.name.clone());
        format!("{} “{name}”", setting.full_label)
    }

    /// The settings named so far.
    pub fn named(&self) -> Vec<&'static str> {
        self.named.borrow().clone()
    }
}

/// What to do about R2PRIMEnet's field strength before running it.
#[derive(Debug, PartialEq)]
pub enum R2primenetFieldPolicy {
    /// At 3 T: run.
    Run,
    /// Off 3 T but explicitly allowed: run, and warn that the R2' is uncalibrated.
    RunUncalibrated,
    /// Off 3 T under `--r2prime-strategy auto`: skip R2' with a warning rather than fail the run
    /// over a stage the user may not have asked for by name.
    Skip,
    /// Off 3 T and R2PRIMEnet asked for by name: refuse.
    Refuse,
}

pub fn r2primenet_field_policy(
    field_strength: f64, strategy: R2PrimeStrategy, ignore_field_mismatch: bool,
) -> R2primenetFieldPolicy {
    match (r2primenet_field_offset(field_strength).is_some(), ignore_field_mismatch, strategy) {
        (false, _, _) => R2primenetFieldPolicy::Run,
        (true, true, _) => R2primenetFieldPolicy::RunUncalibrated,
        (true, false, R2PrimeStrategy::Auto) => R2primenetFieldPolicy::Skip,
        (true, false, _) => R2primenetFieldPolicy::Refuse,
    }
}

/// How far this acquisition sits outside the band R2PRIMEnet's weights were trained over, in
/// Tesla, or `None` when it is inside it.
///
/// Kept separate from the inference so a non-`dl` build still compiles the check, and so the
/// policy is decided before the weights are fetched rather than after.
fn r2primenet_field_offset(field_strength: f64) -> Option<f64> {
    let off = (field_strength - R2PRIMENET_TRAINED_B0_T).abs();
    (off > R2PRIMENET_B0_TOLERANCE_T).then_some(off)
}

/// qsm-core's `relaxometry::{TRAINED_B0_T, B0_TOLERANCE_T}`, which only exist with its `onnx`
/// feature; a build without it still needs them for the policy. Checked against qsm-core in
/// `qsmxt`'s tests.
pub const R2PRIMENET_TRAINED_B0_T: f64 = 3.0;
pub const R2PRIMENET_B0_TOLERANCE_T: f64 = 0.3;

/// Why R2PRIMEnet off 3 T is a problem, for the warning or the error.
///
/// R2* scales with field strength and the network has no way to be told what it is looking at,
/// so a 7 T R2* produces an R2' map that is the right shape, the right units and the wrong
/// numbers. Nothing downstream can detect that, which is the reason to say it here.
/// What is wrong with running R2PRIMEnet at this field strength under this strategy, when the
/// policy would skip or refuse it; `None` when it may run.
pub fn r2primenet_off_field(
    field_strength: f64, strategy: R2PrimeStrategy, ignore_field_mismatch: bool,
) -> Option<String> {
    r2primenet_off_field_in(field_strength, strategy, ignore_field_mismatch, &Words::flags())
}

fn r2primenet_off_field_in(
    field_strength: f64, strategy: R2PrimeStrategy, ignore_field_mismatch: bool, w: &Words,
) -> Option<String> {
    match r2primenet_field_policy(field_strength, strategy, ignore_field_mismatch) {
        R2primenetFieldPolicy::Skip | R2primenetFieldPolicy::Refuse =>
            Some(r2primenet_field_message_in(field_strength, w)),
        R2primenetFieldPolicy::Run | R2primenetFieldPolicy::RunUncalibrated => None,
    }
}

pub fn r2primenet_field_message(field_strength: f64) -> String {
    r2primenet_field_message_in(field_strength, &Words::flags())
}

fn r2primenet_field_message_in(field_strength: f64, w: &Words) -> String {
    let (measure, supply) = if w.labels {
        (format!("set {} to measure R2' instead", w.choice("separation.r2prime_strategy", "mese", "")),
         format!("supply an R2' map ({})", w.setting("separation.custom_r2prime_tool", "")))
    } else {
        ("use --r2prime-strategy mese to measure R2' instead".to_string(),
         "supply one with --use-custom-r2prime".to_string())
    };
    format!(
        "R2PRIMEnet's weights are trained at {} T and this acquisition is {} T. R2* scales with \
         field strength, so a predicted R2' would be outside what the network has seen and wrong \
         in a way that looks plausible. Acquire a MESE and {measure}, or {supply}",
        R2PRIMENET_TRAINED_B0_T, field_strength,
    )
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

/// Why ARLO cannot fit R2* on this run's GRE echoes, if it cannot: it needs the magnitude and the
/// echo times QSM.rs's ARLO takes ([`qsm_core::r2star::arlo_echo_times_problem`]).
pub fn arlo_problem(f: &ScanFacts) -> Option<String> {
    if !f.has_magnitude {
        return Some("needs magnitude images, and this run has none".into());
    }
    qsm_core::r2star::arlo_echo_times_problem(&f.echo_times).map(|why| {
        let why = why.strip_prefix("ARLO ").unwrap_or(&why);
        format!("{why} (R2* by ARLO)")
    })
}

/// Why the noise map of MEDI, TFI and PDF (MEDI toolbox `Fit_ppm_complex`, from every echo's
/// magnitude) cannot be made for this run, if it cannot: `method` names the method.
fn echo_noise_map_problem(f: &ScanFacts, method: &str) -> Option<String> {
    if !f.has_magnitude {
        return Some(format!(
            "needs the magnitude of every echo ({method} weights the data by a noise map made from \
             them), and this run has no magnitude"));
    }
    let spacing: Vec<f64> = f.echo_times.windows(2).map(|w| w[1] - w[0]).collect();
    let uneven = spacing.iter()
        .any(|d| (d - spacing[0]).abs() > FIT_PPM_COMPLEX_SPACING_TOLERANCE * spacing[0].abs());
    uneven.then(|| format!(
        "needs evenly spaced echoes ({method}'s noise map assumes one echo spacing), and this run's \
         echo times are {}", ms(&f.echo_times)))
}

/// Why the configured dipole inversion cannot run on this run's data, if it cannot: the methods
/// QSM.rs gives a magnitude data weight need magnitude images, MEDI and TFI need every echo's
/// (evenly spaced), and MEDI-mSMV needs R2* for its vessel mask.
pub fn inversion_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    if config.inversion.algorithm == QsmAlgorithm::Qsmart {
        return qsmart_inner_problem(f, config);
    }
    method_problem(f, config, config.inversion.algorithm)
}

/// What dipole inversion `alg` needs of this run's data ([`inversion_problem`]).
fn method_problem(f: &ScanFacts, config: &PipelineConfig, alg: QsmAlgorithm) -> Option<String> {
    let (_, _, core, _) = crate::bridge::to_pipeline_stages(config);
    let weighted = match alg {
        QsmAlgorithm::Ndi | QsmAlgorithm::Nltv | QsmAlgorithm::Fansi | QsmAlgorithm::FansiTgv
        | QsmAlgorithm::L1qsm => true,
        QsmAlgorithm::Whqsm => core.whqsm.weight.needs_magnitude(),
        _ => false,
    };
    if weighted && !f.has_magnitude {
        return Some("needs magnitude images (its data weight), and this run has none".into());
    }
    // The echo time the method converts the field to radians at has to resolve from this scan's
    // echo times (the echo spacing needs them in order). TGV on one echo starts from that echo's
    // wrapped phase, at its own echo time.
    if let Some(te) = crate::stages::inversion::echo_time_choice(config, alg) {
        if !(alg == QsmAlgorithm::Tgv && f.n_echoes == 1) {
            if let Err(e) = qsm_core::EchoTime::from(te).resolve(&f.echo_times) {
                let flag = if alg == QsmAlgorithm::FansiTgv { "fansi".to_string() } else { alg.to_string() };
                return Some(format!("cannot run on this scan with --{flag}-te {te}: {e}"));
            }
        }
    }
    match alg {
        QsmAlgorithm::Medi | QsmAlgorithm::Tfi => {
            let method = if alg == QsmAlgorithm::Medi { "MEDI" } else { "TFI" };
            if let Some(why) = echo_noise_map_problem(f, method) {
                return Some(why);
            }
            if alg == QsmAlgorithm::Medi && config.inversion.medi.msmv && f.n_echoes < 3 {
                return Some(format!(
                    "with mSMV needs at least 3 echoes (its vessel mask uses R2* from ARLO), and this \
                     run has {}", f.n_echoes));
            }
            None
        }
        // The echo time STAR-QSM converts at must resolve from this scan's echo times (the echo
        // spacing needs them in order), and the field strength must be positive.
        QsmAlgorithm::Star => qsm_core::inversion::StarParams {
            echo_times: f.echo_times.clone(), b0: f.field_strength, ..core.star
        }.validate().err().map(|e| format!("cannot run on this scan: {e}")),
        _ => None,
    }
}

/// Why QSMART cannot run its inner inversion on this run's data, if it cannot: what that method
/// needs of the data ([`method_problem`]), and, for MEDI and TFI on several echoes, the magnitude of
/// every echo, which QSM.rs does not give QSMART's inner inversion (it passes one combined
/// magnitude; with a single echo the two are the same).
fn qsmart_inner_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    let inner = config.inversion.qsmart.inversion;
    if matches!(inner, QsmAlgorithm::Medi | QsmAlgorithm::Tfi) && f.n_echoes > 1 {
        return Some(format!(
            "cannot use {inner} as its inner inversion on {} echoes: {inner} needs the magnitude of \
             every echo, and QSM.rs gives QSMART's inner inversion one combined magnitude. Choose \
             another inner inversion (--qsmart-inversion), or use a single echo", f.n_echoes));
    }
    method_problem(f, config, inner).map(|why| format!("with {inner} as its inner inversion: {inner} {why}"))
}

/// Why the configured background removal cannot run on this run's data, if it cannot: PDF needs
/// every echo's magnitude, evenly spaced, for its noise map.
pub fn bg_removal_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    match config.bg_removal.algorithm {
        BfAlgorithm::Pdf => echo_noise_map_problem(f, "PDF"),
        _ => None,
    }
}

/// Why the configured B0 estimation cannot run on this run's data, if it cannot: MEDI's complex
/// fit (QSM.rs `B0EstimationMethod::ComplexFit`) fits every echo's complex signal, so it needs the
/// magnitude of every echo, increasing and evenly spaced echo times (it models the phase per echo
/// spacing), and no bipolar correction (QSM.rs refuses it: MEDI's bipolar fit is not ported).
pub fn field_mapping_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    let fm = &config.field_mapping;
    if fm.b0_estimation != B0Estimation::ComplexFit {
        return None;
    }
    if fm.bipolar_correction {
        return Some("cannot be used with bipolar correction (MEDI's complex fit for bipolar readouts \
                     is not available): turn bipolar correction off, or choose another B0 estimation".into());
    }
    if !f.has_magnitude {
        return Some("needs the magnitude of every echo (it fits the complex echoes), and this run has \
                     no magnitude".into());
    }
    let spacing: Vec<f64> = f.echo_times.windows(2).map(|w| w[1] - w[0]).collect();
    let uneven = spacing.iter()
        .any(|d| *d <= 0.0 || (d - spacing[0]).abs() > FIT_PPM_COMPLEX_SPACING_TOLERANCE * spacing[0].abs());
    uneven.then(|| format!(
        "needs increasing, evenly spaced echoes (it fits the phase per echo spacing), and this run's \
         echo times are {}", ms(&f.echo_times)))
}

/// The note for MEDI+0's CSF term on a run with fewer than 3 echoes (MEDI, or QSMART's inner
/// MEDI): QSM.rs's MEDI finds the CSF from ARLO R2*, which needs 3 echoes, and runs without the
/// term otherwise.
pub fn medi_csf_note(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    let inv = &config.inversion;
    let medi = inv.algorithm == QsmAlgorithm::Medi
        || (inv.algorithm == QsmAlgorithm::Qsmart && inv.qsmart.inversion == QsmAlgorithm::Medi);
    (config.pipeline.do_qsm && medi && inv.medi.csf && f.n_echoes < 3).then(|| format!(
        "MEDI's CSF zero reference (MEDI+0) needs at least 3 echoes (it finds the CSF from R2*), and \
         this run has {}: MEDI runs without it", f.n_echoes))
}

/// Why the EPG fit cannot measure R2 from this run's MESE acquisition, if it cannot.
pub fn mese_problem(f: &ScanFacts) -> Option<String> {
    let Some(tes) = f.mese_echo_times.as_deref() else {
        return Some("needs a multi-echo spin-echo (MESE) acquisition, and this run has none".into());
    };
    if tes.len() < 3 {
        return Some(format!(
            "needs at least 3 spin echoes for the EPG fit, and the MESE has {}", tes.len()));
    }
    let (esp, dev) = spacing_deviation(tes, true);
    if dev > EPG_TOLERANCE_S {
        return Some(format!(
            "the EPG fit needs equally spaced spin echoes, and the MESE's echo times {} are not",
            ms(tes)));
    }
    let first = tes.iter().copied().fold(f64::INFINITY, f64::min);
    if (first - esp).abs() > EPG_TOLERANCE_S {
        return Some(format!(
            "the EPG fit models echoes at TEₙ = n·ESP, and the MESE's first echo ({:.2} ms) is not \
             at its echo spacing ({:.2} ms)", first * 1e3, esp * 1e3));
    }
    None
}

/// Why R2 cannot be had for this run, if it cannot.
pub fn r2_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    r2_problem_in(f, config, &Words::flags())
}

/// A supplied derivative the config names (`--use-custom-* tool`) that was not found: `what` is
/// how the command line names the file, `map` how a label-worded message does.
fn not_supplied(w: &Words, path: &'static str, flag: &str, tool: &str, what: &str, map: &str) -> String {
    if w.labels {
        format!("needs the {map} supplied for {}, and none was found for this run", w.setting(path, ""))
    } else {
        w.note(path);
        format!("{flag} {tool} names no {what} for this run under derivatives/{tool}")
    }
}

fn r2_problem_in(f: &ScanFacts, config: &PipelineConfig, w: &Words) -> Option<String> {
    match config.separation.custom_r2_tool.as_deref() {
        Some(tool) if !f.custom_r2 => Some(not_supplied(
            w, "separation.custom_r2_tool", "--use-custom-r2", tool, "R2 map", "R2 map")),
        Some(_) => None,
        None => mese_problem(f),
    }
}

/// Why R2' cannot be had for this run, if it cannot.
pub fn r2prime_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    r2prime_problem_in(f, config, &Words::flags())
}

fn r2prime_problem_in(f: &ScanFacts, config: &PipelineConfig, w: &Words) -> Option<String> {
    let s = &config.separation;
    if let Some(tool) = s.custom_r2prime_tool.as_deref() {
        return (!f.custom_r2prime).then(|| not_supplied(
            w, "separation.custom_r2prime_tool", "--use-custom-r2prime", tool, "R2' map", "R2' map"));
    }
    if let Some(why) = arlo_problem(f) {
        return Some(format!("needs R2*, which {why}"));
    }
    let r2 = r2_problem_in(f, config, w);
    match s.r2prime_strategy {
        R2PrimeStrategy::Mese => r2.map(|why| if w.labels {
            format!("measures R2' as R2* − R2 (with {}), and R2 {why}",
                w.choice("separation.r2prime_strategy", "mese", ""))
        } else {
            w.note("separation.r2prime_strategy");
            format!("--r2prime-strategy mese measures R2' as R2* − R2, and R2 {why}")
        }),
        R2PrimeStrategy::Auto if r2.is_none() => None,
        strategy => {
            let field = r2primenet_off_field_in(
                f.field_strength, strategy, s.r2primenet.ignore_field_mismatch, w)?;
            let anyway = if w.labels {
                format!("turn on {} to run it anyway (uncalibrated)",
                    w.setting("separation.r2primenet.ignore_field_mismatch", ""))
            } else {
                w.note("separation.r2primenet.ignore_field_mismatch");
                "pass --r2primenet-ignore-field-mismatch to run it anyway (uncalibrated)".to_string()
            };
            Some(match (strategy, r2) {
                (R2PrimeStrategy::Auto, Some(r2)) => format!(
                    "has no R2 to subtract (R2 {r2}), so R2PRIMEnet would estimate it, but {field}; \
                     {anyway}"),
                _ => format!("{}: {field}; {anyway}",
                    w.choice("separation.r2prime_strategy", "r2primenet", "--r2prime-strategy r2primenet")),
            })
        }
    }
}

/// Why a segmentation cannot be had for this run, if it cannot.
pub fn segmentation_problem(f: &ScanFacts) -> Option<String> {
    segmentation_problem_in(f, &Words::flags())
}

fn segmentation_problem_in(f: &ScanFacts, w: &Words) -> Option<String> {
    if f.custom_dseg || f.has_magnitude {
        return None;
    }
    let supplied = if w.labels {
        format!("a supplied segmentation, {}", w.setting("segmentation.custom_dseg_tool", ""))
    } else {
        w.note("segmentation.custom_dseg_tool");
        "a dseg via --use-custom-dseg".to_string()
    };
    Some(format!("needs magnitude images to segment (or {supplied}), and this run has no magnitude"))
}

/// Why chi-separation cannot run on this run, if it cannot.
pub fn chisep_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    chisep_problem_in(f, config, &Words::flags())
}

fn chisep_problem_in(f: &ScanFacts, config: &PipelineConfig, w: &Words) -> Option<String> {
    let s = &config.separation;
    if let Some(tool) = s.custom_qsm_tool.as_deref() {
        if !f.custom_qsm {
            return Some(not_supplied(w, "separation.custom_qsm_tool", "--use-custom-qsm", tool, "Chimap", "susceptibility map"));
        }
    } else if !config.pipeline.do_qsm {
        return Some(if w.labels {
            format!("needs a susceptibility map: turn on {} or supply one ({})",
                w.setting("pipeline.do_qsm", ""), w.setting("separation.custom_qsm_tool", ""))
        } else {
            w.note("pipeline.do_qsm");
            w.note("separation.custom_qsm_tool");
            "needs a susceptibility map: drop --no-qsm or pass --use-custom-qsm".into()
        });
    }
    let magnitude = || (!f.has_magnitude)
        .then(|| "needs magnitude images, and this run has none".to_string());
    let r2prime = || r2prime_problem_in(f, config, w).map(|why| format!("needs R2', which {why}"));
    // The local field is written by background-field removal, which only the field-based
    // inversions run, and only when the pipeline reconstructs χ itself.
    let local_field = || {
        if s.custom_qsm_tool.is_some() && !config.pipeline.do_qsm {
            Some("needs the local field, which a bring-your-own QSM does not come with".to_string())
        } else if crate::bridge::skips_bgremove(config) {
            let algorithm = config.inversion.algorithm.to_string();
            Some(format!("needs the local field, and {} has no background-field removal step to \
                produce one", w.choice("inversion.algorithm", &algorithm,
                    &format!("--qsm-algorithm {algorithm}"))))
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
pub fn problem(f: &ScanFacts, config: &PipelineConfig, out: Output) -> Option<String> {
    problem_in(f, config, out, &Words::flags())
}

/// [`problem`], naming settings in `w`'s words.
pub fn problem_in(f: &ScanFacts, config: &PipelineConfig, out: Output, w: &Words) -> Option<String> {
    match out {
        Output::Swi => (!f.has_magnitude)
            .then(|| "needs magnitude images, and this run has none".into()),
        Output::Smwi => match config.separation.custom_qsm_tool.as_deref() {
            _ if !f.has_magnitude => Some("needs magnitude images, and this run has none".into()),
            Some(tool) if !config.pipeline.do_qsm && !f.custom_qsm => Some(if w.labels {
                not_supplied(w, "separation.custom_qsm_tool", "", tool, "", "susceptibility map")
            } else {
                format!("needs a susceptibility map, and {}",
                    not_supplied(w, "separation.custom_qsm_tool", "--use-custom-qsm", tool, "Chimap", ""))
            }),
            _ => None,
        },
        Output::Segmentation => segmentation_problem_in(f, w),
        Output::T2star | Output::R2star => arlo_problem(f),
        Output::R2 => r2_problem_in(f, config, w),
        Output::R2prime => r2prime_problem_in(f, config, w),
        Output::ChiSeparation => chisep_problem_in(f, config, w),
        Output::Analysis | Output::RegionReference => segmentation_problem_in(f, w)
            .map(|why| format!("needs a segmentation, which {why}")),
    }
}

/// Whether the runner should run the stage for `out` on this run: enabled, and possible.
pub fn wanted(f: &ScanFacts, config: &PipelineConfig, out: Output) -> bool {
    out.enabled(config) && problem(f, config, out).is_none()
}

/// Problems that stop a run whatever was asked for.
fn input_problems(f: &ScanFacts) -> Option<String> {
    coil_combination_problem(f.n_echoes, f.n_coils)
}

/// Why the magnitude cannot be corrected for inhomogeneity as configured, if it cannot: N4 needs
/// a grid it can shrink (on the acquired grid; the run checks its working grid again).
pub fn inhomogeneity_problem(f: &ScanFacts, config: &PipelineConfig) -> Option<String> {
    let on = f.has_magnitude && config.masking.inhomogeneity_correction
        && config.masking.inhomogeneity_method == InhomogeneityMethod::N4;
    if !on {
        return None;
    }
    config.n4.grid_problem(f.dims)
}

/// Why `n_coils` uncombined coils of `n_echoes` echoes cannot be combined, if they cannot.
pub fn coil_combination_problem(n_echoes: usize, n_coils: usize) -> Option<String> {
    (n_coils > 1 && n_echoes < 2).then(|| format!(
        "MCPC-3D-S coil combination needs at least 2 echoes, and this run has {n_echoes} echo with \
         {n_coils} uncombined coils. Combine the coils first (on the scanner, or with another tool)."))
}

/// The result of checking every run: one error line per requested output a run cannot produce,
/// one warning per implied output it cannot.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Check {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// The same problems, each as data: for a front end that words them itself (qsmbly names
    /// settings by their labels, not by their flags).
    pub problems: Vec<Problem>,
}

/// One problem a [`Check`] found, as data, in words a front end that shows settings rather than
/// flags can use: settings are named by their schema labels and listed by their config paths.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Problem {
    /// An error (an output asked for that cannot be made: nothing runs) or a warning (an output
    /// another one implied: the run goes ahead without it).
    pub severity: Severity,
    /// The setting asking for the output ([`Output::setting`]); `None` for a problem with the
    /// input data itself.
    pub setting: Option<&'static str>,
    /// For a warning, the setting whose option implied the output ([`implied_by_setting`]).
    pub implied_by: Option<&'static str>,
    /// Every setting the problem involves, by config path: the output's, the one that implied
    /// it, and each the message names (what would resolve it among them).
    pub settings: Vec<&'static str>,
    /// What is missing, after the output's name ("needs magnitude images, and this run has
    /// none"); for an input problem, the whole message. Settings are named by their labels.
    pub reason: String,
    /// The whole problem in a sentence, naming the output and any implying setting by label:
    /// "Compute SWI needs magnitude images, and this run has none".
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// Check one scan against the config. `key` names it in the messages (`qsmxt` uses the run's
/// BIDS key, e.g. `sub-01_ses-1_MEGRE`).
pub fn check_scan(key: &str, f: &ScanFacts, config: &PipelineConfig, out: &mut Check) {
    let Check { errors, warnings, problems } = scan_check(f, config);
    out.problems.extend(problems);
    out.errors.extend(errors.into_iter().map(|e| format!("{key}: {e}")));
    out.warnings.extend(warnings.into_iter().map(|w| format!("{key}: {w}")));
}

/// [`check_scan`] for a scan on its own: the same messages, without a key in front of them.
pub fn scan_check(f: &ScanFacts, config: &PipelineConfig) -> Check {
    let mut out = Check::default();
    if let Some(why) = input_problems(f) {
        out.errors.push(why.clone());
        out.problems.push(Problem {
            severity: Severity::Error, setting: None, implied_by: None, settings: vec![],
            reason: why.clone(), message: why,
        });
    }
    if let Some(why) = inhomogeneity_problem(f, config) {
        let path = "masking.inhomogeneity_method";
        out.errors.push(why.clone());
        out.problems.push(Problem {
            severity: Severity::Error, setting: Some(path), implied_by: None,
            settings: vec![path, "n4.shrink_factor"], reason: why.clone(), message: why,
        });
    }
    let label = |path: &str| crate::schema::setting(path).map_or_else(|| path.to_string(), |s| s.full_label.to_string());
    // The reconstruction's own methods: what they need of the data.
    if config.pipeline.do_qsm {
        let bg = config.bg_removal.algorithm;
        let methods = [
            ("field_mapping.b0_estimation", "--b0-estimation", config.field_mapping.b0_estimation.to_string(),
             if crate::bridge::field_mapping_runs(config, f.n_echoes) { field_mapping_problem(f, config) } else { None }),
            ("inversion.algorithm", "--qsm-algorithm", config.inversion.algorithm.to_string(),
             inversion_problem(f, config)),
            ("bg_removal.algorithm", "--bf-algorithm", bg.to_string(),
             if crate::bridge::skips_bgremove(config) { None } else { bg_removal_problem(f, config) }),
        ];
        for (path, flag, value, why) in methods {
            let Some(why) = why else { continue };
            out.errors.push(format!("{flag} {value} {why}"));
            let words = Words::labels();
            let message = format!("{} {why}", words.choice(path, &value, ""));
            out.problems.push(Problem {
                severity: Severity::Error, setting: Some(path), implied_by: None,
                settings: vec![path], reason: why, message,
            });
        }
    }
    if let Some(note) = medi_csf_note(f, config) {
        out.warnings.push(note.clone());
        out.problems.push(Problem {
            severity: Severity::Warning, setting: Some("inversion.medi.csf"), implied_by: None,
            settings: vec!["inversion.medi.csf"], reason: note.clone(), message: note,
        });
    }
    for o in Output::ALL {
        if !o.enabled(config) {
            continue;
        }
        let Some(why) = problem(f, config, o) else { continue };
        let by = implied_by(config, o);
        match &by {
            Some(by) => out.warnings.push(format!("{} (turned on by {by}) is skipped: it {why}", o.flag())),
            None => out.errors.push(format!("{} {why}", o.flag())),
        }
        let severity = if by.is_some() { Severity::Warning } else { Severity::Error };
        let words = Words::labels();
        let reason = problem_in(f, config, o, &words).unwrap_or_else(|| why.clone());
        let implied = implied_by_setting(config, o);
        let output = label(o.setting());
        let message = match implied {
            Some(by) => format!("{output} (turned on by {}) is skipped: it {reason}", label(by)),
            None => format!("{output} {reason}"),
        };
        let mut settings = vec![o.setting()];
        settings.extend(implied);
        for s in words.named() {
            if !settings.contains(&s) {
                settings.push(s);
            }
        }
        out.problems.push(Problem { severity, setting: Some(o.setting()), implied_by: implied, settings, reason, message });
    }
    out
}

impl Check {
    /// The outcome of checking every scan before any is processed. Warnings are logged; errors,
    /// all of them at once, fail the command.
    pub fn finish(self) -> Result<(), EngineError> {
        for w in &self.warnings {
            log::warn!("{w}");
        }
        if self.errors.is_empty() {
            return Ok(());
        }
        Err(EngineError::Config(format!(
            "{} requested output(s) cannot be produced from the data, so nothing was processed:\n  {}\n\
             Drop the option, or narrow the runs with --include/--exclude.",
            self.errors.len(), self.errors.join("\n  "))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(echo_times: &[f64], has_magnitude: bool) -> ScanFacts {
        ScanFacts {
            dims: (4, 4, 4),
            n_echoes: echo_times.len(),
            echo_times: echo_times.to_vec(),
            has_magnitude,
            n_coils: 1,
            field_strength: 3.0,
            b0_dir: None,
            mese_echo_times: None,
            real_imaginary: false,
            custom_mask: false, custom_dseg: false, custom_qsm: false, custom_r2: false,
            custom_r2prime: false,
        }
    }

    fn with_mese(mut r: ScanFacts, tes: &[f64]) -> ScanFacts {
        r.mese_echo_times = Some(tes.to_vec());
        r
    }

    const EVEN: [f64; 4] = [0.005, 0.010, 0.015, 0.020];

    #[test]
    fn n4_refuses_a_grid_it_cannot_shrink() {
        let mut c = PipelineConfig::default();
        c.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        let thin = ScanFacts { dims: (64, 64, 6), ..run(&[0.004, 0.008], true) };
        let out = scan_check(&thin, &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("64x64x6") && out.errors[0].contains("--n4-shrink-factor"), "{out:?}");
        assert_eq!(out.problems[0].setting, Some("masking.inhomogeneity_method"));
        c.n4.shrink_factor = 3;
        assert!(scan_check(&thin, &c).errors.is_empty());
        c.n4.shrink_factor = 4;
        c.masking.inhomogeneity_method = InhomogeneityMethod::Makehomogeneous;
        assert!(scan_check(&thin, &c).errors.is_empty(), "makehomogeneous has no such limit");
        c.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        c.masking.inhomogeneity_correction = false;
        assert!(scan_check(&thin, &c).errors.is_empty(), "no correction, no limit");
        assert!(scan_check(&ScanFacts { has_magnitude: false, ..thin }, &PipelineConfig {
            masking: MaskingConfig { inhomogeneity_method: InhomogeneityMethod::N4, ..Default::default() },
            ..Default::default() }).errors.is_empty(), "no magnitude, nothing to correct");
    }

    fn check(r: &ScanFacts, c: &PipelineConfig) -> Check {
        let mut out = Check::default();
        check_scan("sub-01_MEGRE", r, c, &mut out);
        out
    }

    /// The final config, as `apply_run_overrides` leaves it.
    fn enforced(mut c: PipelineConfig) -> PipelineConfig {
        crate::config::enforce_analysis_dependencies(&mut c);
        crate::config::enforce_smwi_dependencies(&mut c);
        crate::config::enforce_separation_dependencies(&mut c);
        crate::config::enforce_reference_dependencies(&mut c);
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

    /// R2* takes what QSM.rs's ARLO takes: magnitude, and three or more increasing echo times,
    /// evenly spaced or not.
    #[test]
    fn r2star_needs_three_increasing_echoes_with_magnitude() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2starmap = true;
        let two = check(&run(&[0.005, 0.010], true), &c);
        assert!(two.errors.len() == 1 && two.errors[0].contains("at least 3 echoes"), "{two:?}");
        let unordered = check(&run(&[0.005, 0.015, 0.010], true), &c);
        assert!(unordered.errors.len() == 1 && unordered.errors[0].contains("strictly increasing"), "{unordered:?}");
        let phase_only = check(&run(&EVEN, false), &c);
        assert!(phase_only.errors.len() == 1 && phase_only.errors[0].contains("magnitude"), "{phase_only:?}");
        for ok in [&EVEN[..], &[0.005, 0.010, 0.015, 0.030]] {
            let ok = check(&run(ok, true), &c);
            assert!(ok.errors.is_empty() && ok.warnings.is_empty(), "{ok:?}");
        }
    }

    #[test]
    fn t2star_is_refused_on_the_same_terms() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_t2starmap = true;
        let out = check(&run(&[0.005, 0.010], true), &c);
        assert!(out.errors.iter().any(|e| e.contains("--do-t2starmap")), "{out:?}");
    }

    /// R2*-based chi-separation turns R2* on: R2* is then a warning, and chi-separation, which
    /// was asked for, is the error.
    #[test]
    fn r2star_qsm_chisep_on_two_echoes_is_refused_and_its_r2star_warned() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::R2starQsm;
        let out = check(&run(&[0.005, 0.010], true), &enforced(c));
        assert!(out.warnings.iter().any(|w| w.contains("--do-r2starmap")
            && w.contains("turned on by --do-chisep")), "{out:?}");
        assert!(out.errors.len() == 1 && out.errors[0].contains("--do-chisep")
            && out.errors[0].contains("at least 3 echoes"), "{out:?}");
    }

    /// What the QSM methods need of the data: magnitude for the magnitude-weighted ones, every
    /// echo's (evenly spaced) for MEDI, TFI and PDF, three echoes for MEDI-mSMV. Each is an
    /// error naming the method, before any processing; the defaults need nothing more.
    #[test]
    fn qsm_methods_say_what_they_need_of_the_data() {
        let with = |f: fn(&mut PipelineConfig)| { let mut c = PipelineConfig::default(); f(&mut c); c };
        assert!(check(&run(&EVEN, false), &PipelineConfig::default()).errors.is_empty());
        for alg in [QsmAlgorithm::Ndi, QsmAlgorithm::Nltv, QsmAlgorithm::Fansi, QsmAlgorithm::FansiTgv,
                    QsmAlgorithm::L1qsm, QsmAlgorithm::Medi, QsmAlgorithm::Tfi] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let out = check(&run(&EVEN, false), &c);
            assert!(out.errors.len() == 1 && out.errors[0].contains(&format!("--qsm-algorithm {alg}"))
                && out.errors[0].contains("magnitude"), "{alg}: {out:?}");
            assert_eq!(out.problems[0].setting, Some("inversion.algorithm"));
            assert!(check(&run(&EVEN, true), &c).errors.is_empty(), "{alg}");
        }
        // WH-QSM's default weight is flat: no magnitude needed.
        assert!(check(&run(&EVEN, false), &with(|c| c.inversion.algorithm = QsmAlgorithm::Whqsm)).errors.is_empty());
        // The noise map of MEDI, TFI and PDF needs evenly spaced echoes.
        let uneven = run(&[0.005, 0.010, 0.020], true);
        for c in [with(|c| c.inversion.algorithm = QsmAlgorithm::Medi), with(|c| c.inversion.algorithm = QsmAlgorithm::Tfi),
                  with(|c| c.bg_removal.algorithm = BfAlgorithm::Pdf)] {
            let out = check(&uneven, &c);
            assert!(out.errors.len() == 1 && out.errors[0].contains("evenly spaced"), "{out:?}");
        }
        let pdf = check(&run(&EVEN, false), &with(|c| c.bg_removal.algorithm = BfAlgorithm::Pdf));
        assert!(pdf.errors.len() == 1 && pdf.errors[0].contains("--bf-algorithm pdf"), "{pdf:?}");
        assert_eq!(pdf.problems[0].setting, Some("bg_removal.algorithm"));
        // PDF is not asked of a run without background removal.
        let mut tgv = with(|c| c.bg_removal.algorithm = BfAlgorithm::Pdf);
        tgv.inversion.algorithm = QsmAlgorithm::Tgv;
        assert!(check(&run(&EVEN, false), &tgv).errors.is_empty());
        // MEDI-mSMV needs three echoes for its vessel mask.
        let mut msmv = with(|c| c.inversion.algorithm = QsmAlgorithm::Medi);
        msmv.inversion.medi.msmv = true;
        let out = check(&run(&[0.005, 0.010], true), &msmv);
        assert!(out.errors.len() == 1 && out.errors[0].contains("mSMV") && out.errors[0].contains("at least 3 echoes"), "{out:?}");
        assert!(check(&run(&EVEN, true), &msmv).errors.is_empty());
        // Nothing is asked of a run that does not reconstruct.
        let mut off = with(|c| c.inversion.algorithm = QsmAlgorithm::Medi);
        off.pipeline.do_qsm = false;
        assert!(check(&run(&EVEN, false), &off).errors.is_empty());
    }

    /// STAR-QSM's echo time has to resolve from the scan's echo times.
    #[test]
    fn star_checks_its_echo_time_against_the_scan() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Star;
        assert!(check(&run(&EVEN, false), &c).errors.is_empty(), "STAR-QSM takes no magnitude");
        c.inversion.star.te = crate::EchoTimeChoice::Spacing;
        let out = check(&run(&[0.02, 0.01], false), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--qsm-algorithm star")
            && out.errors[0].contains("echo spacing"), "{out:?}");
        assert_eq!(out.problems[0].setting, Some("inversion.algorithm"));
    }

    /// A method's echo time has to resolve from the scan's echo times; TGV on one echo starts from
    /// that echo's wrapped phase, so its choice is not checked there.
    #[test]
    fn the_echo_time_is_checked_against_the_scan() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(check(&run(&EVEN, true), &c).errors.is_empty());
        // The echo spacing of echoes out of order is not a time.
        let out = check(&run(&[0.02, 0.01], true), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--medi-te spacing")
            && out.errors[0].contains("echo spacing"), "{out:?}");
        // One echo's spacing is its echo time (MEDI toolbox).
        assert!(check(&run(&[0.02], true), &c).errors.is_empty());
        c.inversion.algorithm = QsmAlgorithm::FansiTgv;
        c.inversion.fansi.te = crate::EchoTimeChoice::Seconds(0.0);
        let out = check(&run(&EVEN, true), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--fansi-te 0"), "{out:?}");
        c.inversion.algorithm = QsmAlgorithm::Tgv;
        c.inversion.tgv.te = crate::EchoTimeChoice::Spacing;
        assert!(check(&run(&[0.02, 0.01], false), &c).errors.len() == 1);
        assert!(check(&run(&[0.02], false), &c).errors.is_empty(), "one echo is at its own echo time");
    }

    /// WH-QSM needs magnitude images only for a magnitude data weight.
    #[test]
    fn whqsm_needs_magnitude_only_for_a_magnitude_weight() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        assert!(check(&run(&EVEN, false), &c).errors.is_empty());
        c.inversion.whqsm.weight = crate::WhqsmWeight::MaxNormalised;
        let out = check(&run(&EVEN, false), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("needs magnitude"), "{out:?}");
    }

    /// QSMART's inner inversion gets the same checks as the inversion; inner MEDI or TFI is
    /// refused only on several echoes, where QSM.rs cannot give it every echo's magnitude.
    #[test]
    fn qsmart_checks_its_inner_inversion() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        for inner in [QsmAlgorithm::Medi, QsmAlgorithm::Tfi] {
            c.inversion.qsmart.inversion = inner;
            let out = check(&run(&EVEN, true), &c);
            assert!(out.errors.len() == 1 && out.errors[0].contains("--qsm-algorithm qsmart")
                && out.errors[0].contains("QSM.rs gives QSMART's inner inversion one combined magnitude"), "{out:?}");
            assert!(check(&run(&[0.02], true), &c).errors.is_empty(), "{inner} on one echo works");
        }
        c.inversion.qsmart.inversion = QsmAlgorithm::Nltv;
        let out = check(&run(&EVEN, false), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("nltv") && out.errors[0].contains("magnitude"), "{out:?}");
        c.inversion.qsmart.inversion = QsmAlgorithm::Ilsqr;
        assert!(check(&run(&EVEN, false), &c).errors.is_empty());
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
        seven.field_strength = 7.0;
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
        r.n_coils = 2;
        let out = check(&r, &PipelineConfig::default());
        assert!(out.errors.len() == 1 && out.errors[0].contains("MCPC-3D-S")
            && out.errors[0].contains("2 uncombined coils"), "{out:?}");
    }

    #[test]
    fn finish_reports_every_problem_at_once() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_swi = true;
        c.pipeline.do_r2starmap = true;
        let err = check(&run(&[0.005, 0.010], false), &c).finish().unwrap_err().to_string();
        assert!(err.contains("--do-swi") && err.contains("--do-r2starmap"), "{err}");
        assert!(check(&run(&EVEN, true), &c).finish().is_ok());
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

    /// Every problem the checks can produce, as data, names settings by label and path, never
    /// by flag; the command line's messages are unchanged.
    #[test]
    fn problems_as_data_name_no_flags() {
        let (mut configs, mut distinct) = (Vec::new(), Vec::new());
        for algorithm in [SeparationAlgorithm::R2starQsm, SeparationAlgorithm::ChiSepIlsqr,
                          SeparationAlgorithm::HcChisep, SeparationAlgorithm::SusepNet] {
            for strategy in [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese, R2PrimeStrategy::R2primenet] {
                for (custom, no_qsm, inversion) in [(false, false, QsmAlgorithm::Ilsqr), (true, false, QsmAlgorithm::Ilsqr),
                                                    (false, true, QsmAlgorithm::Ilsqr), (false, false, QsmAlgorithm::Iqsm)] {
                    let mut c = PipelineConfig::default();
                    c.pipeline.do_swi = true;
                    c.pipeline.do_smwi = true;
                    c.pipeline.do_analysis = true;
                    c.pipeline.do_r2starmap = true;
                    c.pipeline.do_r2map = true;
                    c.pipeline.do_r2primemap = true;
                    c.pipeline.do_chi_separation = true;
                    c.pipeline.do_qsm = !no_qsm;
                    c.inversion.algorithm = inversion;
                    c.separation.algorithm = algorithm;
                    c.separation.r2prime_strategy = strategy;
                    if custom {
                        c.separation.custom_qsm_tool = Some("mine".into());
                        c.separation.custom_r2_tool = Some("mine".into());
                        c.separation.custom_r2prime_tool = Some("mine".into());
                    }
                    // Some combinations come out the same once enforced: each is checked once.
                    let c = enforced(c);
                    let json = serde_json::to_value(&c).unwrap();
                    if !distinct.contains(&json) {
                        distinct.push(json);
                        configs.push(c);
                    }
                }
            }
        }
        let mut seven = run(&[0.005, 0.010, 0.030], false);
        seven.field_strength = 7.0;
        let mut coils = run(&[0.005], true);
        coils.n_coils = 4;
        let scans = [run(&EVEN, true), run(&EVEN, false), seven, coils];
        let settings: Vec<&str> = crate::schema::schema().iter().map(|s| s.path).collect();
        let mut seen = 0;
        for c in &configs {
            for f in &scans {
                let out = check(f, c);
                assert_eq!(out.problems.len(), out.errors.len() + out.warnings.len(), "{out:?}");
                for p in &out.problems {
                    seen += 1;
                    for text in [&p.reason, &p.message] {
                        assert!(!text.contains("--"), "a flag in {text:?}");
                        assert!(!text.contains("derivatives/"), "a path in {text:?}");
                    }
                    if let Some(s) = p.setting {
                        assert_eq!(p.settings[0], s);
                    }
                    for s in &p.settings {
                        assert!(settings.contains(s), "{s} is not a setting");
                    }
                    if let Some(by) = p.implied_by {
                        assert!(p.settings.contains(&by));
                    }
                }
            }
        }
        assert!(seen > 100, "{seen}");
    }

    /// The words a problem names its settings in, and the settings it lists.
    #[test]
    fn problems_name_settings_by_label() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        let out = check(&run(&EVEN, true), &enforced(c));
        let p = out.problems.iter().find(|p| p.setting == Some("pipeline.do_r2primemap")).unwrap();
        assert!(p.reason.starts_with("measures R2' as R2* − R2 (with ") && p.reason.contains("“MESE”)"), "{}", p.reason);
        assert!(p.settings.contains(&"separation.r2prime_strategy"), "{:?}", p.settings);
        assert!(p.message.starts_with(crate::schema::setting("pipeline.do_r2primemap").unwrap().full_label), "{}", p.message);
        // The command line's message is the one it always was.
        assert!(out.errors.iter().any(|e| e.contains("--r2prime-strategy mese measures R2'")), "{out:?}");

        // An implied output names the setting that implied it.
        let mut c = PipelineConfig::default();
        c.pipeline.do_analysis = true;
        let out = check(&run(&EVEN, false), &enforced(c));
        let w = out.problems.iter().find(|p| p.severity == Severity::Warning).unwrap();
        assert_eq!((w.setting, w.implied_by), (Some("pipeline.do_segmentation"), Some("pipeline.do_analysis")));
        assert!(w.message.contains("turned on by") && w.settings.contains(&"segmentation.custom_dseg_tool"), "{w:?}");

        // The local field an end-to-end inversion does not produce names the inversion's method.
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        let out = check(&with_mese(run(&EVEN, true), &[0.01, 0.02, 0.03]), &enforced(c));
        let p = &out.problems[0];
        assert!(p.reason.contains("“iQSM” has no background-field removal step"), "{}", p.reason);
        assert!(p.settings.contains(&"inversion.algorithm"), "{:?}", p.settings);
    }

    /// Off 3 T: `auto` skips, an explicit `r2primenet` refuses, and the override runs either.
    #[test]
    fn r2primenet_field_policy_by_strategy() {
        use super::{r2primenet_field_policy as policy, R2primenetFieldPolicy as P};
        use crate::R2PrimeStrategy as S;
        assert_eq!(policy(3.0, S::Auto, false), P::Run);
        assert_eq!(policy(3.0, S::R2primenet, false), P::Run);
        assert_eq!(policy(7.0, S::Auto, false), P::Skip);
        assert_eq!(policy(7.0, S::R2primenet, false), P::Refuse);
        assert_eq!(policy(7.0, S::Auto, true), P::RunUncalibrated);
        assert_eq!(policy(1.5, S::R2primenet, true), P::RunUncalibrated);
    }

    /// R2PRIMEnet's 3 T band: everything sold as "3 T" is inside it, 1.5 T and 7 T are not.
    ///
    /// The warning this drives is the only thing standing between a 7 T user and an R2' map that
    /// is the right shape, the right units and the wrong numbers.
    #[test]
    fn r2primenet_field_band_covers_3t_and_excludes_7t() {
        for inside in [3.0, 2.89, 2.8, 3.2] {
            assert!(r2primenet_field_offset(inside).is_none(),
                    "{inside} T is a 3 T acquisition");
        }
        for outside in [1.5, 7.0, 9.4, 0.55] {
            assert!(r2primenet_field_offset(outside).is_some(),
                    "{outside} T is not what R2PRIMEnet was trained on");
        }
    }

    /// MEDI's complex fit needs every echo's magnitude, evenly spaced echoes and no bipolar
    /// correction; a single echo is its phase over its echo time, as in QSM.rs.
    #[test]
    fn complex_fit_says_what_it_needs() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::ComplexFit;
        assert!(check(&run(&EVEN, true), &c).errors.is_empty());
        assert!(check(&run(&[0.02], true), &c).errors.is_empty());
        let out = check(&run(&EVEN, false), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("--b0-estimation complex-fit needs the magnitude"), "{out:?}");
        let out = check(&run(&[0.005, 0.010, 0.020], true), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("evenly spaced"), "{out:?}");
        assert!(out.problems[0].settings == ["field_mapping.b0_estimation"], "{out:?}");
        assert!(check(&run(&[0.010, 0.005], true), &c).errors.len() == 1, "decreasing echo times");
        c.field_mapping.bipolar_correction = true;
        let out = check(&run(&EVEN, true), &c);
        assert!(out.errors.len() == 1 && out.errors[0].contains("bipolar correction"), "{out:?}");
        // Without the complex fit none of this applies.
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(check(&run(&[0.005, 0.010, 0.020], false), &c).errors.is_empty());
    }

    /// MEDI+0's CSF term on fewer than 3 echoes is a warning that MEDI runs without it, not an
    /// error and not silence.
    #[test]
    fn medi_csf_on_fewer_than_three_echoes_is_noted() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        let out = check(&run(&[0.005, 0.010], true), &c);
        assert!(out.errors.is_empty() && out.warnings.len() == 1 && out.warnings[0].contains("MEDI runs without it"), "{out:?}");
        assert_eq!(out.problems[0].setting, Some("inversion.medi.csf"));
        assert!(check(&run(&EVEN, true), &c).warnings.is_empty());
        c.inversion.medi.csf = false;
        assert!(check(&run(&[0.005, 0.010], true), &c).warnings.is_empty());
    }
}
