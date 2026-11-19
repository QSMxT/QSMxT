//! Every form the TUI reaches from a fresh one, changing one choice at a time: what the golden
//! form files (`form_snapshots`), the config round trips (`form_from_config`) and the checks
//! against the engine's settings view (`schema_parity`) go through.
//!
//! The search changes every option of every select and every checkbox, with and without an
//! orientation grouping and an echo limit, for as long as a change shows a row not seen before.

use std::collections::BTreeSet;

use super::app::*;

/// Rows that edit no config setting of their own.
pub(super) const NOT_CONFIG: &[&str] = &[
    // Expands into the mask recipe (masking.sections and friends), which the recipe rows edit.
    "mask_preset",
    // Run options of `qsmxt run`, not settings of the config.
    "source_dicom", "execution_mode", "dry_run", "n_procs", "mem_limit_gb", "slurm_account",
    "slurm_partition", "slurm_time", "slurm_mem", "slurm_cpus", "slurm_submit", "debug", "force", "clean_intermediates",
];

/// One change to a form.
#[derive(Clone, Debug)]
pub(super) enum Act {
    Toggle(&'static str),
    Select(&'static str, usize),
    /// An orientation grouping on the Input tab.
    Group,
    /// Limit the run to this many echoes.
    Echoes(usize),
}

pub(super) fn build(acts: &[Act]) -> App {
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
pub(super) fn explored() -> &'static [Vec<Act>] {
    static FORMS: std::sync::OnceLock<Vec<Vec<Act>>> = std::sync::OnceLock::new();
    FORMS.get_or_init(explore_forms)
}

/// The exploration behind [`explored`], from scratch each call.
fn explore_forms() -> Vec<Vec<Act>> {
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
}

/// The golden files of what each form builds (`form_snapshots`) list the forms in the order
/// [`explored`] finds them, so the exploration must find the same forms in the same order every
/// time: nothing in it may follow a hash map's order or the clock.
#[test]
fn exploring_finds_the_same_forms_in_the_same_order() {
    assert_eq!(format!("{:?}", explore_forms()), format!("{:?}", explored()));
}
