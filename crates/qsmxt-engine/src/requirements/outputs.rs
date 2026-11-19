//! What each output needs of a run's data: R2 from a MESE, R2', a segmentation, χ-separation.

use super::*;

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

/// Whether this scan gives χ-sepnet's toolbox route an R2′ measured as R2* − R2, which it takes in
/// place of its own estimate: there is an R2 to subtract and R2* to subtract it from.
pub fn measures_r2prime_for_chisepnet(f: &ScanFacts, config: &PipelineConfig) -> bool {
    crate::config::chisepnet_route_can_estimate_r2prime(config)
        && config.separation.r2prime_strategy != R2PrimeStrategy::R2primenet
        && r2_problem(f, config).is_none()
        && arlo_problem(f).is_none()
}

/// χ-separation by the toolbox's iterative methods (χ-sep-iLSQR, χ-sep-MEDI), which take a
/// measured R2′.
fn c_is_chisep_iterative(config: &PipelineConfig) -> bool {
    use crate::enums::SeparationAlgorithm as S;
    config.pipeline.do_chi_separation && matches!(config.separation.algorithm, S::ChiSepIlsqr | S::ChiSepMedi)
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
        R2PrimeStrategy::Mese => r2.map(|why| {
            // The χ-separation toolbox's pipelines measure R2′ only; prediction is QSMxT's offer.
            let toolbox = c_is_chisep_iterative(config);
            if w.labels {
                let predict = if toolbox {
                    format!("; {} predicts it with R2PRIMEnet instead (QSMxT's recommendation; the \
                             χ-separation toolbox takes a measured R2')",
                        w.choice("separation.r2prime_strategy", "auto", ""))
                } else { String::new() };
                format!("measures R2' as R2* − R2 (with {}), and R2 {why}{predict}",
                    w.choice("separation.r2prime_strategy", "mese", ""))
            } else {
                w.note("separation.r2prime_strategy");
                let predict = if toolbox {
                    "; --r2prime-strategy auto predicts it with R2PRIMEnet instead (QSMxT's \
                     recommendation; the χ-separation toolbox takes a measured R2'), or supply one \
                     with --use-custom-r2prime"
                } else { "" };
                format!("--r2prime-strategy mese measures R2' as R2* − R2, and R2 {why}{predict}")
            }
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
        // chi_sep_iLSQR takes the magnitude but never uses it.
        SeparationAlgorithm::ChiSepIlsqr => local_field().or_else(r2prime),
        // Its noise map is made from every echo's magnitude, and its CSF mask from R2* (ARLO or
        // the toolbox's NNLS fit, separation.chi_sep_medi.csf_r2star).
        SeparationAlgorithm::ChiSepMedi => local_field().or_else(r2prime).or_else(magnitude)
            .or_else(|| echo_noise_map_problem(f, "χ-separation (MEDI)"))
            .or_else(|| match s.chi_sep_medi.csf_r2star {
                crate::ChiSepCsfR2star::Arlo => arlo_problem(f),
                m @ crate::ChiSepCsfR2star::Nnls => qsm_core::separation::ChiSepCsfR2star::from(m)
                    .echo_times_problem(&f.echo_times).map(|why| format!("{why} (R2* by NNLS)")),
            }.map(|why| format!("needs a CSF mask from R2*, which {why}"))),
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
