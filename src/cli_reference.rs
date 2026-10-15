//! The generated command-line reference (`docs/src/content/docs/reference/cli.md`), and the
//! checks that keep the hand-written docs from drifting from the binary (#282).
//!
//! The page is rendered from clap's own model of `qsmxt` — every subcommand, argument, possible
//! value and default — so it cannot list a flag the binary does not have or miss one it does.
//! `cli_reference_is_current` fails when the committed page differs from what the binary
//! renders; regenerate it with
//!
//! ```sh
//! QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference
//! ```

use clap::{Arg, ArgAction, Command, CommandFactory};

/// The page, relative to the repository root.
pub const CLI_REFERENCE: &str = "docs/src/content/docs/reference/cli.md";

/// Render the whole reference page.
pub fn render() -> String {
    let mut cmd = crate::cli::Cli::command();
    cmd.build();
    let mut out = String::from(
        "---\n\
         title: CLI reference\n\
         description: Every qsmxt command, option, accepted value and default, generated from the binary.\n\
         ---\n\n\
         <!-- Generated from qsmxt's own argument definitions by src/cli_reference.rs. Do not edit:\n\
         \x20    run `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference` to regenerate. -->\n\n\
         This page is generated from the `qsmxt` binary's argument definitions, so it lists exactly\n\
         the commands and options the binary accepts. `qsmxt <command> --help` prints the same\n\
         text. For what the options *do*, see [Commands](/QSMxT/reference/commands/),\n\
         [Algorithms](/QSMxT/reference/algorithms/) and [Standalone tools](/QSMxT/reference/tools/).\n\n",
    );
    out.push_str("## Commands\n\n");
    for sub in visible_subcommands(&cmd) {
        out.push_str(&format!("- [`qsmxt {}`](#{})\n", sub.get_name(), anchor(&format!("qsmxt {}", sub.get_name()))));
    }
    out.push('\n');
    for sub in visible_subcommands(&cmd) {
        render_command(&mut out, sub, "qsmxt", 2);
    }
    // One trailing newline, whatever the last section left.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn visible_subcommands(cmd: &Command) -> Vec<&Command> {
    cmd.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help").collect()
}

/// The heading anchor Starlight gives `text` (lower case, spaces to dashes, punctuation dropped).
fn anchor(text: &str) -> String {
    text.to_lowercase().chars().filter_map(|c| match c {
        'a'..='z' | '0'..='9' | '-' | '_' => Some(c),
        ' ' => Some('-'),
        _ => None,
    }).collect()
}

/// Text that is safe inside a Markdown/MDX paragraph or table cell.
fn escape(text: &str) -> String {
    // Inside `code` only the table's `|` needs escaping; outside it, `<`/`>` would be read as
    // HTML and `*` as emphasis.
    text.split('`').enumerate().map(|(k, part)| {
        let part = part.replace('|', "\\|");
        if k % 2 == 1 { part } else { part.replace('<', "&lt;").replace('>', "&gt;").replace('*', "\\*") }
    }).collect::<Vec<_>>().join("`")
}

/// `text` ended as a sentence, so what follows it reads as a new one.
fn sentence(text: &str) -> String {
    let t = text.trim_end();
    if t.is_empty() || t.ends_with(['.', '!', '?', ')']) && !t.ends_with("e.g.)") { t.to_string() } else { format!("{t}.") }
}

fn render_command(out: &mut String, cmd: &Command, parent: &str, level: usize) {
    let path = format!("{parent} {}", cmd.get_name());
    out.push_str(&format!("{} `{path}`\n\n", "#".repeat(level.min(6))));
    if let Some(about) = cmd.get_long_about().or_else(|| cmd.get_about()) {
        out.push_str(&escape(about.to_string().trim()));
        out.push_str("\n\n");
    }
    let subs = visible_subcommands(cmd);
    if !subs.is_empty() {
        out.push_str("| Subcommand | Description |\n| --- | --- |\n");
        for s in &subs {
            let about = s.get_about().map(|a| first_paragraph(&a.to_string())).unwrap_or_default();
            out.push_str(&format!("| [`{}`](#{}) | {} |\n", s.get_name(),
                anchor(&format!("{path} {}", s.get_name())), escape(&about)));
        }
        out.push('\n');
        for s in subs {
            render_command(out, s, &path, level + 1);
        }
        return;
    }
    let mut usage = cmd.clone().bin_name(path.clone());
    let usage = usage.render_usage().to_string();
    let usage = usage.trim_start_matches("Usage:").trim();
    out.push_str(&format!("```sh\n{usage}\n```\n\n"));

    let args: Vec<&Arg> = cmd.get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    if args.is_empty() {
        return;
    }
    out.push_str("| Argument | Description |\n| --- | --- |\n");
    for a in args {
        out.push_str(&format!("| {} | {} |\n", arg_name(a), arg_description(a)));
    }
    out.push('\n');
}

fn first_paragraph(text: &str) -> String {
    text.split("\n\n").next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn arg_name(a: &Arg) -> String {
    let value = a.get_value_names().map(|v| v.iter().map(|n| format!("<{n}>")).collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| format!("<{}>", a.get_id().as_str().to_uppercase()));
    let takes_value = !matches!(a.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count);
    let mut names = Vec::new();
    if let Some(s) = a.get_short() {
        names.push(format!("-{s}"));
    }
    if let Some(l) = a.get_long() {
        names.push(format!("--{l}"));
    }
    let name = if names.is_empty() {
        // Positional.
        value.clone()
    } else if takes_value {
        format!("{} {value}", names.join(", "))
    } else {
        names.join(", ")
    };
    format!("`{}`", name.replace('|', "\\|"))
}

fn arg_description(a: &Arg) -> String {
    let help = a.get_long_help().or_else(|| a.get_help()).map(|h| h.to_string()).unwrap_or_default();
    let mut parts = vec![sentence(&escape(&help.split_whitespace().collect::<Vec<_>>().join(" ")))];
    let values: Vec<String> = a.get_possible_values().iter().filter(|v| !v.is_hide_set())
        .map(|v| format!("`{}`", v.get_name())).collect();
    let takes_value = !matches!(a.get_action(), ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count);
    if takes_value && !values.is_empty() && !is_bool(a) {
        parts.push(format!("Values: {}.", values.join(", ")));
    }
    let defaults: Vec<String> = a.get_default_values().iter().map(|d| d.to_string_lossy().into_owned()).collect();
    if takes_value && !defaults.is_empty() {
        parts.push(format!("Default: `{}`.", defaults.join(" ")));
    }
    if a.is_required_set() {
        parts.push("Required.".to_string());
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" ")
}

/// A `true`/`false` value, which lists its possible values to no one's benefit.
fn is_bool(a: &Arg) -> bool {
    let v: Vec<String> = a.get_possible_values().iter().map(|v| v.get_name().to_string()).collect();
    v == ["true", "false"]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn doc(rel: &str) -> String {
        std::fs::read_to_string(repo().join("docs/src/content/docs").join(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// The committed reference is what the binary renders. Regenerate with
    /// `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference`.
    #[test]
    fn cli_reference_is_current() {
        let path = repo().join(CLI_REFERENCE);
        let rendered = render();
        if std::env::var_os("QSMXT_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &rendered).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(committed == rendered,
            "{} is out of date with the CLI: run `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt \
             cli_reference` and commit the result", Path::new(CLI_REFERENCE).display());
    }

    /// Every command the binary has is in the hand-written command overview, and every
    /// standalone tool's subcommands are in the tools page.
    #[test]
    fn every_command_is_in_the_commands_and_tools_pages() {
        let commands = doc("reference/commands.md");
        let tools = doc("reference/tools.md");
        let cmd = crate::cli::Cli::command();
        for sub in visible_subcommands(&cmd) {
            let name = sub.get_name();
            assert!(commands.contains(&format!("`{name}`")), "reference/commands.md does not list `{name}`");
            for s in visible_subcommands(sub) {
                assert!(tools.contains(&format!("`{}`", s.get_name())),
                    "reference/tools.md does not list `qsmxt {name} {}`", s.get_name());
            }
        }
    }

    /// Every value of every algorithm option is described in the algorithms page.
    #[test]
    fn every_algorithm_value_is_in_the_algorithms_page() {
        let algorithms = doc("reference/algorithms.md");
        let cmd = crate::cli::Cli::command();
        let run = cmd.find_subcommand("run").unwrap();
        for flag in ["qsm-algorithm", "bf-algorithm", "unwrapping-algorithm", "mask-preset",
                     "masking-input", "chisep", "r2prime-strategy", "multi-orientation-algorithm",
                     "b0-estimation", "b0-weight-type", "laplacian-solver", "output-space"] {
            let arg = run.get_arguments().find(|a| a.get_long() == Some(flag))
                .unwrap_or_else(|| panic!("`qsmxt run` has no --{flag}"));
            for v in arg.get_possible_values().iter().filter(|v| !v.is_hide_set()) {
                assert!(algorithms.contains(&format!("`{}`", v.get_name()))
                        || algorithms.contains(&format!("`--{flag} {}`", v.get_name())),
                    "reference/algorithms.md does not describe --{flag} `{}`", v.get_name());
            }
        }
    }

    /// The config's list of inversions is the CLI's: what the counts below and the methods-text
    /// route test iterate over is everything `--qsm-algorithm` accepts.
    #[test]
    fn every_inversion_is_listed_once() {
        let cmd = crate::cli::Cli::command();
        let run = cmd.find_subcommand("run").unwrap();
        let arg = run.get_arguments().find(|a| a.get_long() == Some("qsm-algorithm")).unwrap();
        let cli: Vec<String> = arg.get_possible_values().iter().map(|v| v.get_name().to_string()).collect();
        let all: Vec<String> = qsmxt_engine::QsmAlgorithm::ALL.iter().map(|a| a.to_string()).collect();
        assert_eq!(cli, all);
    }

    /// "N classical and M deep-learning" wherever the docs count the inversions. Deep learning is
    /// what QSM.rs says assumes B0 along z (`OrientationSupport::AxialOnly`).
    #[test]
    fn inversion_counts_in_the_docs_are_right() {
        use qsmxt_engine::bridge::{axial_only_algorithms, RunOrientationFacts};
        let (mut classical, mut dl) = (0, 0);
        for alg in qsmxt_engine::QsmAlgorithm::ALL {
            let mut cfg = qsmxt_engine::PipelineConfig::default();
            cfg.inversion.algorithm = alg;
            let axial = axial_only_algorithms(&cfg, RunOrientationFacts { has_measured_r2: true });
            if axial.iter().any(|a| a.starts_with("dipole inversion")) { dl += 1 } else { classical += 1 }
        }
        for path in doc_pages() {
            let text = std::fs::read_to_string(&path).unwrap();
            let page = path.display().to_string();
            let counted = |word: &str| -> Vec<usize> {
                text.split_whitespace().collect::<Vec<_>>().windows(2)
                    .filter(|w| w[1].trim_matches(|c: char| !c.is_alphanumeric() && c != '-') == word)
                    .filter_map(|w| w[0].parse().ok()).collect()
            };
            let c = counted("classical");
            let d = counted("deep-learning");
            assert!(c.iter().all(|n| *n == classical), "{page} says {c:?} classical inversions; there are {classical}");
            assert!(d.iter().all(|n| *n == dl), "{page} says {d:?} deep-learning inversions; there are {dl}");
        }
    }

    /// The dipole-inversion table marks the config's default inversion, and only it, as the default.
    #[test]
    fn the_default_inversion_is_the_one_the_docs_mark() {
        let algorithms = doc("reference/algorithms.md");
        let default = qsmxt_engine::InversionConfig::default().algorithm;
        for alg in qsmxt_engine::QsmAlgorithm::ALL {
            let row = algorithms.lines().find(|l| l.starts_with(&format!("| `{alg}` |")))
                .unwrap_or_else(|| panic!("reference/algorithms.md has no row for `{alg}`"));
            assert_eq!(row.contains("(default"), alg == default,
                "reference/algorithms.md: `{alg}` row marks the default wrongly: {row}");
        }
    }

    /// The algorithms page spells every `--mask-preset` out as the flags it stands for, and those
    /// are exactly the recipe the preset runs.
    #[test]
    fn mask_preset_expansions_in_the_docs_are_the_recipes() {
        let algorithms = doc("reference/algorithms.md");
        for (name, recipe) in qsmxt_engine::mask_presets() {
            let mut flags: Vec<String> = recipe.sections.iter()
                .map(|s| format!("--mask {}", s.compact_spec())).collect();
            if recipe.sections.len() > 1 && recipe.combine == qsmxt_engine::MaskCombine::And {
                flags.push("--mask-combine and".to_string());
            }
            flags.extend(recipe.refinements.iter().map(|r| format!("--mask-refine {}", r.compact_spec())));
            let row = format!("| `{name}` | `{}` |", flags.join(" "));
            assert!(algorithms.contains(&row), "reference/algorithms.md lacks the expansion row\n{row}");
        }
    }

    /// Every `--flag` the docs mention is one some `qsmxt` command accepts (a renamed or removed
    /// flag would otherwise linger in the prose).
    #[test]
    fn every_flag_in_the_docs_exists() {
        fn longs(cmd: &Command, out: &mut std::collections::BTreeSet<String>) {
            for a in cmd.get_arguments() {
                if let Some(l) = a.get_long() { out.insert(l.to_string()); }
                for alias in a.get_all_aliases().unwrap_or_default() { out.insert(alias.to_string()); }
            }
            for s in cmd.get_subcommands() { longs(s, out); }
        }
        let mut known = std::collections::BTreeSet::new();
        longs(&crate::cli::Cli::command(), &mut known);
        known.extend(["help", "version"].map(String::from));
        // Other programs' flags quoted inside `qsmxt` prose: sbatch's.
        let foreign = ["dependency"];
        let check = |page: &Path, text: &str| {
            for (i, _) in text.match_indices("--") {
                let before = text[..i].chars().last();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '<' || c == '!') { continue; }
                let token: String = text[i + 2..].chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
                // QSMxT 8.x's flags (`--export_dicoms`), quoted on the migration page, are not v9's.
                if token.contains('_') { continue; }
                let flag = token.trim_end_matches('-');
                if flag.is_empty() || foreign.contains(&flag) { continue; }
                // `--romeo-*` style families.
                if text[i + 2 + flag.len()..].starts_with("-*") || text[i + 2 + flag.len()..].starts_with('*') {
                    assert!(known.iter().any(|k| k.starts_with(flag)), "{}: no flag starts --{flag}", page.display());
                    continue;
                }
                assert!(known.contains(flag), "{}: --{flag} is not a qsmxt flag", page.display());
            }
        };
        for page in doc_pages() {
            if page.ends_with(CLI_REFERENCE.rsplit('/').next().unwrap()) { continue; }
            let text = std::fs::read_to_string(&page).unwrap();
            // In code blocks, only `qsmxt …` command lines (and their `\` continuations); in
            // prose, only inline code. Other tools' flags (cargo, sha256sum) are not checked.
            let (mut in_code, mut in_cmd) = (false, false);
            for line in text.lines() {
                if line.trim_start().starts_with("```") { in_code = !in_code; in_cmd = false; continue; }
                if in_code {
                    let t = line.trim_start();
                    in_cmd = in_cmd || t.starts_with("qsmxt ");
                    if in_cmd { check(&page, line); }
                    in_cmd = in_cmd && line.trim_end().ends_with('\\');
                } else {
                    for (k, span) in line.split('`').enumerate() {
                        // A flag, or a `qsmxt …` command; other programs' spans are not ours.
                        if k % 2 == 1 && (span.starts_with("--") || span.starts_with("qsmxt ")) {
                            check(&page, span);
                        }
                    }
                }
            }
        }
    }

    fn doc_pages() -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![repo().join("docs/src/content/docs")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() { stack.push(p) } else if p.extension().is_some_and(|x| x == "md" || x == "mdx") { files.push(p) }
            }
        }
        files.sort();
        files
    }

    /// The output-files page documents every file the pipeline publishes, under the name the
    /// naming module gives it, and every name kept out of BIDS through `.bidsignore`.
    #[test]
    fn every_published_name_is_in_the_outputs_page() {
        use crate::bids::derivatives::DerivativeOutputs;
        use crate::bids::entities::AcquisitionKey;
        let outputs = doc("reference/outputs.md");
        let o = DerivativeOutputs::new(Path::new("/o"));
        let k = AcquisitionKey {
            subject: "01".into(), session: None, acquisition: None, reconstruction: None,
            inversion: None, run: None, suffix: "MEGRE".into(),
        };
        let paths = [
            o.qsm_path(&k), o.mask_path(&k), o.magnitude_path(&k), o.swi_path(&k), o.swi_mip_path(&k),
            o.dseg_path(&k), o.dseg_lookup_path(&k), o.segmentation_stats_path(&k),
            o.smwi_path(&k, "paramagnetic"), o.smwi_path(&k, "diamagnetic"),
            o.smwi_mip_path(&k, "paramagnetic"), o.smwi_mip_path(&k, "diamagnetic"),
            o.t2star_path(&k), o.r2star_path(&k), o.r2_path(&k), o.r2prime_path(&k),
            o.total_field_path(&k), o.total_field_magnitude_path(&k), o.cosmos_path(&k),
            o.sti_tensor_path(&k), o.sti_mms_path(&k), o.sti_msa_path(&k), o.sti_eigenvector_path(&k),
            o.chi_para_path(&k), o.chi_dia_path(&k), o.chi_sep_total_path(&k),
            o.singlepass_qsm_path(&k), o.two_pass_mask_path(&k), o.qsm_support_path(&k),
            o.r2prime_coverage_path(&k), o.motion_coverage_path(&k),
        ];
        for p in paths {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let tail = name.strip_prefix("sub-01").unwrap();
            assert!(outputs.contains(&format!("{tail}`")), "reference/outputs.md does not list …{tail}");
        }
        for s in crate::bids::naming::BIDSIGNORED_SUFFIXES {
            assert!(outputs.contains(&format!("`{s}`")), "reference/outputs.md does not explain the bidsignored `{s}`");
        }
    }

    /// Claims the docs used to make that the code does not honour.
    #[test]
    fn docs_make_no_stale_claims() {
        for f in doc_pages() {
            let text = std::fs::read_to_string(&f).unwrap();
            let name = f.display();
            // The pipeline writes methods.md, not references.txt.
            assert!(!text.contains("references.txt"), "{name} mentions references.txt");
            // Derivatives are uncompressed NIfTI; inputs may be either.
            for line in text.lines().filter(|l| l.contains(".nii.gz")) {
                assert!(!["Chimap", "_mask", "T2starw", "dicoms"].iter().any(|s| line.contains(s))
                        || line.contains("input") || line.contains("rec-uncombined"),
                    "{name}: an output shown as .nii.gz: {line}");
            }
        }
    }
}
