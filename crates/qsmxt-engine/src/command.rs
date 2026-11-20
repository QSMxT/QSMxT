//! CLI command generation from PipelineConfig.

use serde_json::Value;

use crate::config::*;
use crate::enums::*;
use crate::schema::cli::{Flag, FlagAction, PIPELINE_FLAGS};
use crate::schema::cli_apply::{opposite_of, setting_of};
use crate::schema::{Setting, ValueType};

/// Generate a `qsmxt run` CLI command from a pipeline configuration, as a shell would take it
/// ([`to_shell`] of [`command_args`]).
pub fn generate_command(config: &PipelineConfig) -> String {
    to_shell(&command_args(config))
}

/// The words of a `qsmxt run` command for a pipeline configuration, unquoted: `qsmxt`, `run`,
/// `<bids_dir>`, then the flags and their values. Compares against defaults and only emits flags
/// that differ.
///
/// Settings that do not apply to the configuration (the TUI hides them; see
/// [`crate::schema::settings_view`]) are left out: they cannot change the run, so saying them
/// only makes the command longer and suggests they matter.
///
/// A config that follows a toolbox pipeline ([`crate::pipelines::followed`]) starts with
/// `--pipeline <id>`, and its other flags are where it departs from that pipeline.
pub fn command_args(config: &PipelineConfig) -> Vec<String> {
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



/// The command for `config`, with flags where it differs from `base`: QSMxT's defaults, or
/// `pipeline`'s settings. Most flags set one registry setting ([`setting_of`]) and are said
/// when that setting is shown and differs from `base`; the rest are spelled out below.
fn command_for(config: &PipelineConfig, base: &PipelineConfig, pipeline: Option<&str>) -> Vec<String> {
    let mut parts: Vec<String> = vec!["qsmxt".into(), "run".into(), "<bids_dir>".into()];
    if let Some(id) = pipeline {
        say(&mut parts, &["--pipeline", id]);
    }
    let visible: Vec<&str> = crate::schema::settings_view(config, &crate::schema::ViewFacts::default())
        .into_iter().filter(|v| v.visible).map(|v| v.path).collect();
    // A config is plain data with string keys: serialising it cannot fail.
    let json = |c: &PipelineConfig| serde_json::to_value(c).expect("a config serialises");
    let (values, base_values) = (json(config), json(base));
    let m = &config.multi_orientation;
    // In the table's order: the registry's (`schema::cli`), each opposite switch said at its
    // switch's place.
    for flag in PIPELINE_FLAGS.iter() {
        let long = &flag.long;
        match *long {
            // Said first, above.
            "pipeline" => {}
            // The pattern is what switches grouping on, so it and the reconstruction are said
            // whenever it is on; the patterns are quoted so that a shell leaves their globs alone.
            "orientation-group" if m.enabled() => say(&mut parts, &["--orientation-group", &m.group_by]),
            "orientation-reference" if m.enabled() && !m.reference.trim().is_empty() => {
                say(&mut parts, &["--orientation-reference", m.reference.trim()]);
            }
            "multi-orientation-algorithm" if m.enabled() => say(&mut parts, &["--multi-orientation-algorithm", &m.algorithm.to_string()]),
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
                    say(&mut parts, &["--romeo-template", &(t + 1).to_string()]);
                }
            }
            // One value when the three are the same.
            "laplacian-fft-pad" => {
                let pad = config.field_mapping.laplacian_fft_pad;
                if config.field_mapping.laplacian_solver == LaplacianSolver::Fft && pad != base.field_mapping.laplacian_fft_pad {
                    let pad: Vec<String> = if pad[0] == pad[1] && pad[1] == pad[2] { vec![pad[0].to_string()] } else { pad.map(|p| p.to_string()).to_vec() };
                    parts.push("--laplacian-fft-pad".into());
                    parts.extend(pad);
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
                            say(&mut parts, &["--two-pass-mask", &section.compact_spec()]);
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
                    say(&mut parts, &["--qsm-reference", &spec]);
                }
            }
            "dismiss-recommendation" => dismissal_flags(&mut parts, config),
            _ => {
                let Some(setting) = setting_of(flag) else { continue };
                let (v, b) = (lookup(&values, setting.path), lookup(&base_values, setting.path));
                if v != b && visible.contains(&setting.path) {
                    if let Some(words) = render(flag, setting, &v) {
                        parts.extend(words);
                    }
                }
            }
        }
    }
    parts
}

/// Add `words` to a command's.
fn say(parts: &mut Vec<String>, words: &[&str]) {
    parts.extend(words.iter().map(|w| w.to_string()));
}

/// A command's words as a shell takes them: each word a shell would split or expand (a glob, a
/// space, a quote) is quoted, so the command runs as it reads. A placeholder (`<bids_dir>`) is
/// left as it is written, for the user to fill in.
pub fn to_shell(words: &[String]) -> String {
    words.iter().map(|w| {
        let placeholder = w.starts_with('<') && w.ends_with('>');
        if placeholder || (!w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_.,/=:+@%".contains(c))) {
            w.clone()
        } else {
            format!("'{}'", w.replace('\'', r"'\''"))
        }
    }).collect::<Vec<_>>().join(" ")
}

/// The value at `path`, `null` for an unset optional value (left out of the serialised config).
fn lookup(config: &Value, path: &str) -> Value {
    path.split('.').try_fold(config, |v, k| v.get(k)).cloned().unwrap_or(Value::Null)
}

/// `--flag value` for a setting `qsmxt` keeps outside the registry's flags.
fn emit_value(parts: &mut Vec<String>, flag: &Flag, value: &f64, base: &f64) {
    if value != base {
        say(parts, &[&format!("--{}", flag.long), &value.to_string()]);
    }
}

/// How `flag` says `value` of `setting`, or `None` where it cannot: an unset optional value is
/// the one qsm-core derives, said by leaving the flag out, and a switch is said only by a flag
/// for its direction.
fn render(flag: &Flag, setting: &Setting, value: &Value) -> Option<Vec<String>> {
    let long = flag.long;
    let text = match (value, &setting.value_type) {
        (Value::Null, _) => return None,
        (Value::Bool(on), ValueType::Bool) if flag.action == FlagAction::SetTrue => {
            // `--x` turns it on and `--no-x` off; the other direction is its opposite's.
            if *on != long.starts_with("no-") {
                return Some(vec![format!("--{long}")]);
            }
            return PIPELINE_FLAGS.iter().find(|f| opposite_of(f).is_some_and(|(o, _)| o.long == long))
                .map(|f| vec![format!("--{}", f.long)]);
        }
        (v, ValueType::EchoTime { .. }) => vec![serde_json::from_value::<EchoTimeChoice>(v.clone()).ok()?.to_string()],
        (v, ValueType::NumberOrChoice { .. }) => vec![serde_json::from_value::<ChiBoundChoice>(v.clone()).ok()?.to_string()],
        (v, ValueType::EchoReference { .. }) => vec![serde_json::from_value::<crate::MotionReference>(v.clone()).ok()?.to_string()],
        // A bare flag stands for the value it takes when given without one (`*`, any tool).
        (Value::String(s), _) if flag.default_missing.contains(&s.as_str()) => return Some(vec![format!("--{long}")]),
        (Value::Array(xs), _) => xs.iter().map(scalar).collect(),
        (v, _) => vec![scalar(v)],
    };
    Some(std::iter::once(format!("--{long}")).chain(text).collect())
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
            say(parts, &["--mask-preset", preset]);
            if let Some(input) = input {
                say(parts, &["--masking-input", &input.to_string()]);
            }
        }
        None => {
            for section in &c.sections {
                say(parts, &["--mask", &section.compact_spec()]);
            }
            if c.combine != b.combine {
                say(parts, &["--mask-combine", &c.combine.to_string()]);
            }
            for op in &c.refinements {
                say(parts, &["--mask-refine", &op.compact_spec()]);
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
        say(parts, &["--dismiss-recommendation", &format!("{}={}", x.rule, x.chosen)]);
    }
}

#[cfg(test)]
mod every_flag_tests;

#[cfg(test)]
mod tests {
    /// Whether a generated command says `words`: the same whole tokens, in a row. A plain
    /// substring test would take `--medi-smv` to be in `--medi-smv-radius 5`, or
    /// `--qsm-algorithm tv` to be in `--qsm-algorithm tvx`.
    pub(super) trait Says {
        fn says(&self, words: &str) -> bool;
    }

    impl Says for str {
        fn says(&self, words: &str) -> bool {
            let have: Vec<&str> = self.split_whitespace().collect();
            let want: Vec<&str> = words.split_whitespace().collect();
            !want.is_empty() && have.windows(want.len()).any(|w| w == want.as_slice())
        }
    }

    /// A word a shell would split or expand is quoted; a placeholder is left for the user.
    #[test]
    fn to_shell_quotes_what_a_shell_would_change() {
        let words: Vec<String> = ["qsmxt", "run", "<bids_dir>", "/data/my bids", "--include", "sub-1*", "--x", "it's"]
            .map(String::from).to_vec();
        assert_eq!(to_shell(&words), r"qsmxt run <bids_dir> '/data/my bids' --include 'sub-1*' --x 'it'\''s'");
    }

    #[test]
    fn says_matches_whole_tokens() {
        let cmd = "qsmxt run <bids_dir> --medi-smv-radius 5 --qsm-algorithm tvx";
        assert!(cmd.says("--medi-smv-radius 5"));
        assert!(!cmd.says("--medi-smv"));
        assert!(!cmd.says("--qsm-algorithm tv"));
        assert!(!cmd.says(""));
    }
    /// A mask supplied for the run (qsmbly writes it to derivatives/qsmbly/, `crate::supplied`)
    /// is read back with `--use-custom-masks qsmbly`.
    #[test]
    fn a_supplied_mask_is_read_from_its_tools_derivatives() {
        let mut config = crate::config::PipelineConfig::default();
        assert!(!super::generate_command(&config).says("--use-custom-masks"));
        config.masking.custom_mask_tool = Some("qsmbly".into());
        assert!(super::generate_command(&config).says("--use-custom-masks qsmbly"));
        config.masking.custom_mask_tool = Some("*".into());
        let command = super::generate_command(&config);
        assert!(command.says("--use-custom-masks") && !command.says("--use-custom-masks *"), "{command}");
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
        assert!(cmd.says("--do-r2map"), "R2 is not implied by r2star-qsm: {cmd}");
        assert!(!cmd.says("--do-r2starmap"), "R2* is: {cmd}");
        // R2'-based separation implies R2' and with it R2* and R2: none spelled out.
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        enforce_separation_dependencies(&mut c);
        let cmd = generate_command(&c);
        for f in ["--do-r2primemap", "--do-r2map", "--do-r2starmap"] {
            assert!(!cmd.says(f), "{f} is implied: {cmd}");
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
        assert!(cmd.says("--bet-voxel-scale 10"), "cmd: {cmd}");
        assert!(cmd.says("--mask magnitude,bet:0.50:scale=10"), "cmd: {cmd}");

        // The default is silent — the clinical command line is unchanged.
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.says("--bet-voxel-scale"), "cmd: {cmd}");
        assert!(!cmd.contains("scale="), "cmd: {cmd}");
    }

    #[test]
    fn test_changed_algorithm() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Tv;
        let cmd = generate_command(&config);
        assert!(cmd.says("--qsm-algorithm tv"));
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
            assert!(cmd.says(&format!("--qsm-algorithm {}", a)), "missing {} in: {}", a, cmd);
        }
        // iQFM background removal + χ-sepnet separation (deep-learning; patches in the profile tests).
        let mut c = PipelineConfig::default();
        c.bg_removal.algorithm = BfAlgorithm::Iqfm;
        assert!(generate_command(&c).says("--bf-algorithm iqfm"));
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        assert!(generate_command(&c).says("--chisep chi-sepnet"));
    }

    #[test]
    fn test_dl_tiling_command() {
        use crate::enums::QsmAlgorithm;
        // Off by default: no tiling flags for a DL algorithm.
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--tile-size"), "got: {}", cmd);
        assert!(!cmd.says("--tile-halo"), "got: {}", cmd);
        // Setting tile_size enables tiling; halo emitted only when set.
        c.inversion.tile_size = Some(64);
        let cmd = generate_command(&c);
        assert!(cmd.says("--tile-size 64"), "got: {}", cmd);
        assert!(!cmd.says("--tile-halo"), "got: {}", cmd);
        c.inversion.tile_halo = Some(4);
        let cmd = generate_command(&c);
        assert!(cmd.says("--tile-size 64") && cmd.says("--tile-halo 4"), "got: {}", cmd);
        // Native-patch nets ignore tiling options (no flags emitted).
        c.inversion.algorithm = QsmAlgorithm::Autoqsm;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--tile-size"), "autoqsm should not emit tiling: {}", cmd);
        // A classical inversion after a tiled background-removal network keeps them.
        c.inversion.algorithm = QsmAlgorithm::Tkd;
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Bfrnet;
        assert!(generate_command(&c).says("--tile-size 64"), "BFRnet tiles");
        // The phase-input reconstructions tile too.
        c.bg_removal.algorithm = crate::enums::BfAlgorithm::Vsharp;
        c.inversion.algorithm = QsmAlgorithm::Iqsm;
        assert!(generate_command(&c).says("--tile-size 64"), "iQSM tiles");
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
        assert!(cmd.says("--medi-msmv") && cmd.says("--medi-msmv-maxk 9"), "got: {cmd}");
        assert!(!cmd.says("--medi-msmv-vessel-radius") && !cmd.contains("prefilter"), "got: {cmd}");
        config.inversion.medi.msmv_vessel_radius = Some(12.0);
        config.inversion.medi.msmv_prefilter = false;
        let cmd = generate_command(&config);
        assert!(cmd.says("--medi-msmv-vessel-radius 12") && cmd.says("--no-medi-msmv-prefilter"), "got: {cmd}");
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
        assert!(!generate_command(&config).says("--bf-algorithm"), "{}", generate_command(&config));
        config.bg_removal.algorithm = BfAlgorithm::Pdf;
        assert!(generate_command(&config).says("--bf-algorithm pdf"));
        config.inversion.algorithm = QsmAlgorithm::Rts;
        assert!(generate_command(&config).says("--bf-algorithm pdf"));
    }

    #[test]
    fn medi_smv_shrink_mask_command() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Medi;
        assert!(!generate_command(&config).contains("shrink"));
        config.inversion.medi.smv_shrink_mask = false;
        assert!(generate_command(&config).says("--no-medi-smv-shrink-mask"));
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
        assert!(cmd.says("--do-chisep"));
        assert!(!cmd.says("--do-r2starmap"), "r2starmap implied by chisep, got: {}", cmd);
        assert!(!cmd.says("--do-r2map"));
        assert!(!cmd.says("--do-r2primemap"));
        assert!(!cmd.says("--chisep")); // default method → not emitted

        // An R2'-based method (wavesep) forces R2' → R2 → R2*, none of which should leak.
        let mut wc = PipelineConfig::default();
        wc.pipeline.do_chi_separation = true;
        wc.separation.algorithm = SeparationAlgorithm::WaveSep;
        enforce_separation_dependencies(&mut wc);
        assert!(wc.pipeline.do_r2primemap && wc.pipeline.do_r2map && wc.pipeline.do_r2starmap);
        let cmd = generate_command(&wc);
        assert!(cmd.says("--chisep wavesep"));
        assert!(!cmd.says("--do-r2map") && !cmd.says("--do-r2primemap") && !cmd.says("--do-r2starmap"));

        // Selecting a non-default method emits it; editing a param emits that too.
        config.separation.algorithm = SeparationAlgorithm::Decompose;
        config.separation.decompose.lsqcurvefit.max_iterations = 99;
        config.separation.decompose.unwrap_phase = true;
        let cmd = generate_command(&config);
        assert!(cmd.says("--chisep decompose"));
        assert!(cmd.says("--decompose-max-iterations 99") && cmd.says("--decompose-unwrap-phase true"), "{cmd}");
        assert!(!cmd.says("--decompose-chi-bound"), "{cmd}");
        config.separation.decompose.chi_bound = crate::ChiBoundChoice::EchoSpacing;
        assert!(generate_command(&config).says("--decompose-chi-bound echo-spacing"));
        config.separation.decompose.chi_bound = crate::ChiBoundChoice::Ppm(0.3);
        assert!(generate_command(&config).says("--decompose-chi-bound 0.3"));

        // A param for a non-selected method must NOT leak into the command.
        assert!(!cmd.says("--wavesep-alpha"));
    }

    #[test]
    fn test_changed_rts_param() {
        let mut config = PipelineConfig::default();
        config.inversion.algorithm = QsmAlgorithm::Rts;
        config.inversion.rts.delta = 0.2;
        let cmd = generate_command(&config);
        assert!(cmd.says("--rts-delta 0.2"));
    }

    #[test]
    fn test_phase_offset_disabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.phase_offset_removal = false;
        let cmd = generate_command(&config);
        assert!(cmd.says("--phase-offset-removal false"));
    }

    #[test]
    fn test_bipolar_enabled() {
        let mut config = PipelineConfig::default();
        config.field_mapping.bipolar_correction = true;
        let cmd = generate_command(&config);
        assert!(cmd.says("--bipolar-correction"));
    }

    #[test]
    fn test_romeo_individual_mode() {
        let mut config = PipelineConfig::default();
        // Template mode is the default: neither the mode nor the template echo is stated.
        assert!(!generate_command(&config).contains("romeo-individual"));
        assert!(!generate_command(&config).says("--romeo-template"));
        config.field_mapping.romeo.individual = true;
        let cmd = generate_command(&config);
        assert!(cmd.says("--romeo-individual") && !cmd.says("--romeo-template"), "{cmd}");
    }

    #[test]
    fn test_swi_params_only_when_enabled() {
        let mut config = PipelineConfig::default();
        config.swi.strength = 8.0;
        // SWI off — no SWI flags
        let cmd = generate_command(&config);
        assert!(!cmd.says("--swi-strength"));
        // SWI on — flag appears
        config.pipeline.do_swi = true;
        let cmd = generate_command(&config);
        assert!(cmd.says("--swi-strength 8"));
        assert!(!cmd.says("--swi-unwrapping"), "{cmd}");
        config.swi.unwrap = crate::SwiUnwrap::Romeo;
        assert!(generate_command(&config).says("--swi-unwrapping romeo"));
        assert!(!generate_command(&config).says("--swi-filtered-phase"));
        config.swi.filtered_phase = true;
        assert!(generate_command(&config).says("--swi-filtered-phase"));
        // Without SWI there is no filtered phase to write.
        config.pipeline.do_swi = false;
        assert!(!generate_command(&config).says("--swi-filtered-phase"));
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
            assert!(cmd.says(&format!("--qsm-algorithm {}", name)), "missing algorithm flag for {}", name);
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
            assert!(cmd.says(&format!("--bf-algorithm {}", name)), "missing bf flag for {}", name);
        }
    }

    #[test]
    fn test_tv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tv;
        c.inversion.tv.lambda = 0.002;
        c.inversion.tv.max_iter = 100;
        let cmd = generate_command(&c);
        assert!(cmd.says("--tv-lambda 0.002"));
        assert!(cmd.says("--tv-max-iter 100"));
        // ρ unset follows λ (qsm-core's 100·λ): no flag. Set, it is said.
        assert!(!cmd.says("--tv-rho"), "{cmd}");
        c.inversion.tv.rho = Some(0.5);
        assert!(generate_command(&c).says("--tv-rho 0.5"));
    }

    #[test]
    fn test_medi_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Medi;
        c.inversion.medi.lambda = 999.0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--medi-lambda 999"));
    }

    #[test]
    fn test_tgv_params() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Tgv;
        c.inversion.tgv.iterations = Some(500);
        c.inversion.tgv.erosions = 2;
        let cmd = generate_command(&c);
        assert!(cmd.says("--tgv-iterations 500"));
        assert!(cmd.says("--tgv-erosions 2"));
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
            assert!(cmd.says(&format!("--{flag}-te 7")), "{alg}: {cmd}");
        }
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        c.inversion.whqsm.weight = WhqsmWeight::InVivo;
        assert!(generate_command(&c).says("--whqsm-weight in-vivo"));
        c.inversion.algorithm = QsmAlgorithm::Nltv;
        c.inversion.nltv.rho = Some(0.5);
        c.inversion.nltv.tol_newton = 1e-4;
        let cmd = generate_command(&c);
        assert!(cmd.says("--nltv-rho 0.5") && cmd.says("--nltv-tol-newton 0.0001"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.tau = 0.5;
        c.inversion.rts.gamma = 2.0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--rts-tau 0.5") && cmd.says("--rts-gamma 2"), "{cmd}");
        c.bg_removal.algorithm = BfAlgorithm::Pdf;
        c.bg_removal.pdf.n_cg = 300;
        c.bg_removal.pdf.space = PdfSpace::Kspace;
        c.bg_removal.pdf.n_pad = 0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--pdf-n-cg 300") && cmd.says("--pdf-space kspace") && cmd.says("--pdf-n-pad 0"), "{cmd}");
        for (alg, flag) in [(BfAlgorithm::Harperella, "--harperella-pad"), (BfAlgorithm::Iharperella, "--iharperella-pad")] {
            let mut c = PipelineConfig::default();
            c.bg_removal.algorithm = alg;
            assert!(!generate_command(&c).says(flag));
            c.bg_removal.harperella.pad = [2, 4, 6];
            c.bg_removal.iharperella.pad = [2, 4, 6];
            assert!(generate_command(&c).says(&format!("{flag} 2 4 6")));
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
            assert!(generate_command(&c).says(&format!("{flag} {rim}")), "{}", generate_command(&c));
        }
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--bf-erode-voxels"));
        c.bg_removal.erode_voxels = 3;
        assert!(generate_command(&c).says("--bf-erode-voxels 3"), "{}", generate_command(&c));
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Ndi;
        assert!(!generate_command(&c).says("--ndi-weight-norm"));
        c.inversion.ndi.weight_norm = NdiWeightNorm::MaskP999;
        assert!(generate_command(&c).says("--ndi-weight-norm mask-p999"));
        c.inversion.algorithm = QsmAlgorithm::Whqsm;
        c.inversion.whqsm.weight = WhqsmWeight::FlatInMask;
        assert!(generate_command(&c).says("--whqsm-weight flat-in-mask"));
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
        assert!(cmd.says("--qsmart-vasc-sphere-radius 10"));
    }

    /// Under QSMART the default mask goes unsaid, as elsewhere: it is not replaced.
    #[test]
    fn qsmart_keeps_the_default_mask() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        assert!(!generate_command(&c).says("--mask"), "{}", generate_command(&c));
        c.masking.sections = crate::masking::mask_presets().into_iter().find(|(n, _)| *n == "bet").unwrap().1.sections;
        assert!(generate_command(&c).says("--mask-preset bet"), "{}", generate_command(&c));
    }

    /// Under QSMART the command says the field mapping it was given, as elsewhere; QSMART's vessel
    /// options are said when changed.
    #[test]
    fn qsmart_field_mapping_and_vessel_options_are_in_the_command() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--unwrapping-algorithm") && !cmd.says("--b0-estimation"), "{cmd}");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.laplacian_solver = LaplacianSolver::Mirror;
        c.field_mapping.b0_estimation = B0Estimation::QsmartEchofit;
        let cmd = generate_command(&c);
        for flag in ["--unwrapping-algorithm laplacian", "--laplacian-solver mirror", "--b0-estimation qsmart-echofit"] {
            assert!(cmd.says(flag), "{cmd}");
        }
        assert!(!cmd.says("--qsmart-fit-threshold") && !cmd.says("--qsmart-vessel"), "{cmd}");
        assert!(!cmd.says("--laplacian-unmasked"), "{cmd}");
        c.field_mapping.laplacian_unmasked = true;
        assert!(generate_command(&c).says("--laplacian-unmasked"));
        c.inversion.qsmart.vessel_magnitude = QsmartVesselMagnitude::Combined;
        c.inversion.qsmart.vessel_size_units = VesselSizeUnits::Mm;
        let cmd = generate_command(&c);
        for flag in ["--qsmart-vessel-magnitude combined", "--qsmart-vessel-units mm"] {
            assert!(cmd.says(flag), "{cmd}");
        }
        // The default fixed threshold (QSMART v1.0's 40): nothing; the threshold when changed.
        c.field_mapping.qsmart_fit_threshold = 30.0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsmart-fit-threshold 30") && !cmd.says("--qsmart-fit-threshold-mode")
                && !cmd.says("--qsmart-fit-percentile"), "{cmd}");
        // A percentile: the mode, and the percentile when changed; not the threshold.
        c.field_mapping.qsmart_fit_threshold_mode = QsmartFitThresholdMode::Percentile;
        c.field_mapping.qsmart_fit_percentile = 95.0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsmart-fit-threshold-mode percentile") && cmd.says("--qsmart-fit-percentile 95")
                && !cmd.says("--qsmart-fit-threshold "), "{cmd}");
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
            assert!(cmd.says(flag), "missing `{}` in: {}", flag, cmd);
        }
        assert!(!cmd.says("--qsmart-frangi-c-"), "{cmd}");
        c.inversion.qsmart.frangi_c_mode = QsmartFrangiCMode::Relative;
        c.inversion.qsmart.frangi_c_factor = 0.3;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsmart-frangi-c-mode relative") && cmd.says("--qsmart-frangi-c-factor 0.3"), "{cmd}");
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
            assert!(cmd.says(flag), "missing `{}` in: {}", flag, cmd);
        }
        // The inner iLSQR's settings do not apply under an RTS inner inversion.
        assert!(!cmd.says("--qsmart-ilsqr"), "{cmd}");
        let config = PipelineConfig::from_toml(&toml.replace(r#"inversion = "rts""#, r#"inversion = "ilsqr""#)).unwrap();
        assert!(generate_command(&config).says("--qsmart-ilsqr-pad-mm 8"));
    }

    #[test]
    fn test_r2primenet_settings_round_trip() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--r2primenet"));
        c.separation.r2primenet.ignore_field_mismatch = true;
        c.separation.r2primenet.patch = [128, 128, 64];
        // Not said while R2' is not computed: R2PRIMEnet does not run.
        assert!(!generate_command(&c).says("--r2primenet"));
        c.pipeline.do_r2primemap = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--r2primenet-ignore-field-mismatch"), "{cmd}");
        assert!(cmd.says("--r2primenet-patch 128 128 64"), "{cmd}");
        // Meaningless when R2' is only ever measured.
        c.separation.r2prime_strategy = R2PrimeStrategy::Mese;
        assert!(!generate_command(&c).says("--r2primenet"));
    }

    #[test]
    fn test_vsharp_radius_step_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--vsharp-radius-step"));
        c.bg_removal.vsharp.radius_step = 2.0;
        assert!(generate_command(&c).says("--vsharp-radius-step 2"));
    }

    #[test]
    fn test_vsharp_grid_flags() {
        let mut c = PipelineConfig::default();
        let cmd = generate_command(&c);
        assert!(!cmd.says("--vsharp-grid"), "the default grid is not stated: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Native;
        c.bg_removal.vsharp.grid_spacing = 0.8;
        let cmd = generate_command(&c);
        assert!(cmd.says("--vsharp-grid native") && !cmd.says("--vsharp-grid-spacing"), "unused unless isotropic: {cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Isotropic;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--vsharp-grid ") && cmd.says("--vsharp-grid-spacing 0.8"), "{cmd}");
        c.bg_removal.vsharp.grid = VsharpGrid::Finest;
        let cmd = generate_command(&c);
        assert!(cmd.says("--vsharp-grid finest") && !cmd.says("--vsharp-grid-spacing"), "{cmd}");
    }

    #[test]
    fn test_vsharp_deconvolve_flag() {
        let mut c = PipelineConfig::default();
        assert!(!c.bg_removal.vsharp.deconvolve, "QSMxT's default is off");
        c.bg_removal.vsharp.threshold = 0.1;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--vsharp-deconvolve"), "{cmd}");
        assert!(!cmd.says("--vsharp-threshold"), "unused without deconvolution: {cmd}");
        c.bg_removal.vsharp.deconvolve = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--vsharp-deconvolve true"), "{cmd}");
        assert!(cmd.says("--vsharp-threshold 0.1"), "{cmd}");
    }

    #[test]
    fn test_complex_fit_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::ComplexFit;
        let cmd = generate_command(&c);
        assert!(cmd.says("--b0-estimation complex-fit") && !cmd.says("--complex-fit-"), "{cmd}");
        assert!(!cmd.says("--linear-fit-"), "the linear fit's settings mean nothing to it: {cmd}");
        (c.field_mapping.complex_fit.reltol, c.field_mapping.complex_fit.max_iter) = (1e-3, 12);
        let cmd = generate_command(&c);
        assert!(cmd.says("--complex-fit-reltol 0.001") && cmd.says("--complex-fit-max-iter 12"), "{cmd}");
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).says("--complex-fit-"));
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
            assert!(cmd.says(f), "{f} missing: {cmd}");
        }
        c.inversion.medi.csf = false;
        let cmd = generate_command(&c);
        assert!(cmd.says("--medi-csf false") && !cmd.says("--medi-lambda-csf"), "{cmd}");
        c.inversion.algorithm = QsmAlgorithm::Tfi;
        (c.inversion.tfi.merit, c.inversion.tfi.linear) = (false, false);
        let cmd = generate_command(&c);
        assert!(cmd.says("--tfi-merit false") && cmd.says("--tfi-linear false") && !cmd.says("--medi-"), "{cmd}");
    }

    #[test]
    fn test_robust_fit_flags_round_trip() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::RobustFit;
        let cmd = generate_command(&c);
        assert!(cmd.says("--b0-estimation robust-fit"), "{cmd}");
        assert!(!cmd.says("--robust-fit-"), "defaults are not emitted: {cmd}");
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
            assert!(cmd.says(f), "missing {f}: {cmd}");
        }
        // The robust settings mean nothing to the weighted average.
        c.field_mapping.b0_estimation = B0Estimation::WeightedAvg;
        assert!(!generate_command(&c).says("--robust-fit-"));
    }

    #[test]
    fn test_motion_correction_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).contains("motion"));
        c.motion_correction.enabled = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--motion-correction"), "{cmd}");
        assert!(!cmd.says("--motion-reference") && !cmd.says("--registration-"), "defaults: {cmd}");
        c.motion_correction.reference = crate::motion::MotionReference::Echo(2);
        c.motion_correction.chain = false;
        c.motion_correction.min_ncc = 0.9;
        c.registration.levels = 4;
        c.registration.min_overlap = 0.5;
        let cmd = generate_command(&c);
        for f in ["--motion-reference 2", "--no-motion-chain", "--motion-min-ncc 0.9",
                  "--registration-levels 4", "--registration-min-overlap 0.5"] {
            assert!(cmd.says(f), "missing {f}: {cmd}");
        }
    }

    #[test]
    fn test_b0_estimation_linear_fit() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_estimation = B0Estimation::LinearFit;
        let cmd = generate_command(&c);
        assert!(cmd.says("--b0-estimation linear-fit"));
    }

    #[test]
    fn test_b0_weight_type() {
        let mut c = PipelineConfig::default();
        c.field_mapping.b0_weight_type = B0WeightType::Average;
        let cmd = generate_command(&c);
        assert!(cmd.says("--b0-weight-type average"));
    }

    #[test]
    fn test_star_settings() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Star;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsm-algorithm star"), "{cmd}");
        assert!(!cmd.says("--star-pad") && !cmd.says("--star-te"), "defaults are not emitted: {cmd}");
        c.inversion.star.pad = [4, 4, 12];
        c.inversion.star.te = EchoTimeChoice::First;
        let cmd = generate_command(&c);
        assert!(cmd.says("--star-pad 4 4 12") && cmd.says("--star-te first"), "{cmd}");
        c.inversion.star.te = EchoTimeChoice::Seconds(0.007);
        assert!(generate_command(&c).says("--star-te 7"), "an echo time is given in ms");
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
        assert!(!generate_command(&c).says("--laplacian-solver"));
        c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
        // Only Laplacian unwrapping has a solver.
        assert!(!generate_command(&c).says("--laplacian-solver"));
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        let cmd = generate_command(&c);
        assert!(cmd.says("--laplacian-solver fft"), "{cmd}");
        assert!(!cmd.says("--laplacian-fft-pad"), "default pad is not emitted: {cmd}");
        c.field_mapping.laplacian_fft_pad = [12; 3];
        assert!(generate_command(&c).says("--laplacian-fft-pad 12"));
        c.field_mapping.laplacian_fft_pad = [64, 64, 32];
        assert!(generate_command(&c).says("--laplacian-fft-pad 64 64 32"));
        c.field_mapping.laplacian_solver = LaplacianSolver::Mgpcg;
        let cmd = generate_command(&c);
        assert!(cmd.says("--laplacian-solver mgpcg") && !cmd.says("--laplacian-fft-pad"), "{cmd}");
    }

    /// MEDI's region growing is selected by `--unwrapping-algorithm medi-region-growing`, and a
    /// config file keeps it.
    #[test]
    fn test_medi_region_growing_command() {
        let mut c = PipelineConfig::default();
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::MediRegionGrowing;
        let cmd = generate_command(&c);
        assert!(cmd.says("--unwrapping-algorithm medi-region-growing") && !cmd.says("--romeo"), "{cmd}");
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
        assert!(!cmd.says("--prelude"), "ROMEO has no PRELUDE settings: {cmd}");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
        let cmd = generate_command(&c);
        for said in ["--unwrapping-algorithm prelude", "--prelude-labelling volume", "--prelude-partitions 6"] {
            assert!(cmd.says(said), "{said:?} not in {cmd}");
        }
        assert!(!cmd.says("--romeo"), "{cmd}");
        let mut d = PipelineConfig::default();
        d.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Prelude;
        let cmd = generate_command(&d);
        assert!(cmd.says("--unwrapping-algorithm prelude") && !cmd.says("--prelude-"), "defaults are not said: {cmd}");
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
        assert!(!generate_command(&c).says("--laplacian-unmasked"), "ROMEO has no Laplacian unwrapping");
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        assert!(generate_command(&c).says("--laplacian-unmasked"));
        c.field_mapping.laplacian_unmasked = false;
        assert!(!generate_command(&c).says("--laplacian-unmasked"), "off is the default");
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
        assert!(cmd.says("--b0-weight-type assumed-decay"), "{cmd}");
        assert!(!cmd.says("--b0-weight-assumed-t2star"), "default T2* is not emitted: {cmd}");
        c.field_mapping.b0_weight_assumed_t2star_ms = 35.0;
        assert!(generate_command(&c).says("--b0-weight-assumed-t2star 35"));
        // only meaningful with assumed-decay weighting
        c.field_mapping.b0_weight_type = B0WeightType::TEs;
        assert!(!generate_command(&c).says("--b0-weight-assumed-t2star"));
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
        assert!(cmd.says("--unwrapping-algorithm laplacian"));
    }

    #[test]
    fn test_romeo_correct_global_enabled() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.correct_global = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--romeo-correct-global"));
    }

    #[test]
    fn test_romeo_weight_flags() {
        let mut c = PipelineConfig::default();
        c.field_mapping.romeo.phase_gradient_coherence = false;
        c.field_mapping.romeo.mag_coherence = false;
        let cmd = generate_command(&c);
        assert!(cmd.says("--no-romeo-phase-gradient-coherence"));
        assert!(cmd.says("--no-romeo-mag-coherence"));
    }

    #[test]
    fn test_mask_non_default() {
        let mut c = PipelineConfig::default();
        c.masking.sections[0].refinements = vec![
            crate::masking::MaskOp::Erode { iterations: 3 },
        ];
        let cmd = generate_command(&c);
        assert!(cmd.says("--mask"));
        assert!(cmd.contains("erode:3"));
    }

    #[test]
    fn segmentation_and_analysis_emit_their_flags() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--do-segmentation"));

        c.pipeline.do_segmentation = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--do-segmentation"), "{cmd}");
        assert!(!cmd.says("--synthseg-version"), "the default version stays off: {cmd}");

        // --do-analysis implies segmentation, so emitting both would be noise.
        c.pipeline.do_analysis = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--do-analysis"), "{cmd}");
        assert!(!cmd.says("--do-segmentation"), "implied, so not repeated: {cmd}");

        c.segmentation.version = crate::enums::SynthSegVersion::V2;
        c.segmentation.flip_averaging = false;
        c.segmentation.crop = Some(192);
        let cmd = generate_command(&c);
        assert!(cmd.says("--synthseg-version v2"), "{cmd}");
        assert!(cmd.says("--no-synthseg-flip-averaging"), "{cmd}");
        assert!(cmd.says("--synthseg-crop 192"), "{cmd}");
    }

    #[test]
    fn smwi_emits_its_flag_and_non_default_params() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--do-smwi"));

        c.pipeline.do_smwi = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--do-smwi"), "{cmd}");
        assert!(!cmd.says("--smwi-threshold"), "defaults stay off: {cmd}");

        c.smwi.threshold_ppm = 0.2;
        c.smwi.mip_window = 7;
        let cmd = generate_command(&c);
        assert!(cmd.says("--smwi-threshold 0.2"), "{cmd}");
        assert!(cmd.says("--smwi-mip-window 7"), "{cmd}");
        assert!(!cmd.says("--smwi-power"), "an untouched parameter stays off: {cmd}");
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
        assert!(cmd.says("--qsm-algorithm lsqr"), "{cmd}");
        assert!(cmd.says("--lsqr-tol 0.0000001"), "{cmd}");
        assert!(!cmd.says("--heidi"), "plain LSQR should not emit HEIDI: {cmd}");

        c.inversion.heidi.enabled = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsm-algorithm lsqr --lsqr-tol 0.0000001 --heidi"), "{cmd}");
        assert!(cmd.says("--heidi-cone-threshold 0.2"), "{cmd}");

        c.inversion.algorithm = QsmAlgorithm::Rts;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsm-algorithm rts"), "{cmd}");
        assert!(cmd.says("--heidi --heidi-cone-threshold 0.2"), "{cmd}");
        assert!(!cmd.says("--lsqr-tol"), "RTS does not read LSQR's: {cmd}");
    }

    /// Defaults stay off the command line, and the two `no-` flags only appear when the switch is
    /// actually turned off.
    #[test]
    fn lsqr_and_heidi_defaults_are_silent() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Lsqr;
        c.inversion.heidi.enabled = true;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--lsqr-tol"), "{cmd}");
        assert!(!cmd.says("--no-lsqr-global-offset"), "{cmd}");
        assert!(!cmd.says("--no-heidi-denoise"), "{cmd}");
        assert!(!cmd.says("--heidi-denoise-iterations"), "defaults stay off: {cmd}");

        c.inversion.lsqr.fit_global_offset = false;
        c.inversion.heidi.denoise = false;
        let cmd = generate_command(&c);
        assert!(cmd.says("--no-lsqr-global-offset"), "{cmd}");
        assert!(cmd.says("--no-heidi-denoise"), "{cmd}");
        // With denoising off, its three parameters are noise on the command line.
        assert!(!cmd.says("--heidi-denoise-iterations"), "{cmd}");
    }

    /// Off by default, so the bare flag says everything when the reliable mask is the default one.
    #[test]
    fn two_pass_emits_a_bare_flag() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--two-pass"));

        c.masking.two_pass = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--two-pass"), "{cmd}");
        assert!(!cmd.says("--two-pass-mask"), "default recipe should not be spelled out: {cmd}");

        // Explicitly setting the default recipe means the same thing as leaving it unset.
        c.masking.two_pass_sections = Some(crate::masking::default_two_pass_sections());
        assert!(!generate_command(&c).says("--two-pass-mask"));
    }

    #[test]
    fn two_pass_names_the_original_mode() {
        let mut c = PipelineConfig::default();
        c.masking.two_pass = true;
        assert!(!generate_command(&c).says("--two-pass-mode"));
        c.masking.two_pass_mode = crate::enums::TwoPassMode::Original;
        assert!(generate_command(&c).says("--two-pass-mode original"), "{}", generate_command(&c));
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
        assert!(cmd.says("--two-pass"), "{cmd}");
        assert!(cmd.says("--two-pass-mask magnitude,threshold:percentile:30.0"), "{cmd}");
        assert!(!cmd.says("--two-pass-mask-combine"), "unset, it is the main mask's: {cmd}");
        // Its own combine mode, once it has sections to combine.
        c.masking.two_pass_combine = Some(crate::masking::MaskCombine::And);
        assert!(!generate_command(&c).says("--two-pass-mask-combine"));
        c.masking.two_pass_sections.as_mut().unwrap().extend(crate::masking::default_two_pass_sections());
        assert!(generate_command(&c).says("--two-pass-mask-combine and"));
    }

    #[test]
    fn test_pipeline_toggles() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_swi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        let cmd = generate_command(&c);
        assert!(cmd.says("--no-qsm"));
        assert!(cmd.says("--do-swi"));
        assert!(cmd.says("--do-t2starmap"));
        assert!(cmd.says("--do-r2starmap"));
    }

    #[test]
    fn n4_and_its_settings_are_said_when_they_apply() {
        let mut c = PipelineConfig::default();
        c.n4.shrink_factor = 2;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--inhomogeneity-method") && !cmd.says("--n4-"), "makehomogeneous hides N4's settings: {cmd}");
        c.masking.inhomogeneity_method = InhomogeneityMethod::N4;
        c.homogeneity.sigma_mm = 5.0;
        c.n4.max_iterations = vec![20, 10];
        c.n4.mesh = [2, 2, 1];
        c.n4.wiener_noise = 0.02;
        c.masking.n4_geometry = crate::enums::N4Geometry::MakeNii;
        let cmd = generate_command(&c);
        for f in ["--inhomogeneity-method n4", "--n4-shrink-factor 2", "--n4-max-iterations 20 10",
                  "--n4-mesh 2 2 1", "--n4-wiener-noise 0.02", "--n4-geometry make-nii"] {
            assert!(cmd.says(f), "{f} missing: {cmd}");
        }
        assert!(!cmd.says("--homogeneity-sigma-mm") && !cmd.says("--n4-spline-order"), "{cmd}");
    }

    #[test]
    fn test_inhomogeneity_disabled() {
        let mut c = PipelineConfig::default();
        c.masking.inhomogeneity_correction = false;
        let cmd = generate_command(&c);
        assert!(cmd.says("--no-inhomogeneity-correction"));
    }

    #[test]
    fn a_per_echo_qsm_is_in_the_command() {
        let mut config = PipelineConfig::default();
        assert!(!generate_command(&config).says("--qsm-per-echo"));
        config.qsm.per_echo = true;
        assert!(generate_command(&config).says("--qsm-per-echo true"));
    }

    #[test]
    fn test_qsm_reference_none() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::None;
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsm-reference none"));

        // A region reference travels as the spec itself, not as the word "region".
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("left-thalamus,right-thalamus".to_string());
        let cmd = generate_command(&c);
        assert!(cmd.says("--qsm-reference left-thalamus,right-thalamus"), "{cmd}");
        assert!(!cmd.says("--qsm-reference region"), "{cmd}");
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
        assert!(!generate_command(&c).says("--output-space"));
        c.pipeline.output_space = OutputSpace::Working;
        // Only meaningful when the data is resampled.
        assert!(!generate_command(&c).says("--output-space"));
        c.pipeline.axial_resampling = true;
        assert!(generate_command(&c).says("--output-space working"));
    }

    #[test]
    fn test_crop_options() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--crop"));
        c.pipeline.crop_to_mask = true;
        c.pipeline.crop_margin_mm = 24.0;
        let cmd = generate_command(&c);
        assert!(cmd.says("--crop-to-mask"), "{cmd}");
        assert!(cmd.says("--crop-margin-mm 24"), "{cmd}");
    }

    #[test]
    fn test_coil_combination_sigma() {
        let mut c = PipelineConfig::default();
        assert!(!generate_command(&c).says("--coil-combination-sigma"));
        c.field_mapping.coil_combination_sigma = [4.0, 4.0, 4.0];
        assert!(generate_command(&c).says("--coil-combination-sigma 4 4 4"));
    }

    #[test]
    fn test_phase_offset_sigma() {
        let mut c = PipelineConfig::default();
        c.field_mapping.phase_offset_sigma_mm = [10.0, 10.0, 5.0];
        let cmd = generate_command(&c);
        assert!(cmd.says("--phase-offset-sigma-mm 10 10 5"));
    }
}

#[cfg(test)]
mod multiorient_command_tests {
    use super::*;
    use super::tests::Says;

    #[test]
    fn nothing_is_emitted_when_grouping_is_off() {
        let cmd = generate_command(&PipelineConfig::default());
        assert!(!cmd.says("--orientation-group"), "{cmd}");
        assert!(!cmd.says("--multi-orientation"), "{cmd}");
    }

    #[test]
    fn a_glob_pattern_is_quoted_so_the_shell_leaves_it_alone() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "*acq-dir*".into();
        let cmd = generate_command(&config);
        assert!(cmd.says("--orientation-group '*acq-dir*'"), "{cmd}");
        assert!(cmd.says("--multi-orientation-algorithm cosmos"), "{cmd}");
        // Lambda is at its default, so it should not add noise.
        assert!(!cmd.says("--multi-orientation-lambda"), "{cmd}");
    }

    #[test]
    fn a_bare_entity_needs_no_quoting() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
        config.multi_orientation.lambda = 1e-3;
        let cmd = generate_command(&config);
        assert!(cmd.says("--orientation-group acq"), "{cmd}");
        assert!(cmd.says("--multi-orientation-algorithm sti"), "{cmd}");
        assert!(cmd.says("--multi-orientation-lambda 0.001"), "{cmd}");
    }

    #[test]
    fn the_orientation_reference_is_rendered_only_when_set() {
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        assert!(!generate_command(&config).says("--orientation-reference"));
        config.multi_orientation.reference = "*acq-Neutral*".into();
        let cmd = generate_command(&config);
        assert!(cmd.says("--orientation-reference '*acq-Neutral*'"), "{cmd}");
    }
    /// χ-sepnet's toolbox route implies R2*; an R2′ map the user asked for is a flag of its own.
    #[test]
    fn the_toolbox_route_implies_only_r2star() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::SeparationAlgorithm::ChiSepNet;
        let cmd = generate_command(&c);
        assert!(!cmd.says("--do-r2primemap") && !cmd.says("--do-r2map") && !cmd.says("--do-r2starmap"), "{cmd}");
        c.pipeline.do_r2primemap = true;
        assert!(generate_command(&c).says("--do-r2primemap"));
    }
}
