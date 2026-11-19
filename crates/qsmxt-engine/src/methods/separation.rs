//! Methods prose for susceptibility-source separation.

use super::*;

/// The non-default settings of the selected susceptibility-source separation.
pub(super) fn separation_changes(config: &PipelineConfig) -> Changed {
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

/// DECOMPOSE's χ bound from the echo spacing: in ppm where every run that recorded it
/// ([`Topic::ChiBound`]) had the same, and otherwise as where it came from.
pub(super) fn chi_bound_sentence(ran: Ran) -> String {
    let reasons: std::collections::BTreeSet<&str> = ran.0.and_then(|r| r.step("chi_separation"))
        .map(|s| s.decisions.iter().filter(|d| d.topic == Topic::ChiBound).map(|d| d.reason.as_str()).collect())
        .unwrap_or_default();
    match reasons.iter().next() {
        Some(one) if reasons.len() == 1 => one.to_string(),
        Some(_) => "The χ search was bounded from each scan's field strength and echo spacing.".to_string(),
        None => "The χ search was bounded from the field strength and echo spacing.".to_string(),
    }
}

/// Where χ-sepnet's toolbox route got its R2′: its own estimate unless it is given a measured one.
pub(super) fn chisepnet_r2prime_sentences(doc: &mut Draft, config: &PipelineConfig, ran: Ran) {
    let (measured, estimated) = (ran.count(Topic::ChiSeparationR2prime, Choice::Measured),
                                 ran.count(Topic::ChiSeparationR2prime, Choice::Predicted));
    if measured > 0 && estimated == 0 {
        doc.say(&[Stage::ChiSeparation], "χ-separation used the measured R2'.".to_string());
        return;
    }
    doc.cite_method(MethodStage::Relaxometry, "r2primenet");
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

/// The χ-separation method's name as the methods text gives it.
pub(crate) fn separation_name(alg: SeparationAlgorithm) -> &'static str {
    separation_label(alg)
}

/// Display name + citations for a source separation: the catalogue's name, or QSMxT's where it
/// says more (what R2*-QSM and the networks do, HC-ChiSep's name in full).
pub(super) fn separation_name_cite(alg: SeparationAlgorithm) -> (&'static str, Vec<Citation>) {
    (separation_label(alg), method(MethodStage::Separation, alg.as_str()))
}

fn separation_label(alg: SeparationAlgorithm) -> &'static str {
    match alg {
        SeparationAlgorithm::R2starQsm => "R2*-QSM susceptibility source separation",
        SeparationAlgorithm::HcChisep => "hollow-cylinder χ-separation (HC-ChiSep)",
        SeparationAlgorithm::SusepNet => "SUSEP-Net (deep-learning source separation)",
        SeparationAlgorithm::ChiSepNet => "χ-sepnet (deep-learning source separation)",
        _ => catalogue_method(MethodStage::Separation, alg.as_str()).name,
    }
}
