//! Checking a run's data against the config, before any processing.

use super::*;

/// The result of checking every run: one error line per requested output a run cannot produce,
/// one warning per implied output it cannot.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Check {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// The same problems, each as data: for a front end that words them itself (qsmbly names
    /// settings by their labels, not by their flags).
    pub problems: Vec<Problem>,
    /// With motion correction on, each run [`check_scan`] checked: its key, echo count and
    /// whether it has magnitude images, for [`motion_correction_summary`].
    #[serde(skip)]
    pub motion_runs: Vec<(String, usize, bool)>,
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
///
/// Motion correction is decided per run: rather than a line per run it cannot correct, the
/// runs are counted ([`Check::motion_runs`]), and [`Check::finish`] says how many it runs for
/// and which it is skipped for ([`motion_correction_summary`]).
pub fn check_scan(key: &str, f: &ScanFacts, config: &PipelineConfig, out: &mut Check) {
    let Check { errors, warnings, problems, .. } = scan_check(f, config);
    let motion = motion_correction_note(f, config);
    out.problems.extend(problems);
    out.errors.extend(errors.into_iter().map(|e| format!("{key}: {e}")));
    out.warnings.extend(warnings.into_iter().filter(|w| Some(w) != motion.as_ref()).map(|w| format!("{key}: {w}")));
    if config.motion_correction.enabled {
        out.motion_runs.push((key.to_string(), f.n_echoes, f.has_magnitude));
    }
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
    if let Some(why) = tukey_window_problem(f, config) {
        let path = "pipeline.tukey_window";
        out.errors.push(format!("--tukey-window {why}"));
        out.problems.push(Problem {
            severity: Severity::Error, setting: Some(path), implied_by: None,
            settings: vec![path], reason: why.clone(), message: format!("Tukey window {why}"),
        });
    }
    // Otherwise only the run refuses it: a browser run would first download the weights (#495).
    if config.pipeline.do_chi_separation && config.separation.algorithm == SeparationAlgorithm::ChiSepNet {
        if let Err(why) = config.separation.chi_sepnet.validate() {
            let path = match config.separation.chi_sepnet.route {
                crate::ChiSepNetRoute::Toolbox => "separation.chi_sepnet.frame_patch",
                crate::ChiSepNetRoute::Native => "separation.chi_sepnet.patch",
            };
            out.errors.push(why.clone());
            out.problems.push(Problem {
                severity: Severity::Error, setting: Some(path), implied_by: None,
                settings: vec![path], reason: why.clone(), message: why,
            });
        }
    }
    let label = |path: &str| crate::schema::setting(path).map_or_else(|| path.to_string(), |s| s.full_label.to_string());
    // The reconstruction's own methods: what they need of the data.
    if config.pipeline.do_qsm {
        if let Some(why) = per_echo_problem(f, config) {
            let path = "qsm.per_echo";
            out.errors.push(format!("--qsm-per-echo {why}"));
            out.problems.push(Problem {
                severity: Severity::Error, setting: Some(path), implied_by: None,
                settings: vec![path], reason: why.clone(), message: format!("{} {why}", label(path)),
            });
        }
        let bg = config.bg_removal.algorithm;
        // A per-echo QSM reconstructs each echo as a single-echo scan's: its methods are checked
        // on each echo's, and no echo combination runs.
        let per_echo = crate::bridge::per_echo(config, f.n_echoes);
        let echoes: Vec<ScanFacts> = if per_echo { (0..f.n_echoes).map(|e| one_echo(f, e)).collect() } else { vec![f.clone()] };
        let on_each = |check: fn(&ScanFacts, &PipelineConfig) -> Option<String>| echoes.iter().find_map(|f| check(f, config));
        let methods = [
            ("field_mapping.b0_estimation", "--b0-estimation", config.field_mapping.b0_estimation.to_string(),
             if !per_echo && crate::bridge::field_mapping_runs(config, f.n_echoes) { field_mapping_problem(f, config) } else { None }),
            ("field_mapping.unwrapping_algorithm", "--unwrapping-algorithm", config.field_mapping.unwrapping_algorithm.to_string(),
             if crate::bridge::field_mapping_runs(config, f.n_echoes) { unwrapping_problem(f, config) } else { None }),
            ("inversion.algorithm", "--qsm-algorithm", config.inversion.algorithm.to_string(),
             on_each(inversion_problem)),
            ("bg_removal.algorithm", "--bf-algorithm", bg.to_string(),
             if crate::bridge::skips_bgremove(config) { None } else { on_each(bg_removal_problem) }),
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
    // What a chosen method cannot run without, from QSM.rs's pipeline rules (`required`): refused.
    // QSMxT's own required changes (an output another setting needs) are not: the run applies
    // them itself and says so ([`crate::recommendations::Recommendation::applied_by_run`]).
    for r in crate::recommendations::recommendations(config, &crate::recommendations::Facts::of_scan(f)) {
        if r.strength != crate::recommendations::Strength::Required || r.applied_by_run() {
            continue;
        }
        let path = crate::schema::setting(&r.setting).map(|s| s.path);
        let or_off = if r.flag_trigger().is_some() { format!(" Or turn off {}.", r.flag_name()) } else { String::new() };
        let why = format!("{} is {}, and {} requires {}: {}.{or_off}", r.label, r.current_text(), r.trigger.name, r.recommended.name, r.reason);
        out.errors.push(format!("{why} (rule {}; --accept-recommendations applies it)", r.rule));
        out.problems.push(Problem {
            severity: Severity::Error, setting: path, implied_by: None, settings: path.into_iter().collect(),
            reason: why.clone(), message: why,
        });
    }
    if let Some(note) = medi_csf_note(f, config) {
        out.warnings.push(note.clone());
        out.problems.push(Problem {
            severity: Severity::Warning, setting: Some("inversion.medi.csf"), implied_by: None,
            settings: vec!["inversion.medi.csf"], reason: note.clone(), message: note,
        });
    }
    if let Some(note) = motion_correction_note(f, config) {
        let path = "motion_correction.enabled";
        out.warnings.push(note.clone());
        out.problems.push(Problem {
            severity: Severity::Warning, setting: Some(path), implied_by: None,
            settings: vec![path], reason: note.clone(), message: note,
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
    /// How many of the checked runs motion correction runs for, and which it is skipped for
    /// ([`motion_correction_summary`]); `None` with motion correction off.
    pub fn motion_summary(&self) -> Option<String> {
        motion_correction_summary(&self.motion_runs)
    }

    /// The outcome of checking every scan before any is processed. Warnings are logged; errors,
    /// all of them at once, fail the command.
    pub fn finish(self) -> Result<(), EngineError> {
        for w in &self.warnings {
            log::warn!(target: "qsmxt_engine::requirements", "{w}");
        }
        // Per run: a warning only where some run goes without it.
        if let Some(summary) = self.motion_summary() {
            if self.motion_runs.iter().any(|(_, n, mag)| motion_correction_lacks(*n, *mag).is_some()) {
                log::warn!(target: "qsmxt_engine::requirements", "{summary}");
            } else {
                log::info!(target: "qsmxt_engine::requirements", "{summary}");
            }
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
