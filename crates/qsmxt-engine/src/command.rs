//! CLI command generation from PipelineConfig.

use serde_json::Value;

use crate::config::*;
use crate::enums::*;
use crate::schema::cli::{Flag, FlagAction, PIPELINE_FLAGS};
use crate::schema::cli_apply::{opposite_of, setting_of};
use crate::schema::{Setting, ValueType};

/// Generate a `qsmxt run` CLI command from a pipeline configuration.
/// Compares against defaults and only emits flags that differ.
///
/// Settings that do not apply to the configuration (the TUI hides them; see
/// [`crate::schema::settings_view`]) are left out: they cannot change the run, so saying them
/// only makes the command longer and suggests they matter.
///
/// A config that follows a toolbox pipeline ([`crate::pipelines::followed`]) starts with
/// `--pipeline <id>`, and its other flags are where it departs from that pipeline.
pub fn generate_command(config: &PipelineConfig) -> String {
    let facts = crate::schema::ViewFacts::default();
    let pruned = crate::schema::prune_hidden(&config.normalised().0, &facts);
    let pipeline = crate::pipelines::followed(config, &facts).and_then(|id| {
        let base = crate::pipelines::target(&PipelineConfig::default(), id, &facts).ok()?;
        Some((id, crate::schema::prune_hidden(&base.normalised().0, &facts)))
    });
    match pipeline {
        Some((id, base)) => command_for(&pruned, &base, Some(id)),
        None => command_for(&pruned, &PipelineConfig::default(), None),
    }
}



/// The flags a command says, in the order it says them: roughly the order the run does things
/// in, with each method's settings after the flag that chooses it. (QSMART's inner inversion's
/// settings come between QSMART's own, after the three that choose and cap it.) The opposites of
/// switches (`--no-x` of `--x`) are said at their switch's place. A registry flag not listed here
/// is said after the flag of the registry entry before it ([`emit_order`]).
const EMIT_ORDER: &[&str] = &[
    "pipeline", "orientation-group", "orientation-reference", "multi-orientation-algorithm",
    "multi-orientation-lambda", "cosmos-magnitude-weighting", "cosmos-tol", "cosmos-max-iter",
    "multi-orientation-force", "no-orientation-registration", "no-qsm", "do-swi", "no-magnitude",
    "do-segmentation", "do-analysis", "synthseg-version", "synthseg-crop", "synthseg-patch",
    "no-synthseg-flip-averaging", "no-synthseg-topology-cleanup", "synthseg-sigma", "use-custom-dseg",
    "do-smwi", "smwi-threshold", "smwi-power", "smwi-mip-window", "do-t2starmap", "do-r2primemap", "do-r2map",
    "do-r2starmap", "do-chisep", "use-custom-qsm", "use-custom-r2", "use-custom-r2prime", "r2prime-strategy",
    "r2primenet-patch", "r2primenet-ignore-field-mismatch", "export-dicom", "axial-resampling",
    "obliquity-threshold", "crop-to-mask", "ignore-slice-gap", "fft-padding", "tukey-window",
    "crop-margin-mm", "output-space", "no-inhomogeneity-correction", "homogeneity-sigma-mm",
    "homogeneity-nbox", "inhomogeneity-method", "n4-shrink-factor", "n4-max-iterations",
    "n4-convergence-threshold", "n4-mesh", "n4-spline-order", "n4-bias-field-fwhm", "n4-wiener-noise",
    "n4-histogram-bins", "n4-geometry", "use-custom-masks", "phase-offset-removal", "phase-offset-sigma-mm",
    "coil-combination-sigma", "bipolar-correction", "unwrapping-algorithm", "laplacian-solver",
    "laplacian-fft-pad", "laplacian-unmasked", "laplacian-keep-outside", "prelude-labelling",
    "prelude-partitions", "motion-correction", "motion-reference", "no-motion-chain", "motion-chain-levels",
    "motion-min-ncc", "registration-levels", "registration-max-samples", "registration-max-iterations",
    "registration-max-evaluations", "registration-tolerance", "registration-rotation-step-deg",
    "registration-translation-step-mm", "registration-min-overlap", "b0-estimation",
    "qsmart-fit-threshold-mode", "qsmart-fit-percentile", "qsmart-fit-threshold", "b0-weight-type",
    "b0-weight-assumed-t2star", "linear-fit-estimate-offset", "robust-fit-loss", "robust-fit-tuning",
    "robust-fit-iterations", "robust-fit-downweight-level", "robust-fit-flag-ratio", "complex-fit-reltol",
    "complex-fit-max-iter", "romeo-individual", "romeo-correct-global", "no-romeo-phase-coherence",
    "no-romeo-phase-gradient-coherence", "no-romeo-phase-linearity", "no-romeo-mag-coherence",
    "romeo-mag-weight", "romeo-mag-weight2", "romeo-bestpath", "romeo-merge-regions", "romeo-correct-regions",
    "romeo-template", "romeo-temporal-uncertain-unwrapping", "romeo-max-seeds", "romeo-wrap-addition",
    "mask-preset", "masking-input", "mask", "mask-combine", "mask-refine", "two-pass", "two-pass-mask",
    "two-pass-mask-combine", "two-pass-mode", "bet-fractional-intensity", "bet-smoothness",
    "bet-gradient-threshold", "bet-iterations", "bet-subdivisions", "bet-voxel-scale", "bet-orientation",
    "bf-algorithm", "bf-erode-voxels", "bf-polish-mask", "bf-ukb-mask-trim", "vsharp-deconvolve",
    "vsharp-threshold", "vsharp-max-radius", "vsharp-min-radius", "vsharp-radius-step", "vsharp-grid",
    "vsharp-grid-spacing", "pdf-tol", "pdf-n-cg", "pdf-space", "pdf-n-pad", "lbv-tol", "lbv-depth",
    "lbv-peel", "lbv-n1", "lbv-n2", "lbv-n3", "lbv-precision", "ismv-variant", "ismv-tol", "ismv-max-iter",
    "ismv-radius", "sharp-threshold", "sharp-radius", "resharp-radius", "resharp-tik-reg", "resharp-tol",
    "resharp-max-iter", "harperella-radius", "harperella-max-iter", "harperella-tol", "harperella-pad",
    "harperella-rim", "iharperella-max-iter", "iharperella-tol", "iharperella-pad", "iharperella-rim",
    "qsm-algorithm", "qsmart-inversion", "qsmart-ilsqr-max-iter", "qsmart-ilsqr-pad-mm", "rts-delta",
    "rts-mu", "rts-rho", "rts-tol", "rts-max-iter", "rts-lsmr-iter", "rts-tau", "rts-gamma", "rts-pad",
    "tv-lambda", "tv-rho", "tv-tol", "tv-max-iter", "tv-pad", "tkd-threshold", "tkd-pad", "tsvd-threshold",
    "tsvd-pad", "tikhonov-lambda", "tikhonov-pad", "nltv-lambda", "nltv-rho", "nltv-mu", "nltv-tol",
    "nltv-max-iter", "nltv-tol-newton", "nltv-newton-iter", "nltv-te", "nltv-pad", "medi-lambda",
    "medi-percentage", "medi-max-iter", "medi-cg-max-iter", "medi-cg-tol", "medi-tol", "medi-smv-radius",
    "no-medi-smv-shrink-mask", "medi-merit", "medi-data-weighting", "medi-csf", "medi-lambda-csf",
    "medi-csf-r2s-threshold", "medi-csf-erode", "medi-zeropad", "no-medi-smv", "medi-msmv", "medi-msmv-maxk",
    "medi-msmv-vessel-radius", "no-medi-msmv-prefilter", "medi-te", "tfi-lambda", "tfi-precond",
    "tfi-data-weighting", "tfi-merit", "tfi-linear", "tfi-percentage", "tfi-max-iter", "tfi-cg-max-iter",
    "tfi-cg-tol", "tfi-tol", "tfi-te", "ilsqr-tol", "ilsqr-max-iter", "ilsqr-pad-mm", "ilsqr-precision",
    "star-pad", "star-te", "lsqr-residual-weighting", "no-lsqr-global-offset", "lsqr-tol", "lsqr-max-iter",
    "tgv-iterations", "tgv-erosions", "tgv-alpha0", "tgv-alpha1", "tgv-step-size", "tgv-te", "ndi-tau",
    "ndi-alpha", "ndi-max-iter", "ndi-auto-stop", "ndi-te", "ndi-weight-norm", "fansi-alpha1", "fansi-mu1",
    "fansi-mu2", "fansi-alpha0", "fansi-mu0", "fansi-max-iter", "fansi-tol-update", "fansi-tol-delta",
    "fansi-te", "l1qsm-alpha1", "l1qsm-mu1", "l1qsm-mu2", "l1qsm-mu3", "l1qsm-lambda", "l1qsm-max-iter",
    "l1qsm-tol-update", "l1qsm-tol-delta", "l1qsm-te", "whqsm-alpha1", "whqsm-mu1", "whqsm-mu2", "whqsm-beta",
    "whqsm-muh", "whqsm-max-iter", "whqsm-tol-update", "whqsm-tol-delta", "whqsm-weight", "whqsm-te",
    "hdqsm-alpha-l2", "hdqsm-mu1-l2", "hdqsm-mu2", "hdqsm-max-iter-l1", "hdqsm-max-iter-l2",
    "hdqsm-tol-update", "amp-pe-wave-order", "amp-pe-nlevel", "amp-pe-wave-pec", "amp-pe-simulated-te",
    "amp-pe-max-linearization-ite", "amp-pe-damp-rate-sig", "amp-pe-damp-rate-par", "amp-pe-max-pe-spar-ite",
    "amp-pe-max-pe-est-ite", "amp-pe-cvg-thd", "amp-pe-tikhonov-beta", "qsmart-vasc-sphere-radius",
    "qsmart-sdf-sigma1-stage1", "qsmart-sdf-sigma2-stage1", "qsmart-sdf-sigma1-stage2",
    "qsmart-sdf-sigma2-stage2", "qsmart-sdf-lower-lim", "qsmart-sdf-curv-constant", "qsmart-frangi-scale-min",
    "qsmart-frangi-scale-max", "qsmart-frangi-scale-ratio", "qsmart-frangi-c-mode", "qsmart-frangi-c",
    "qsmart-frangi-c-factor", "qsmart-vessel-units", "qsmart-vessel-magnitude", "qsmart-offset-grid", "heidi",
    "heidi-cone-threshold", "heidi-gradient-threshold", "no-heidi-laplacian-correction",
    "heidi-laplacian-threshold", "heidi-gradient-mask-floor", "heidi-continuation-steps",
    "heidi-inner-iterations", "heidi-mu-min", "heidi-tol", "no-heidi-denoise", "heidi-denoise-iterations",
    "heidi-denoise-time-step", "heidi-denoise-conductance", "heidi-denoise-reference-itk", "heidi-te",
    "tile-size", "tile-halo", "chisep", "r2star-qsm-r-const-3t", "decompose-n-inner", "decompose-chi-bound",
    "decompose-unwrap-phase", "decompose-fit", "decompose-max-iterations",
    "decompose-max-function-evaluations", "decompose-function-tolerance", "decompose-step-tolerance",
    "decompose-optimality-tolerance", "decompose-finite-difference-step", "decompose-typical-x",
    "chi-sep-ilsqr-dr-pos", "chi-sep-ilsqr-dr-neg", "chi-sep-ilsqr-pad", "chi-sep-ilsqr-cg-max-iter-lsqr",
    "chi-sep-ilsqr-cg-tol-lsqr", "chi-sep-ilsqr-cg-max-iter-sa", "chi-sep-ilsqr-cg-tol-sa",
    "chi-sep-ilsqr-d2-thr", "chi-sep-ilsqr-prc-w-i-min", "chi-sep-ilsqr-prc-w-i-max",
    "chi-sep-ilsqr-prc-w-g-min", "chi-sep-ilsqr-prc-w-g-max", "chi-sep-ilsqr-qsm-max-iter",
    "chi-sep-ilsqr-qsm-tol", "chi-sep-ilsqr-qsm-pad-mm", "chi-sep-ilsqr-qsm-precision", "chi-sep-medi-lambda",
    "chi-sep-medi-lambda-csf", "chi-sep-medi-dr-pos", "chi-sep-medi-dr-neg", "chi-sep-medi-pad",
    "chi-sep-medi-tol", "chi-sep-medi-max-iter", "chi-sep-medi-cg-tol", "chi-sep-medi-cg-max-iter",
    "chi-sep-medi-qsm-max-iter", "chi-sep-medi-qsm-tol", "chi-sep-medi-qsm-pad-mm",
    "chi-sep-medi-qsm-precision", "chi-sep-medi-csf-r2star", "wavesep-dr-pos", "wavesep-dr-neg",
    "wavesep-alpha", "wavesep-lambda", "wavesep-wavelet-order", "wavesep-max-iter", "wavesep-tol",
    "hc-chisep-dr-pos-3t", "hc-chisep-bin-hz", "chi-sepnet-route", "chi-sepnet-patch",
    "chi-sepnet-frame-patch", "susep-net-patch", "qsm-reference", "qsm-per-echo", "swi-unwrapping",
    "swi-hp-sigma", "swi-scaling", "swi-strength", "swi-mip-window", "swi-filtered-phase",
    "dismiss-recommendation",
];

/// Every flag a command can say, in order: [`EMIT_ORDER`], and each registry flag it does not list
/// after the flag of the entry before it.
fn emit_order() -> &'static [&'static str] {
    static ORDER: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
        let registry: Vec<&'static str> =
            crate::schema::settings().iter().filter_map(|s| s.cli_flag).map(|f| f.trim_start_matches("--")).collect();
        crate::schema::cli::in_registry_place(EMIT_ORDER, &registry)
    });
    &ORDER
}

/// The command for `config`, with flags where it differs from `base`: QSMxT's defaults, or
/// `pipeline`'s settings. Most flags set one registry setting ([`setting_of`]) and are said
/// when that setting is shown and differs from `base`; the rest are spelled out below.
fn command_for(config: &PipelineConfig, base: &PipelineConfig, pipeline: Option<&str>) -> String {
    let mut parts: Vec<String> = vec!["qsmxt".into(), "run".into(), "<bids_dir>".into()];
    if let Some(id) = pipeline {
        parts.push(format!("--pipeline {id}"));
    }
    let visible: Vec<&str> = crate::schema::settings_view(config, &crate::schema::ViewFacts::default())
        .into_iter().filter(|v| v.visible).map(|v| v.path).collect();
    let (values, base_values) = (serde_json::to_value(config).unwrap_or_default(), serde_json::to_value(base).unwrap_or_default());
    let m = &config.multi_orientation;
    for long in emit_order() {
        let flag = PIPELINE_FLAGS.iter().find(|f| f.long == *long).expect("every flag said is in the table");
        match *long {
            // Said first, above.
            "pipeline" => {}
            // The pattern is what switches grouping on, so it and the reconstruction are said
            // whenever it is on; the patterns are quoted so that a shell leaves their globs alone.
            "orientation-group" if m.enabled() => parts.push(format!("--orientation-group {}", shell_quote(&m.group_by))),
            "orientation-reference" if m.enabled() && !m.reference.trim().is_empty() => {
                parts.push(format!("--orientation-reference {}", shell_quote(m.reference.trim())));
            }
            "multi-orientation-algorithm" if m.enabled() => parts.push(format!("--multi-orientation-algorithm {}", m.algorithm)),
            "orientation-group" | "orientation-reference" | "multi-orientation-algorithm" => {}
            // --do-analysis implies segmentation, so saying both adds noise.
            "do-segmentation" => {
                if config.pipeline.do_segmentation && !config.pipeline.do_analysis {
                    parts.push("--do-segmentation".into());
                }
            }
            // R2*, R2 and R2' are chained: chi-separation may imply R2', and computing R2'
            // implies R2* (and R2, where it can be measured). Each flag is said unless the rest
            // of the config implies it, asked of the same enforcement a run applies, so a flag
            // the chosen method does not imply (R2 under r2star-qsm, say) is never dropped.
            "do-r2primemap" | "do-r2map" | "do-r2starmap" => {
                let field: fn(&mut PipelineConfig) -> &mut bool = match *long {
                    "do-r2primemap" => |c| &mut c.pipeline.do_r2primemap,
                    "do-r2map" => |c| &mut c.pipeline.do_r2map,
                    _ => |c| &mut c.pipeline.do_r2starmap,
                };
                let mut cleared = config.clone();
                *field(&mut cleared) = false;
                crate::config::enforce_separation_dependencies(&mut cleared);
                if *field(&mut config.clone()) && !*field(&mut cleared) {
                    parts.push(format!("--{long}"));
                }
            }
            // Counted from 1 on the command line, from 0 in the config.
            "romeo-template" => {
                let (t, bt) = (config.field_mapping.romeo.template, base.field_mapping.romeo.template);
                if !config.field_mapping.romeo.individual && t != bt {
                    parts.push(format!("--romeo-template {}", t + 1));
                }
            }
            // One value when the three are the same.
            "laplacian-fft-pad" => {
                let pad = config.field_mapping.laplacian_fft_pad;
                if config.field_mapping.laplacian_solver == LaplacianSolver::Fft && pad != base.field_mapping.laplacian_fft_pad {
                    parts.push(if pad[0] == pad[1] && pad[1] == pad[2] {
                        format!("--laplacian-fft-pad {}", pad[0])
                    } else {
                        format!("--laplacian-fft-pad {} {} {}", pad[0], pad[1], pad[2])
                    });
                }
            }
            "mask-preset" => masking_flags(&mut parts, config, base),
            // Said with the preset, by `masking_flags`.
            "masking-input" | "mask" | "mask-combine" | "mask-refine" => {}
            // The reliable mask only needs spelling out once it differs from the default recipe.
            "two-pass-mask" => {
                if let Some(sections) = config.masking.two_pass_sections.as_ref().filter(|_| config.masking.two_pass) {
                    if *sections != crate::masking::default_two_pass_sections() {
                        for section in sections {
                            parts.push(format!("--two-pass-mask {}", section.compact_spec()));
                        }
                    }
                }
            }
            // `qsmxt` pushes these out to every BET step of the recipe; the config keeps them here.
            "bet-fractional-intensity" => emit_value(&mut parts, flag, &config.bet.fractional_intensity, &base.bet.fractional_intensity),
            "bet-voxel-scale" => emit_value(&mut parts, flag, &config.bet.voxel_scale, &base.bet.voxel_scale),
            // One flag carries both fields: the method, or the region spec that replaces it.
            "qsm-reference" => {
                let spec = config.qsm.reference_spec();
                if spec != base.qsm.reference_spec() {
                    parts.push(format!("--qsm-reference {spec}"));
                }
            }
            "dismiss-recommendation" => dismissal_flags(&mut parts, config),
            _ => {
                let Some(setting) = setting_of(flag) else { continue };
                let (v, b) = (lookup(&values, setting.path), lookup(&base_values, setting.path));
                if v != b && visible.contains(&setting.path) {
                    if let Some(arg) = render(flag, setting, &v) {
                        parts.push(arg);
                    }
                }
            }
        }
    }
    parts.join(" ")
}

/// The value at `path`, `null` for an unset optional value (left out of the serialised config).
fn lookup(config: &Value, path: &str) -> Value {
    path.split('.').try_fold(config, |v, k| v.get(k)).cloned().unwrap_or(Value::Null)
}

/// `--flag value` for a setting `qsmxt` keeps outside the registry's flags.
fn emit_value(parts: &mut Vec<String>, flag: &Flag, value: &f64, base: &f64) {
    if value != base {
        parts.push(format!("--{} {value}", flag.long));
    }
}

/// How `flag` says `value` of `setting`, or `None` where it cannot: an unset optional value is
/// the one qsm-core derives, said by leaving the flag out, and a switch is said only by a flag
/// for its direction.
fn render(flag: &Flag, setting: &Setting, value: &Value) -> Option<String> {
    let long = flag.long;
    let text = match (value, &setting.value_type) {
        (Value::Null, _) => return None,
        (Value::Bool(on), ValueType::Bool) if flag.action == FlagAction::SetTrue => {
            // `--x` turns it on and `--no-x` off; the other direction is its opposite's.
            if *on != long.starts_with("no-") {
                return Some(format!("--{long}"));
            }
            return PIPELINE_FLAGS.iter().find(|f| opposite_of(f).is_some_and(|(o, _)| o.long == long))
                .map(|f| format!("--{}", f.long));
        }
        (v, ValueType::EchoTime { .. }) => serde_json::from_value::<EchoTimeChoice>(v.clone()).ok()?.to_string(),
        (v, ValueType::NumberOrChoice { .. }) => serde_json::from_value::<ChiBoundChoice>(v.clone()).ok()?.to_string(),
        (v, ValueType::EchoReference { .. }) => serde_json::from_value::<crate::MotionReference>(v.clone()).ok()?.to_string(),
        // A bare flag stands for the value it takes when given without one (`*`, any tool).
        (Value::String(s), _) if flag.default_missing.contains(&s.as_str()) => return Some(format!("--{long}")),
        (Value::Array(xs), _) => xs.iter().map(scalar).collect::<Vec<_>>().join(" "),
        (v, _) => scalar(v),
    };
    Some(format!("--{long} {text}"))
}

/// A number as Rust writes it (`100`, not `100.0`), and other values as they are.
fn scalar(v: &Value) -> String {
    match v {
        Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
            (Some(u), _, _) => u.to_string(),
            (_, Some(i), _) => i.to_string(),
            (_, _, Some(x)) => x.to_string(),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The mask recipe: a preset says in three words what spelling the sections out takes a line
/// and a half to say, so the preset is said whenever the configuration is one (and the input
/// it is read from, which only ever qualifies a preset).
fn masking_flags(parts: &mut Vec<String>, config: &PipelineConfig, base: &PipelineConfig) {
    let (c, b) = (&config.masking, &base.masking);
    // The default recipe is a preset too; saying so adds noise rather than removing it.
    if c.sections == b.sections && c.combine == b.combine && c.refinements == b.refinements {
        return;
    }
    match crate::masking::masking_preset_command(c) {
        Some((preset, input)) => {
            parts.push(format!("--mask-preset {preset}"));
            if let Some(input) = input {
                parts.push(format!("--masking-input {input}"));
            }
        }
        None => {
            for section in &c.sections {
                parts.push(format!("--mask {}", section.compact_spec()));
            }
            if c.combine != b.combine {
                parts.push(format!("--mask-combine {}", c.combine));
            }
            for op in &c.refinements {
                parts.push(format!("--mask-refine {}", op.compact_spec()));
            }
        }
    }
}

/// The dismissed recommendations that still hold: a dismissal whose setting has since changed
/// is gone.
fn dismissal_flags(parts: &mut Vec<String>, config: &PipelineConfig) {
    let mut held = config.dismissed_recommendations.clone();
    if !held.is_empty() {
        let mut c = config.clone();
        crate::recommendations::prune_dismissals(&mut c, &crate::recommendations::Facts::default());
        held = c.dismissed_recommendations;
    }
    for x in &held {
        let arg = format!("{}={}", x.rule, x.chosen);
        // Rule ids and dismissed values are written in characters a shell leaves alone.
        let plain = arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_.=,/".contains(c));
        parts.push(format!("--dismiss-recommendation {}", if plain { arg } else { shell_quote(&arg) }));
    }
}

#[cfg(test)]
mod every_flag_tests;

#[cfg(test)]
mod tests {
    /// A mask supplied for the run (qsmbly writes it to derivatives/qsmbly/, `crate::supplied`)
    /// is read back with `--use-custom-masks qsmbly`.
    #[test]
    fn a_supplied_mask_is_read_from_its_tools_derivatives() {
        let mut config = crate::config::PipelineConfig::default();
        assert!(!super::generate_command(&config).contains("--use-custom-masks"));
        config.masking.custom_mask_tool = Some("qsmbly".into());
        assert!(super::generate_command(&config).contains("--use-custom-masks qsmbly"));
        config.masking.custom_mask_tool = Some("*".into());
        let command = super::generate_command(&config);
        assert!(command.contains("--use-custom-masks") && !command.contains("--use-custom-masks *"), "{command}");
    }

    /// A relaxometry map the selected χ-separation method does not need is still asked for on
    /// the command line: it used to be dropped whenever χ-separation was on.
    #[test]
    fn relaxometry_flags_are_dropped_only_when_implied() {
        use crate::enums::SeparationAlgorithm;
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::R2starQsm;
        c.pipeline.do_r2map = true;
        enforce_separation_dependencies(&mut c);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-r2map"), "R2 is not implied by r2star-qsm: {cmd}");
        assert!(!cmd.contains("--do-r2starmap"), "R2* is: {cmd}");
        // R2'-based separation implies R2' and with it R2* and R2: none spelled out.
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        enforce_separation_dependencies(&mut c);
        let cmd = generate_command(&c);
        for f in ["--do-r2primemap", "--do-r2map", "--do-r2starmap"] {
            assert!(!cmd.contains(f), "{f} is implied: {cmd}");
        }
    }

    use super::*;

    #[test]
    fn test_default_config_minimal_command() {
        let config = PipelineConfig::default();
        let cmd = generate_command(&config);
        assert_eq!(cmd, "qsmxt run <bids_dir>");
    }

    /// A preclinical run has to be reproducible from the command the TUI prints: the scaling
    /// shows up both as the recipe-wide flag and in the BET step's own spec.
    #[test]
    fn test_bet_voxel_scale_command() {
        let mut config = PipelineConfig::default();
        config.bet.voxel_scale = 10.0;
        config.masking.sections = vec![crate::masking::MaskSection {
            input: crate::masking::MaskingInput::Magnitude,
            generator: crate::masking::MaskOp::Bet { fractional_intensity: 0.5, voxel_scale: 10.0 },
            refinements: vec![],
        }];
        let cmd = generate_command(&config);
        assert!(cmd.contains("--bet-voxel-scale 10"), "cmd: {cmd}");
        assert!(cmd.contains("--mask magnitude,bet:0.50:scale=10"), "cmd: {cmd}");

        // The default is silent — the clinical command line is unchanged.
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.contains("--bet-voxel-scale"), "cmd: {cmd}");
        assert!(!cmd.contains("scale="), "cmd: {cmd}");
    }

    #[test]
    fn test_changed_algorithm() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tv;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--qsm-algorithm tv"));
    }

    #[test]
    fn test_dl_algorithms_command() {
        use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};
        for a in [
            QsmAlgorithm::Xqsm, QsmAlgorithm::Qsmnet, QsmAlgorithm::QsmnetPlus, QsmAlgorithm::Qsmnetme,
            QsmAlgorithm::Autoqsm, QsmAlgorithm::Qsmgan, QsmAlgorithm::Ir2qsm,
            QsmAlgorithm::Lpcnn, QsmAlgorithm::ModlQsm, QsmAlgorithm::Nextqsm,
            QsmAlgorithm::Iqsm, QsmAlgorithm::IqsmPlus,
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = a;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--qsm-algorithm {}", a)), "missing {} in: {}", a, cmd);
        }
        // iQFM background removal + χ-sepnet separation (deep-learning; patches in the profile tests).
        let mut c = PipelineConfig::default();
        c.bg_removal.algorithm = BfAlgorithm::Iqfm;
        assert!(generate_command(&c).contains("--bf-algorithm iqfm"));
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        assert!(generate_command(&c).contains("--chisep chi-sepnet"));
    }

    #[test]
    fn test_dl_tiling_command() {
        use crate::enums::QsmAlgorithm;
        // Off by default: no tiling flags for a DL algorithm.
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--tile-size"), "got: {}", cmd);
        assert!(!cmd.contains("--tile-halo"), "got: {}", cmd);
        // Setting tile_size enables tiling; halo emitted only when set.
        c.inversion.tile_size = Some(64);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tile-size 64"), "got: {}", cmd);
        assert!(!cmd.contains("--tile-halo"), "got: {}", cmd);
        c.inversion.tile_halo = Some(4);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tile-size 64") && cmd.contains("--tile-halo 4"), "got: {}", cmd);
        // Native-patch nets ignore tiling options (no flags emitted).
        c.inversion.algorithm = QsmAlgorithm::Autoqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--tile-size"), "autoqsm should not emit tiling: {}", cmd);
        // A classical inversion after a tiled background-removal network keeps them.
        c.inversion.algorithm = QsmAlgorithm::Tkd;
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Bfrnet;
        assert!(generate_command(&c).contains("--tile-size 64"), "BFRnet tiles");
        // The phase-input reconstructions tile too.
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Vsharp;
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        assert!(generate_command(&c).contains("--tile-size 64"), "iQSM tiles");
    }

    /// MEDI-mSMV is MEDI's option: said only with MEDI, its settings only when it is on and
    /// changed.
    #[test]
    fn test_medi_msmv_command() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("msmv"));
        config.inversion.medi.msmv = true;
        config.inversion.medi.msmv_maxk = 9;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--medi-msmv") && cmd.contains("--medi-msmv-maxk 9"), "got: {cmd}");
        assert!(!cmd.contains("--medi-msmv-vessel-radius") && !cmd.contains("prefilter"), "got: {cmd}");
        config.inversion.medi.msmv_vessel_radius = Some(12.0);
        config.inversion.medi.msmv_prefilter = false;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--medi-msmv-vessel-radius 12") && cmd.contains("--no-medi-msmv-prefilter"), "got: {cmd}");
        // Under another inversion it does not apply.
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(!generate_command(&config).contains("msmv"));
    }

    /// A command names the background removal whenever it is not the general default, under
    /// every inversion: nothing depends on the inversion.
    #[test]
    fn the_background_removal_is_named_unless_it_is_the_default() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("--bf-algorithm"), "{}", generate_command(&config));
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        assert!(generate_command(&config).contains("--bf-algorithm pdf"));
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(generate_command(&config).contains("--bf-algorithm pdf"));
    }

    #[test]
    fn medi_smv_shrink_mask_command() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("shrink"));
        config.inversion.medi.smv_shrink_mask = false;
        assert!(generate_command(&config).contains("--no-medi-smv-shrink-mask"));
    }

    #[test]
    fn test_chi_separation_command() {
        // Default method (r2star-qsm) uses R2* directly — it forces R2* only, NOT R2/R2'.
        let mut config = PipelineConfig::default();
        config.pipeline.do_chi_separation = true;
        enforce_separation_dependencies(&mut config);
        assert!(config.pipeline.do_r2starmap);
        assert!(!config.pipeline.do_r2map && !config.pipeline.do_r2primemap);
        // Those maps are implied by --do-chisep and must not appear in the command.
        let cmd = generate_command(&config);
        assert!(cmd.contains("--do-chisep"));
        assert!(!cmd.contains("--do-r2starmap"), "r2starmap implied by chisep, got: {}", cmd);
        assert!(!cmd.contains("--do-r2map"));
        assert!(!cmd.contains("--do-r2primemap"));
        assert!(!cmd.contains("--chisep")); // default method → not emitted

        // An R2'-based method (wavesep) forces R2' → R2 → R2*, none of which should leak.
        let mut wc = PipelineConfig::default();
        wc.pipeline.do_chi_separation = true;
        wc.separation.algorithm = SeparationAlgorithm::WaveSep;
        enforce_separation_dependencies(&mut wc);
        assert!(wc.pipeline.do_r2primemap && wc.pipeline.do_r2map && wc.pipeline.do_r2starmap);
        let cmd = generate_command(&wc);
        assert!(cmd.contains("--chisep wavesep"));
        assert!(!cmd.contains("--do-r2map") && !cmd.contains("--do-r2primemap") && !cmd.contains("--do-r2starmap"));

        // Selecting a non-default method emits it; editing a param emits that too.
        config.separation.algorithm = SeparationAlgorithm::Decompose;
        config.separation.decompose.lsqcurvefit.max_iterations = 99;
        config.separation.decompose.unwrap_phase = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--chisep decompose"));
        assert!(cmd.contains("--decompose-max-iterations 99") && cmd.contains("--decompose-unwrap-phase true"), "{cmd}");
        assert!(!cmd.contains("--decompose-chi-bound"), "{cmd}");
        config.separation.decompose.chi_bound = crate::ChiBoundChoice::EchoSpacing;
        assert!(generate_command(&config).contains("--decompose-chi-bound echo-spacing"));
        config.separation.decompose.chi_bound = crate::ChiBoundChoice::Ppm(0.3);
        assert!(generate_command(&config).contains("--decompose-chi-bound 0.3"));

        // A param for a non-selected method must NOT leak into the command.
        assert!(!cmd.contains("--wavesep-alpha"));
    }

    #[test]
    fn test_changed_rts_param() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.inversion.rts.delta = 0.2;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--rts-delta 0.2"));
    }

    #[test]
    fn test_phase_offset_disabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.phase_offset_removal = false;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--phase-offset-removal false"));
    }

    #[test]
    fn test_bipolar_enabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.bipolar_correction = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--bipolar-correction"));
    }

    #[test]
    fn test_romeo_individual_mode() {
        let mut config = PipelineConfig::default();
        // Template mode is the default: neither the mode nor the template echo is stated.
        assert!(!generate_command(&config).contains("romeo-individual"));
        assert!(!generate_command(&config).contains("--romeo-template"));
        config.field_mapping.romeo.individual = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--romeo-individual") && !cmd.contains("--romeo-template"), "{cmd}");
    }

    #[test]
    fn test_swi_params_only_when_enabled() {
        let mut config = PipelineConfig::default();
        config.swi.strength = 8.0;
        // SWI off — no SWI flags
        let cmd = generate_command(&config);
        assert!(!cmd.contains("--swi-strength"));
        // SWI on — flag appears
        config.pipeline.do_swi = true;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--swi-strength 8"));
        assert!(!cmd.contains("--swi-unwrapping"), "{cmd}");
        config.swi.unwrap = crate::SwiUnwrap::Romeo;
        assert!(generate_command(&config).contains("--swi-unwrapping romeo"));
        assert!(!generate_command(&config).contains("--swi-filtered-phase"));
        config.swi.filtered_phase = true;
        assert!(generate_command(&config).contains("--swi-filtered-phase"));
        // Without SWI there is no filtered phase to write.
        config.pipeline.do_swi = false;
        assert!(!generate_command(&config).contains("--swi-filtered-phase"));
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = PipelineConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = PipelineConfig::from_toml(&toml).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Ilsqr);
        assert_eq!(parsed.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
        assert!(parsed.field_mapping.phase_offset_removal);
        assert!(!parsed.field_mapping.romeo.individual);
    }

    #[test]
    fn test_all_inversion_algorithms() {
        for (alg, name) in [
            (QsmAlgorithm::Tv, "tv"), (QsmAlgorithm::Tkd, "tkd"),
            (QsmAlgorithm::Tsvd, "tsvd"), (QsmAlgorithm::Tgv, "tgv"),
            (QsmAlgorithm::Tikhonov, "tikhonov"), (QsmAlgorithm::Nltv, "nltv"),
            (QsmAlgorithm::Medi, "medi"), (QsmAlgorithm::Tfi, "tfi"),
            (QsmAlgorithm::Rts, "rts"),
            (QsmAlgorithm::Qsmart, "qsmart"),
            (QsmAlgorithm::Ndi, "ndi"), (QsmAlgorithm::Fansi, "fansi"),
            (QsmAlgorithm::FansiTgv, "fansi-tgv"), (QsmAlgorithm::L1qsm, "l1qsm"),
            (QsmAlgorithm::Whqsm, "whqsm"), (QsmAlgorithm::Hdqsm, "hdqsm"),
            (QsmAlgorithm::AmpPe, "amp-pe"),
        ] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--qsm-algorithm {}", name)), "missing algorithm flag for {}", name);
        }
    }

    #[test]
    fn test_all_bf_algorithms() {
        for (alg, name) in [
            (BfAlgorithm::Pdf, "pdf"), (BfAlgorithm::Lbv, "lbv"),
            (BfAlgorithm::Ismv, "ismv"), (BfAlgorithm::Sharp, "sharp"),
            (BfAlgorithm::Resharp, "resharp"), (BfAlgorithm::Harperella, "harperella"),
            (BfAlgorithm::Iharperella, "iharperella"),
        ] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--bf-algorithm {}", name)), "missing bf flag for {}", name);
        }
    }

    #[test]
    fn test_tv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tv;
        c.inversion.tv.lambda = 0.002;
        c.inversion.tv.max_iter = 100;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tv-lambda 0.002"));
        assert!(cmd.contains("--tv-max-iter 100"));
        // ρ unset follows λ (qsm-core's 100·λ): no flag. Set, it is said.
        assert!(!cmd.contains("--tv-rho"), "{cmd}");
        c.inversion.tv.rho = Some(0.5);
        assert!(generate_command(&c).contains("--tv-rho 0.5"));
    }

    #[test]
    fn test_medi_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        c.inversion.medi.lambda = 999.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--medi-lambda 999"));
    }

    #[test]
    fn test_tgv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tgv;
        c.inversion.tgv.iterations = Some(500);
        c.inversion.tgv.erosions = 2;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tgv-iterations 500"));
        assert!(cmd.contains("--tgv-erosions 2"));
    }

    /// Each method's echo time, and the method settings from QSM.rs v0.40.0, are emitted only away
    /// from their defaults; a given echo time is written in ms.
    #[test]
    fn echo_times_and_method_settings_are_emitted_when_changed() {
        let methods = [
            QsmAlgorithm::Nltv, QsmAlgorithm::Medi, QsmAlgorithm::Tfi, QsmAlgorithm::Ndi,
            QsmAlgorithm::Fansi, QsmAlgorithm::FansiTgv, QsmAlgorithm::L1qsm, QsmAlgorithm::Whqsm,
            QsmAlgorithm::Lsqr, QsmAlgorithm::Tgv,
        ];
        for alg in methods {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = alg;
            // LSQR's echo time is HEIDI's.
            c.inversion.heidi.enabled = alg == QsmAlgorithm::Lsqr;
            let flag = match alg {
                QsmAlgorithm::FansiTgv => "fansi".to_string(),
                QsmAlgorithm::Lsqr => "heidi".to_string(),
                _ => alg.to_string(),
            };
            assert!(!generate_command(&c).contains("-te "), "{alg}: the default is not emitted");
            let i = &mut c.inversion;
            for te in [&mut i.nltv.te, &mut i.medi.te, &mut i.tfi.te, &mut i.ndi.te, &mut i.fansi.te,
                       &mut i.l1qsm.te, &mut i.whqsm.te, &mut i.heidi.te, &mut i.tgv.te] {
                *te = EchoTimeChoice::Seconds(0.007);
            }
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("--{flag}-te 7")), "{alg}: {cmd}");
        }
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        c.inversion.whqsm.weight = WhqsmWeight::InVivo;
        assert!(generate_command(&c).contains("--whqsm-weight in-vivo"));
        c.inversion.algorithm = QsmAlgorithm::Nltv;
        c.inversion.nltv.rho = Some(0.5);
        c.inversion.nltv.tol_newton = 1e-4;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--nltv-rho 0.5") && cmd.contains("--nltv-tol-newton 0.0001"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.tau = 0.5;
        c.inversion.rts.gamma = 2.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--rts-tau 0.5") && cmd.contains("--rts-gamma 2"), "{cmd}");
        c.bg_removal.algorithm = BfAlgorithm::Pdf;
        c.bg_removal.pdf.n_cg = 300;
        c.bg_removal.pdf.space = PdfSpace::Kspace;
        c.bg_removal.pdf.n_pad = 0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--pdf-n-cg 300") && cmd.contains("--pdf-space kspace") && cmd.contains("--pdf-n-pad 0"), "{cmd}");
        for (alg, flag) in [(BfAlgorithm::Harperella, "--harperella-pad"), (BfAlgorithm::Iharperella, "--iharperella-pad")] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            assert!(!generate_command(&c).contains(flag));
            c.bg_removal.harperella.pad = [2, 4, 6];
            c.bg_removal.iharperella.pad = [2, 4, 6];
            assert!(generate_command(&c).contains(&format!("{flag} 2 4 6")));
        }
        // the boundary rim, NDI's weight normaliser and WH-QSM's flat-in-mask weight: only when
        // they differ from the method's default
        for (alg, flag, rim) in [(BfAlgorithm::Harperella, "--harperella-rim", HarperellaRim::Sti),
                                 (BfAlgorithm::Iharperella, "--iharperella-rim", HarperellaRim::Whole)] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            assert!(!generate_command(&c).contains("-rim"));
            c.bg_removal.harperella.rim = rim;
            c.bg_removal.iharperella.rim = rim;
            assert!(generate_command(&c).contains(&format!("{flag} {rim}")), "{}", generate_command(&c));
        }
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--bf-erode-voxels"));
        c.bg_removal.erode_voxels = 3;
        assert!(generate_command(&c).contains("--bf-erode-voxels 3"), "{}", generate_command(&c));
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Ndi;
        assert!(!generate_command(&c).contains("--ndi-weight-norm"));
        c.inversion.ndi.weight_norm = NdiWeightNorm::MaskP999;
        assert!(generate_command(&c).contains("--ndi-weight-norm mask-p999"));
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        c.inversion.whqsm.weight = WhqsmWeight::FlatInMask;
        assert!(generate_command(&c).contains("--whqsm-weight flat-in-mask"));
    }

    /// The new settings in a config file: an echo time as a name or `{ seconds = … }`, the WH-QSM
    /// weight and PDF's kernel by their names, and the padding as three numbers.
    #[test]
    fn echo_times_and_method_settings_in_config_files() {
        let c: PipelineConfig = toml::from_str("[inversion.medi]\nte = \"first\"\n[inversion.tgv]\nte = { seconds = 0.03 }\n\
            [inversion.whqsm]\nweight = \"max-normalised\"\n[bg_removal.pdf]\nspace = \"kspace\"\n\
            [bg_removal.harperella]\npad = [1, 2, 3]\n").unwrap();
        assert_eq!(c.inversion.medi.te, EchoTimeChoice::First);
        assert_eq!(c.inversion.tgv.te, EchoTimeChoice::Seconds(0.03));
        assert_eq!(c.inversion.whqsm.weight, WhqsmWeight::MaxNormalised);
        assert_eq!(c.bg_removal.pdf.space, PdfSpace::Kspace);
        assert_eq!(c.bg_removal.harperella.pad, [1, 2, 3]);
        assert!(toml::from_str::<PipelineConfig>("[inversion.ndi]\nte = \"median\"\n").is_err());
        let back: PipelineConfig = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn test_qsmart_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.vasc_sphere_radius = 10;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-vasc-sphere-radius 10"));
    }

    /// Under QSMART the default mask goes unsaid, as elsewhere: it is not replaced.
    #[test]
    fn qsmart_keeps_the_default_mask() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        assert!(!generate_command(&c).contains("--mask"), "{}", generate_command(&c));
        c.masking.sections = crate::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections;
        assert!(generate_command(&c).contains("--mask-preset bet"), "{}", generate_command(&c));
    }

    /// Under QSMART the command says the field mapping it was given, as elsewhere; QSMART's vessel
    /// options are said when changed.
    #[test]
    fn qsmart_field_mapping_and_vessel_options_are_in_the_command() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--unwrapping-algorithm") && !cmd.contains("--b0-estimation"), "{cmd}");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.laplacian_solver = LaplacianSolver::Mirror;
        c.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        let cmd = generate_command(&c);
        for flag in ["--unwrapping-algorithm laplacian", "--laplacian-solver mirror", "--b0-estimation qsmart-echofit"] {
            assert!(cmd.contains(flag), "{cmd}");
        }
        assert!(!cmd.contains("--qsmart-fit-threshold") && !cmd.contains("--qsmart-vessel"), "{cmd}");
        assert!(!cmd.contains("--laplacian-unmasked"), "{cmd}");
        c.field_mapping.laplacian_unmasked = true;
        assert!(generate_command(&c).contains("--laplacian-unmasked"));
        c.inversion.qsmart.vessel_magnitude = QsmartVesselMagnitude::Combined;
        c.inversion.qsmart.vessel_size_units = VesselSizeUnits::Mm;
        let cmd = generate_command(&c);
        for flag in ["--qsmart-vessel-magnitude combined", "--qsmart-vessel-units mm"] {
            assert!(cmd.contains(flag), "{cmd}");
        }
        // The default fixed threshold (QSMART v1.0's 40): nothing; the threshold when changed.
        c.field_mapping.qsmart_fit_threshold = 30.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-fit-threshold 30") && !cmd.contains("--qsmart-fit-threshold-mode")
                && !cmd.contains("--qsmart-fit-percentile"), "{cmd}");
        // A percentile: the mode, and the percentile when changed; not the threshold.
        c.field_mapping.qsmart_fit_threshold_mode = QsmartFitThresholdMode::Percentile;
        c.field_mapping.qsmart_fit_percentile = 95.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-fit-threshold-mode percentile") && cmd.contains("--qsmart-fit-percentile 95")
                && !cmd.contains("--qsmart-fit-threshold "), "{cmd}");
    }

    #[test]
    fn test_qsmart_all_params_emitted() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.inversion = QsmAlgorithm::Tv;
        c.inversion.qsmart.sdf_sigma1_stage1 = 11.0;
        c.inversion.qsmart.sdf_sigma2_stage1 = 12.0;
        c.inversion.qsmart.sdf_sigma1_stage2 = 13.0;
        c.inversion.qsmart.sdf_sigma2_stage2 = 14.0;
        c.inversion.qsmart.sdf_lower_lim = 0.5;
        c.inversion.qsmart.sdf_curv_constant = 600.0;
        c.inversion.qsmart.frangi_scale_min = 1.5;
        c.inversion.qsmart.frangi_scale_max = 7.0;
        c.inversion.qsmart.frangi_scale_ratio = 1.0;
        c.inversion.qsmart.frangi_c = 400.0;
        let cmd = generate_command(&c);
        for flag in [
            "--qsmart-inversion tv",
            "--qsmart-sdf-sigma1-stage1 11",
            "--qsmart-sdf-sigma2-stage1 12",
            "--qsmart-sdf-sigma1-stage2 13",
            "--qsmart-sdf-sigma2-stage2 14",
            "--qsmart-sdf-lower-lim 0.5",
            "--qsmart-sdf-curv-constant 600",
            "--qsmart-frangi-scale-min 1.5",
            "--qsmart-frangi-scale-max 7",
            "--qsmart-frangi-scale-ratio 1",
            "--qsmart-frangi-c 400",
        ] {
            assert!(cmd.contains(flag), "missing `{}` in: {}", flag, cmd);
        }
        assert!(!cmd.contains("--qsmart-frangi-c-"), "{cmd}");
        c.inversion.qsmart.frangi_c_mode = QsmartFrangiCMode::Relative;
        c.inversion.qsmart.frangi_c_factor = 0.3;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsmart-frangi-c-mode relative") && cmd.contains("--qsmart-frangi-c-factor 0.3"), "{cmd}");
    }

    /// Mirrors the WASM command path: the TOML that qsmbly's ConfigBridge emits must
    /// parse (matching serde key names) and produce the QSMART flags.
    #[test]
    fn test_qsmart_toml_from_configbridge_generates_flags() {
        let toml = r#"
[inversion]
algorithm = "qsmart"

[inversion.qsmart]
inversion = "rts"
ilsqr_max_iter = 50
ilsqr_pad_mm = 8.0
vasc_sphere_radius = 8
sdf_sigma1_stage1 = 11.0
sdf_sigma2_stage1 = 12.0
sdf_sigma1_stage2 = 13.0
sdf_sigma2_stage2 = 14.0
sdf_lower_lim = 0.45
sdf_curv_constant = 600.0
frangi_scale_min = 1.5
frangi_scale_max = 7.0
frangi_scale_ratio = 1.0
frangi_c = 400.0
"#;
        let config = PipelineConfig::from_toml(toml).expect("ConfigBridge TOML must parse");
        let cmd = generate_command(&config);
        for flag in [
            "--qsm-algorithm qsmart", "--qsmart-inversion rts",
            "--qsmart-sdf-sigma1-stage1 11", "--qsmart-sdf-lower-lim 0.45",
            "--qsmart-frangi-scale-min 1.5", "--qsmart-frangi-c 400",
        ] {
            assert!(cmd.contains(flag), "missing `{}` in: {}", flag, cmd);
        }
        // The inner iLSQR's settings do not apply under an RTS inner inversion.
        assert!(!cmd.contains("--qsmart-ilsqr"), "{cmd}");
        let config = PipelineConfig::from_toml(&toml.replace(r#"inversion = "rts""#, r#"inversion = "ilsqr""#)).unwrap();
        assert!(generate_command(&config).contains("--qsmart-ilsqr-pad-mm 8"));
    }

    #[test]
    fn test_r2primenet_settings_round_trip() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--r2primenet"));
        c.separation.r2primenet.ignore_field_mismatch = true;
        c.separation.r2primenet.patch = [128, 128, 64];
        // Not said while R2' is not computed: R2PRIMEnet does not run.
        assert!(!generate_command(&c).contains("--r2primenet"));
        c.pipeline.do_r2primemap = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--r2primenet-ignore-field-mismatch"), "{cmd}");
        assert!(cmd.contains("--r2primenet-patch 128 128 64"), "{cmd}");
        // Meaningless when R2' is only ever measured.
        c.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        assert!(!generate_command(&c).contains("--r2primenet"));
    }

    #[test]
    fn test_vsharp_radius_step_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--vsharp-radius-step"));
        c.bg_removal.vsharp.radius_step = 2.0;
        assert!(generate_command(&c).contains("--vsharp-radius-step 2"));
    }

    #[test]
    fn test_vsharp_grid_flags() {
        let mut c = PipelineConfig::default();
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-grid"), "the default grid is not stated: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Native;
        c.bg_removal.vsharp.grid_spacing = 0.8;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-grid native") && !cmd.contains("--vsharp-grid-spacing"), "unused unless isotropic: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-grid ") && cmd.contains("--vsharp-grid-spacing 0.8"), "{cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Finest;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-grid finest") && !cmd.contains("--vsharp-grid-spacing"), "{cmd}");
    }

    #[test]
    fn test_vsharp_deconvolve_flag() {
        let mut c = PipelineConfig::default();
        assert!(!c.bg_removal.vsharp.deconvolve, "QSMxT's default is off");
        c.bg_removal.vsharp.threshold = 0.1;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--vsharp-deconvolve"), "{cmd}");
        assert!(!cmd.contains("--vsharp-threshold"), "unused without deconvolution: {cmd}");
        c.bg_removal.vsharp.deconvolve = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--vsharp-deconvolve true"), "{cmd}");
        assert!(cmd.contains("--vsharp-threshold 0.1"), "{cmd}");
    }

    #[test]
    fn test_complex_fit_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::ComplexFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation complex-fit") && !cmd.contains("--complex-fit-"), "{cmd}");
        assert!(!cmd.contains("--linear-fit-"), "the linear fit's settings mean nothing to it: {cmd}");
        (c.field_mapping.complex_fit.reltol, c.field_mapping.complex_fit.max_iter) = (1e-3, 12);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--complex-fit-reltol 0.001") && cmd.contains("--complex-fit-max-iter 12"), "{cmd}");
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).contains("--complex-fit-"));
    }

    #[test]
    fn test_medi_csf_and_tfi_flags() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("csf") && !cmd.contains("zeropad"), "defaults are not emitted: {cmd}");
        (c.inversion.medi.lambda_csf, c.inversion.medi.csf_r2s_threshold, c.inversion.medi.csf_erode) = (50.0, 4.0, false);
        c.inversion.medi.zeropad = [2, 0, 2];
        let cmd = generate_command(&c);
        for f in ["--medi-lambda-csf 50", "--medi-csf-r2s-threshold 4", "--medi-csf-erode false", "--medi-zeropad 2 0 2"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        c.inversion.medi.csf = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--medi-csf false") && !cmd.contains("--medi-lambda-csf"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Tfi;
        (c.inversion.tfi.merit, c.inversion.tfi.linear) = (false, false);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--tfi-merit false") && cmd.contains("--tfi-linear false") && !cmd.contains("--medi-"), "{cmd}");
    }

    #[test]
    fn test_robust_fit_flags_round_trip() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::RobustFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation robust-fit"), "{cmd}");
        assert!(!cmd.contains("--robust-fit-"), "defaults are not emitted: {cmd}");
        c.field_mapping.robust_fit.loss = RobustLoss::Huber;
        c.field_mapping.robust_fit.tuning = Some(2.5);
        c.field_mapping.robust_fit.iterations = 5;
        c.field_mapping.robust_fit.downweight_level = 0.5;
        c.field_mapping.robust_fit.flag_ratio = 4.0;
        c.field_mapping.linear_fit.estimate_offset = false;
        let cmd = generate_command(&c);
        for f in ["--robust-fit-loss huber", "--robust-fit-tuning 2.5", "--robust-fit-iterations 5",
                  "--robust-fit-downweight-level 0.5", "--robust-fit-flag-ratio 4",
                  "--linear-fit-estimate-offset false"] {
            assert!(cmd.contains(f), "missing {f}: {cmd}");
        }
        // The robust settings mean nothing to the weighted average.
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).contains("--robust-fit-"));
    }

    #[test]
    fn test_motion_correction_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("motion"));
        c.motion_correction.enabled = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--motion-correction"), "{cmd}");
        assert!(!cmd.contains("--motion-reference") && !cmd.contains("--registration-"), "defaults: {cmd}");
        c.motion_correction.reference = crate::motion::MotionReference::Echo(2);
        c.motion_correction.chain = false;
        c.motion_correction.min_ncc = 0.9;
        c.registration.levels = 4;
        c.registration.min_overlap = 0.5;
        let cmd = generate_command(&c);
        for f in ["--motion-reference 2", "--no-motion-chain", "--motion-min-ncc 0.9",
                  "--registration-levels 4", "--registration-min-overlap 0.5"] {
            assert!(cmd.contains(f), "missing {f}: {cmd}");
        }
    }

    #[test]
    fn test_b0_estimation_linear_fit() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::LinearFit;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-estimation linear-fit"));
    }

    #[test]
    fn test_b0_weight_type() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_weight_type = B0WeightType::Average;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-weight-type average"));
    }

    #[test]
    fn test_star_settings() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Star;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm star"), "{cmd}");
        assert!(!cmd.contains("--star-pad") && !cmd.contains("--star-te"), "defaults are not emitted: {cmd}");
        c.inversion.star.pad = [4, 4, 12];
        c.inversion.star.te = EchoTimeChoice::First;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--star-pad 4 4 12") && cmd.contains("--star-te first"), "{cmd}");
        c.inversion.star.te = EchoTimeChoice::Seconds(0.007);
        assert!(generate_command(&c).contains("--star-te 7"), "an echo time is given in ms");
    }

    #[test]
    fn star_te_in_config_files() {
        let c: StarConfig = toml::from_str("te = \"first\"").unwrap();
        assert_eq!(c.te, EchoTimeChoice::First);
        let c: StarConfig = toml::from_str("te = { seconds = 0.03 }").unwrap();
        assert_eq!(c.te, EchoTimeChoice::Seconds(0.03));
        assert!(toml::from_str::<StarConfig>("te = \"median\"").is_err());
        let mut d = PipelineConfig::default();
        d.inversion.star.te = EchoTimeChoice::Seconds(0.025);
        let back: PipelineConfig = toml::from_str(&toml::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.inversion.star.te, EchoTimeChoice::Seconds(0.025));
    }

    #[test]
    fn echo_time_choice_parses_names_and_milliseconds() {
        for c in EchoTimeChoice::NAMED {
            assert_eq!(EchoTimeChoice::parse(&c.to_string()), Ok(c));
        }
        assert_eq!(EchoTimeChoice::parse("20"), Ok(EchoTimeChoice::Seconds(0.02)));
        assert!(EchoTimeChoice::parse("0").is_err() && EchoTimeChoice::parse("median").is_err());
    }

    #[test]
    fn test_laplacian_solver() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--laplacian-solver"));
        c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
        // Only Laplacian unwrapping has a solver.
        assert!(!generate_command(&c).contains("--laplacian-solver"));
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--laplacian-solver fft"), "{cmd}");
        assert!(!cmd.contains("--laplacian-fft-pad"), "default pad is not emitted: {cmd}");
        c.field_mapping.laplacian_fft_pad = [12; 3];
        assert!(generate_command(&c).contains("--laplacian-fft-pad 12"));
        c.field_mapping.laplacian_fft_pad = [64, 64, 32];
        assert!(generate_command(&c).contains("--laplacian-fft-pad 64 64 32"));
        c.field_mapping.laplacian_solver = LaplacianSolver::Mgpcg;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--laplacian-solver mgpcg") && !cmd.contains("--laplacian-fft-pad"), "{cmd}");
    }

    /// MEDI's region growing is selected by `--unwrapping-algorithm medi-region-growing`, and a
    /// config file keeps it.
    #[test]
    fn test_medi_region_growing_command() {
        let mut c = PipelineConfig::default();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::MediRegionGrowing;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--unwrapping-algorithm medi-region-growing") && !cmd.contains("--romeo"), "{cmd}");
        let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(back.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::MediRegionGrowing);
        let f: FieldMappingConfig = toml::from_str("unwrapping_algorithm = \"medi-region-growing\"\n").unwrap();
        assert_eq!(f.unwrapping_algorithm, UnwrappingAlgorithm::MediRegionGrowing);
    }

    /// PRELUDE is selected by `--unwrapping-algorithm prelude`; its settings are said only when
    /// they are not the defaults and PRELUDE runs, and a config file keeps them.
    #[test]
    fn test_prelude_settings() {
        let mut c = PipelineConfig::default();
        c.field_mapping.prelude.labelling = PreludeLabelling::Volume;
        c.field_mapping.prelude.num_phase_partitions = 6;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--prelude"), "ROMEO has no PRELUDE settings: {cmd}");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
        let cmd = generate_command(&c);
        for said in ["--unwrapping-algorithm prelude", "--prelude-labelling volume", "--prelude-partitions 6"] {
            assert!(cmd.contains(said), "{said:?} not in {cmd}");
        }
        assert!(!cmd.contains("--romeo"), "{cmd}");
        let mut d = PipelineConfig::default();
        d.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
        let cmd = generate_command(&d);
        assert!(cmd.contains("--unwrapping-algorithm prelude") && !cmd.contains("--prelude-"), "defaults are not said: {cmd}");
        let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(back.field_mapping.unwrapping_algorithm, UnwrappingAlgorithm::Prelude);
        assert_eq!(back.field_mapping.prelude, c.field_mapping.prelude);
        let f: FieldMappingConfig = toml::from_str("unwrapping_algorithm = \"prelude\"\n[prelude]\nlabelling = \"slices\"\n").unwrap();
        assert_eq!((f.unwrapping_algorithm, f.prelude.labelling, f.prelude.num_phase_partitions),
                   (UnwrappingAlgorithm::Prelude, PreludeLabelling::Slices, 8));
        assert!(toml::from_str::<FieldMappingConfig>("[prelude]\nlabelling = \"planes\"\n").is_err());
    }

    /// The unmasked Laplacian unwrapping is said only when it is on and Laplacian unwrapping
    /// runs, and a config file keeps it.
    #[test]
    fn test_laplacian_unmasked() {
        let mut c = PipelineConfig::default();
        c.field_mapping.laplacian_unmasked = true;
        assert!(!generate_command(&c).contains("--laplacian-unmasked"), "ROMEO has no Laplacian unwrapping");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        assert!(generate_command(&c).contains("--laplacian-unmasked"));
        c.field_mapping.laplacian_unmasked = false;
        assert!(!generate_command(&c).contains("--laplacian-unmasked"), "off is the default");
        let f: FieldMappingConfig = toml::from_str("laplacian_unmasked = true").unwrap();
        assert!(f.laplacian_unmasked);
        assert!(!toml::from_str::<FieldMappingConfig>("").unwrap().laplacian_unmasked);
    }

    #[test]
    fn test_laplacian_fft_pad_in_config_files() {
        let c: FieldMappingConfig = toml::from_str("laplacian_fft_pad = 12").unwrap();
        assert_eq!(c.laplacian_fft_pad, [12; 3]);
        let c: FieldMappingConfig = toml::from_str("laplacian_fft_pad = [64, 64, 32]").unwrap();
        assert_eq!(c.laplacian_fft_pad, [64, 64, 32]);
        assert!(toml::from_str::<FieldMappingConfig>("laplacian_fft_pad = [1, 2]").is_err());
        // What a config writes, it reads back.
        let mut d = PipelineConfig::default();
        d.field_mapping.laplacian_fft_pad = [64, 64, 32];
        let back = PipelineConfig::from_toml(&toml::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.field_mapping.laplacian_fft_pad, [64, 64, 32]);
    }

    #[test]
    fn test_laplacian_solver_old_config_names() {
        // `laplacian_kernel = "sti"` was the (unreleased) earlier spelling.
        let c: FieldMappingConfig = toml::from_str("laplacian_kernel = \"sti\"").unwrap();
        assert_eq!(c.laplacian_solver, LaplacianSolver::Fft);
        assert_eq!(c.laplacian_fft_pad, [64; 3]);
    }

    #[test]
    fn test_b0_weight_type_assumed_decay() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--b0-weight-type assumed-decay"), "{cmd}");
        assert!(!cmd.contains("--b0-weight-assumed-t2star"), "default T2* is not emitted: {cmd}");
        c.field_mapping.b0_weight_assumed_t2star_ms = 35.0;
        assert!(generate_command(&c).contains("--b0-weight-assumed-t2star 35"));
        // only meaningful with assumed-decay weighting
        c.field_mapping.b0_weight_type = B0WeightType::TEs;
        assert!(!generate_command(&c).contains("--b0-weight-assumed-t2star"));
        // The config spelling round-trips, and the never-released "t2star" is not accepted.
        assert_eq!(serde_json::from_str::<B0WeightType>("\"assumed-decay\"").unwrap(), B0WeightType::AssumedDecay);
        assert_eq!(B0WeightType::AssumedDecay.to_string(), "assumed-decay");
        assert!(serde_json::from_str::<B0WeightType>("\"t2star\"").is_err());
    }

    #[test]
    fn test_laplacian_unwrapping() {
        let mut c = PipelineConfig::default();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--unwrapping-algorithm laplacian"));
    }

    #[test]
    fn test_romeo_correct_global_enabled() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.correct_global = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--romeo-correct-global"));
    }

    #[test]
    fn test_romeo_weight_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.phase_gradient_coherence = false;
        c.field_mapping.romeo.mag_coherence = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-romeo-phase-gradient-coherence"));
        assert!(cmd.contains("--no-romeo-mag-coherence"));
    }

    #[test]
    fn test_mask_non_default() {
        let mut c = PipelineConfig::default();
        c.masking.sections[0].refinements = vec![
            crate::masking::MaskOp::Erode { iterations: 3 },
        ];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--mask"));
        assert!(cmd.contains("erode:3"));
    }

    #[test]
    fn segmentation_and_analysis_emit_their_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--do-segmentation"));

        c.pipeline.do_segmentation = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-segmentation"), "{cmd}");
        assert!(!cmd.contains("--synthseg-version"), "the default version stays off: {cmd}");

        // --do-analysis implies segmentation, so emitting both would be noise.
        c.pipeline.do_analysis = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-analysis"), "{cmd}");
        assert!(!cmd.contains("--do-segmentation"), "implied, so not repeated: {cmd}");

        c.segmentation.version = crate::enums::SynthSegVersion::V2;
        c.segmentation.flip_averaging = false;
        c.segmentation.crop = Some(192);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--synthseg-version v2"), "{cmd}");
        assert!(cmd.contains("--no-synthseg-flip-averaging"), "{cmd}");
        assert!(cmd.contains("--synthseg-crop 192"), "{cmd}");
    }

    #[test]
    fn smwi_emits_its_flag_and_non_default_params() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--do-smwi"));

        c.pipeline.do_smwi = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--do-smwi"), "{cmd}");
        assert!(!cmd.contains("--smwi-threshold"), "defaults stay off: {cmd}");

        c.smwi.threshold_ppm = 0.2;
        c.smwi.mip_window = 7;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--smwi-threshold 0.2"), "{cmd}");
        assert!(cmd.contains("--smwi-mip-window 7"), "{cmd}");
        assert!(!cmd.contains("--smwi-power"), "an untouched parameter stays off: {cmd}");
    }

    /// HEIDI is `--heidi` after any inversion, with its own group; LSQR+HEIDI emits LSQR's group
    /// too, and plain LSQR does not emit HEIDI's.
    #[test]
    fn heidi_emits_its_flag_after_any_inversion() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Lsqr;
        c.inversion.lsqr.tol = 1e-7;
        c.inversion.heidi.cone_threshold = 0.2;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm lsqr"), "{cmd}");
        assert!(cmd.contains("--lsqr-tol 0.0000001"), "{cmd}");
        assert!(!cmd.contains("--heidi"), "plain LSQR should not emit HEIDI: {cmd}");

        c.inversion.heidi.enabled = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm lsqr --lsqr-tol 0.0000001 --heidi"), "{cmd}");
        assert!(cmd.contains("--heidi-cone-threshold 0.2"), "{cmd}");

        c.inversion.algorithm = QsmAlgorithm::Rts;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-algorithm rts"), "{cmd}");
        assert!(cmd.contains("--heidi --heidi-cone-threshold 0.2"), "{cmd}");
        assert!(!cmd.contains("--lsqr-tol"), "RTS does not read LSQR's: {cmd}");
    }

    /// Defaults stay off the command line, and the two `no-` flags only appear when the switch is
    /// actually turned off.
    #[test]
    fn lsqr_and_heidi_defaults_are_silent() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Lsqr;
        c.inversion.heidi.enabled = true;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--lsqr-tol"), "{cmd}");
        assert!(!cmd.contains("--no-lsqr-global-offset"), "{cmd}");
        assert!(!cmd.contains("--no-heidi-denoise"), "{cmd}");
        assert!(!cmd.contains("--heidi-denoise-iterations"), "defaults stay off: {cmd}");

        c.inversion.lsqr.fit_global_offset = false;
        c.inversion.heidi.denoise = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-lsqr-global-offset"), "{cmd}");
        assert!(cmd.contains("--no-heidi-denoise"), "{cmd}");
        // With denoising off, its three parameters are noise on the command line.
        assert!(!cmd.contains("--heidi-denoise-iterations"), "{cmd}");
    }

    /// Off by default, so the bare flag says everything when the reliable mask is the default one.
    #[test]
    fn two_pass_emits_a_bare_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--two-pass"));

        c.masking.two_pass = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--two-pass"), "{cmd}");
        assert!(!cmd.contains("--two-pass-mask"), "default recipe should not be spelled out: {cmd}");

        // Explicitly setting the default recipe means the same thing as leaving it unset.
        c.masking.two_pass_sections = Some(crate::masking::default_two_pass_sections());
        assert!(!generate_command(&c).contains("--two-pass-mask"));
    }

    #[test]
    fn two_pass_names_the_original_mode() {
        let mut c = PipelineConfig::default();
        c.masking.two_pass = true;
        assert!(!generate_command(&c).contains("--two-pass-mode"));
        c.masking.two_pass_mode = crate::enums::TwoPassMode::Original;
        assert!(generate_command(&c).contains("--two-pass-mode original"), "{}", generate_command(&c));
    }

    #[test]
    fn two_pass_spells_out_an_edited_reliable_mask() {
        let mut c = PipelineConfig::default();
        c.masking.two_pass = true;
        c.masking.two_pass_sections = Some(vec![crate::masking::MaskSection {
            input: crate::masking::MaskingInput::Magnitude,
            generator: crate::masking::MaskOp::Threshold {
                method: crate::masking::MaskThresholdMethod::Percentile,
                value: Some(30.0),
            },
            refinements: vec![],
        }]);
        let cmd = generate_command(&c);
        assert!(cmd.contains("--two-pass"), "{cmd}");
        assert!(cmd.contains("--two-pass-mask magnitude,threshold:percentile:30.0"), "{cmd}");
        assert!(!cmd.contains("--two-pass-mask-combine"), "unset, it is the main mask's: {cmd}");
        // Its own combine mode, once it has sections to combine.
        c.masking.two_pass_combine = Some(crate::masking::MaskCombine::And);
        assert!(!generate_command(&c).contains("--two-pass-mask-combine"));
        c.masking.two_pass_sections.as_mut().unwrap().extend(crate::masking::default_two_pass_sections());
        assert!(generate_command(&c).contains("--two-pass-mask-combine and"));
    }

    #[test]
    fn test_pipeline_toggles() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_swi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-qsm"));
        assert!(cmd.contains("--do-swi"));
        assert!(cmd.contains("--do-t2starmap"));
        assert!(cmd.contains("--do-r2starmap"));
    }

    #[test]
    fn n4_and_its_settings_are_said_when_they_apply() {
        let mut c = PipelineConfig::default();
        c.n4.shrink_factor = 2;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--inhomogeneity-method") && !cmd.contains("--n4-"), "makehomogeneous hides N4's settings: {cmd}");
        c.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        c.homogeneity.sigma_mm = 5.0;
        c.n4.max_iterations = vec![20, 10];
        c.n4.mesh = [2, 2, 1];
        c.n4.wiener_noise = 0.02;
        c.masking.n4_geometry = crate::enums::N4Geometry::MakeNii;
        let cmd = generate_command(&c);
        for f in ["--inhomogeneity-method n4", "--n4-shrink-factor 2", "--n4-max-iterations 20 10",
                  "--n4-mesh 2 2 1", "--n4-wiener-noise 0.02", "--n4-geometry make-nii"] {
            assert!(cmd.contains(f), "{f} missing: {cmd}");
        }
        assert!(!cmd.contains("--homogeneity-sigma-mm") && !cmd.contains("--n4-spline-order"), "{cmd}");
    }

    #[test]
    fn test_inhomogeneity_disabled() {
        let mut c = PipelineConfig::default();
        c.masking.inhomogeneity_correction = false;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--no-inhomogeneity-correction"));
    }

    #[test]
    fn a_per_echo_qsm_is_in_the_command() {
        let mut config = PipelineConfig::default();
        assert!(!generate_command(&config).contains("--qsm-per-echo"));
        config.qsm.per_echo = true;
        assert!(generate_command(&config).contains("--qsm-per-echo true"));
    }

    #[test]
    fn test_qsm_reference_none() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::None;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-reference none"));

        // A region reference travels as the spec itself, not as the word "region".
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("left-thalamus,right-thalamus".to_string());
        let cmd = generate_command(&c);
        assert!(cmd.contains("--qsm-reference left-thalamus,right-thalamus"), "{cmd}");
        assert!(!cmd.contains("--qsm-reference region"), "{cmd}");
    }

    #[test]
    fn test_json_roundtrip() {
        let config = PipelineConfig::default();
        let json = config.to_json().unwrap();
        let parsed: PipelineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.inversion.algorithm, QsmAlgorithm::Ilsqr);
    }

    #[test]
    fn test_partial_toml() {
        // Only specify one field — rest should use defaults
        let toml = r#"
[inversion]
algorithm = "tv"
"#;
        let config = PipelineConfig::from_toml(toml).unwrap();
        assert_eq!(config.inversion.algorithm, QsmAlgorithm::Tv);
        // Everything else should be default
        assert!(config.pipeline.do_qsm);
        assert_eq!(config.bg_removal.algorithm, BfAlgorithm::Vsharp);
        assert!(config.field_mapping.phase_offset_removal);
    }

    #[test]
    fn test_empty_toml() {
        let config = PipelineConfig::from_toml("").unwrap();
        assert_eq!(config.inversion.algorithm, QsmAlgorithm::Ilsqr);
        assert!(config.pipeline.do_qsm);
    }

    #[test]
    fn test_output_space() {
        let mut c = PipelineConfig::default();
        assert_eq!(c.pipeline.output_space, OutputSpace::Acquired, "acquired is the default");
        assert!(!generate_command(&c).contains("--output-space"));
        c.pipeline.output_space = OutputSpace::Working;
        // Only meaningful when the data is resampled.
        assert!(!generate_command(&c).contains("--output-space"));
        c.pipeline.axial_resampling = true;
        assert!(generate_command(&c).contains("--output-space working"));
    }

    #[test]
    fn test_crop_options() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--crop"));
        c.pipeline.crop_to_mask = true;
        c.pipeline.crop_margin_mm = 24.0;
        let cmd = generate_command(&c);
        assert!(cmd.contains("--crop-to-mask"), "{cmd}");
        assert!(cmd.contains("--crop-margin-mm 24"), "{cmd}");
    }

    #[test]
    fn test_coil_combination_sigma() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("--coil-combination-sigma"));
        c.field_mapping.coil_combination_sigma = [4.0, 4.0, 4.0];
        assert!(generate_command(&c).contains("--coil-combination-sigma 4 4 4"));
    }

    #[test]
    fn test_phase_offset_sigma() {
        let mut c = PipelineConfig::default();
        c.field_mapping.phase_offset_sigma_mm = [10.0, 10.0, 5.0];
        let cmd = generate_command(&c);
        assert!(cmd.contains("--phase-offset-sigma-mm 10 10 5"));
    }
}

/// Quote a pattern for a shell when it contains glob metacharacters.
///
/// An orientation-group pattern is almost always a glob (`*acq-dir*`), and an unquoted glob
/// in the displayed command is one the user's shell would expand against their working
/// directory before qsmxt ever saw it.
fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod multiorient_command_tests {
    use super::*;

    #[test]
    fn nothing_is_emitted_when_grouping_is_off() {
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.contains("--orientation-group"), "{cmd}");
        assert!(!cmd.contains("--multi-orientation"), "{cmd}");
    }

    #[test]
    fn a_glob_pattern_is_quoted_so_the_shell_leaves_it_alone() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "*acq-dir*".into();
        let cmd = generate_command(&config);
        assert!(cmd.contains("--orientation-group '*acq-dir*'"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-algorithm cosmos"), "{cmd}");
        // Lambda is at its default, so it should not add noise.
        assert!(!cmd.contains("--multi-orientation-lambda"), "{cmd}");
    }

    #[test]
    fn a_bare_entity_needs_no_quoting() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
        config.multi_orientation.lambda = 1e-3;
        let cmd = generate_command(&config);
        assert!(cmd.contains("--orientation-group acq"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-algorithm sti"), "{cmd}");
        assert!(cmd.contains("--multi-orientation-lambda 0.001"), "{cmd}");
    }

    #[test]
    fn the_orientation_reference_is_rendered_only_when_set() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        assert!(!generate_command(&config).contains("--orientation-reference"));
        config.multi_orientation.reference = "*acq-Neutral*".into();
        let cmd = generate_command(&config);
        assert!(cmd.contains("--orientation-reference '*acq-Neutral*'"), "{cmd}");
    }
    /// χ-sepnet's toolbox route implies R2*; an R2′ map the user asked for is a flag of its own.
    #[test]
    fn the_toolbox_route_implies_only_r2star() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::SeparationAlgorithm::ChiSepNet;
        let cmd = generate_command(&c);
        assert!(!cmd.contains("--do-r2primemap") && !cmd.contains("--do-r2map") && !cmd.contains("--do-r2starmap"), "{cmd}");
        c.pipeline.do_r2primemap = true;
        assert!(generate_command(&c).contains("--do-r2primemap"));
    }
}
