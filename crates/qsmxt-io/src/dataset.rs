//! The dataset-level files of a run: the BIDS derivative description, README and `.bidsignore`,
//! the dataset's methods text and pipeline graph, the reference summary and cohort figures, and
//! cleaning a run's intermediates.

use log::{info, warn};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery;
use qsmxt_engine::PipelineConfig;

/// The dataset-level files written whatever happens to the runs: the BIDS derivative
/// description (required for a valid derivative dataset), `.bidsignore` and README, the resolved
/// configuration, and the methods text and pipeline graph as planned for `runs`. Once the runs
/// are done, [`write_dataset_summary`] replaces the last two with what the runs did.
pub fn write_dataset_files(
    derivatives_dir: &std::path::Path, bids_dir: &std::path::Path, config: &PipelineConfig,
    runs: &[discovery::QsmRun],
) -> crate::Result<()> {
    crate::bids::dataset_description::write(derivatives_dir, bids_dir)?;
    crate::bids::dataset_description::write_bidsignore(derivatives_dir)?;
    crate::bids::dataset_description::write_readme(derivatives_dir)?;
    std::fs::write(derivatives_dir.join("pipeline_config.toml"), config.to_toml().unwrap_or_default())?;
    std::fs::write(derivatives_dir.join("methods.md"), qsmxt_engine::methods::generate_methods(config))?;
    if let Some(graph) = dataset_graph(config, runs) {
        crate::bids::derivatives::write_graph(&DerivativeOutputs::new(derivatives_dir).dataset_graph_path(), &graph)?;
    }
    Ok(())
}

/// The dataset's methods text and pipeline graph from what its runs did
/// ([`qsmxt_engine::dataset::summarise`]), over every current run record in the output
/// directory ([`current_run_records`]), whichever invocation processed it. `methods.md` and
/// `pipeline.mmd`/`.svg` are replaced, and `changes/` is rebuilt to hold the runs that are
/// exceptions among them: each one's methods text and its graph as it ran. With no current
/// record, the planned files stay.
pub fn write_dataset_summary(derivatives_dir: &std::path::Path, config: &PipelineConfig) -> crate::Result<()> {
    use crate::bids::derivatives::write_graph;
    let output = DerivativeOutputs::new(derivatives_dir);
    let done: Vec<(String, qsmxt_engine::plan::Plan, qsmxt_engine::scan::ScanFacts, qsmxt_engine::record::RunRecord)> =
        current_run_records(derivatives_dir, config).into_iter()
            .map(|(name, file)| (name, qsmxt_engine::plan::plan(config, &file.scan), file.scan, file.record))
            .collect();
    let inputs: Vec<_> = done.iter().map(|(_, p, f, r)| (p, f, r)).collect();
    let Some(summary) = qsmxt_engine::dataset::summarise(config, &inputs, "qsmxt") else { return Ok(()) };
    std::fs::write(derivatives_dir.join("methods.md"), summary.methods.to_markdown())?;
    write_graph(&output.dataset_graph_path(), &summary.graph)?;
    let changes = output.changes_dir();
    if changes.exists() {
        std::fs::remove_dir_all(&changes)?;
    }
    for &i in &summary.changed {
        let name = &done[i].0;
        std::fs::create_dir_all(&changes)?;
        std::fs::write(output.change_path(name, false), summary.runs[i].methods.to_markdown())?;
        write_graph(&output.change_path(name, true), &summary.runs[i].after)?;
    }
    info!("Methods and pipeline graph from {} run(s)", done.len());
    if !summary.changed.is_empty() {
        info!("{} run(s) processed differently from the rest -> {}", summary.changed.len(), changes.display());
    }
    Ok(())
}

/// Every current run record under `workflow/` (`*_runrecord.json`), by the run's name (its BIDS
/// basename), in path order. A record is current when the pipeline state beside it
/// (`.pipeline_state.json`) says its run completed under `config`: a run since processed with
/// other settings, or one whose latest attempt did not finish, is left out, so a record can
/// only describe outputs that are there now. Unreadable records, and records written before
/// they carried the run's scan facts, are left out with a warning.
pub fn current_run_records(
    derivatives_dir: &std::path::Path, config: &PipelineConfig,
) -> Vec<(String, crate::pipeline::runner::RunRecordFile)> {
    const SUFFIX: &str = "_runrecord.json";
    let pattern = derivatives_dir.join("workflow").join("**").join(format!("*{SUFFIX}"));
    let mut paths: Vec<std::path::PathBuf> = glob::glob(&pattern.to_string_lossy())
        .map(|g| g.flatten().collect()).unwrap_or_default();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(SUFFIX)) else { continue };
        let state = path.with_file_name(".pipeline_state.json");
        if let Err(why) = crate::pipeline::graph::state_completed_under(&state, config) {
            log::debug!("{name}: its run record is not current: the run {why}");
            continue;
        }
        match std::fs::read_to_string(&path).map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<crate::pipeline::runner::RunRecordFile>(&t).map_err(|e| e.to_string())) {
            Ok(mut file) => {
                // A record from before findings carried their kind: the kind follows the strength.
                for r in &mut file.record.recommendations {
                    r.finding.kind = r.finding.strength.kind();
                }
                out.push((name.to_string(), file))
            }
            Err(e) => warn!("{name}: its run record cannot be read ({}: {e}); re-run it to include it", path.display()),
        }
    }
    out
}

/// The pipeline the dataset's runs will take, as planned from each run's scan: the graph they
/// share, or, where their scans make them differ, all of them merged
/// ([`qsmxt_engine::pipeline_graph::PipelineGraph::merged`]).
pub fn dataset_graph(config: &PipelineConfig, runs: &[discovery::QsmRun]) -> Option<qsmxt_engine::pipeline_graph::PipelineGraph> {
    let graphs: Vec<_> = runs.iter().map(|run| {
        let facts = crate::pipeline::requirements::scan_facts(run, config);
        let plan = qsmxt_engine::plan::plan(config, &facts);
        qsmxt_engine::pipeline_graph::pipeline_graph(&plan, config, &facts, None)
    }).collect();
    qsmxt_engine::pipeline_graph::PipelineGraph::merged(&graphs)
}

/// One row per referenced susceptibility map of each run: what it was referenced to, and what
/// that removed.
///
/// The same facts are in each map's JSON sidecar, but reading forty sidecars to find the subject
/// whose reference region came out empty is not a thing anyone does. This is the file you scan.
pub fn write_reference_summary(
    derivatives_dir: &std::path::Path, output: &DerivativeOutputs,
    keys: &[crate::bids::entities::AcquisitionKey],
) {
    let mut body = String::new();
    for k in keys {
        // One row per referenced map — the two-pass single-pass map has its own offset — each
        // read from that map's own sidecar.
        for spec in crate::pipeline::stats::candidate_maps(&output.published(k), k) {
            if !crate::pipeline::stats::is_susceptibility(spec.name) {
                continue;
            }
            let Some(sidecar) = crate::bids::entities::sidecar_path(&spec.path) else { continue };
            let Ok(text) = std::fs::read_to_string(&sidecar) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(reference) = v.get("QsmReference").and_then(|r| r.as_str()) else { continue };
            let num = |k: &str| v.get(k).and_then(|x| x.as_f64())
                .map(|x| format!("{x:.6}")).unwrap_or_default();
            let row = [
                k.subject.as_str(),
                k.session.as_deref().unwrap_or(""),
                k.acquisition.as_deref().unwrap_or(""),
                k.reconstruction.as_deref().unwrap_or(""),
                k.inversion.as_deref().unwrap_or(""),
                k.run.as_deref().unwrap_or(""),
                spec.name,
                reference,
                &num("QsmReferenceOffsetPpm"),
                &v.get("QsmReferenceVoxels").and_then(|x| x.as_u64())
                    .map(|x| x.to_string()).unwrap_or_default(),
            ];
            body.push_str(&row.join("\t"));
            body.push('\n');
        }
    }
    if body.is_empty() {
        return;
    }
    let path = derivatives_dir.join("desc-reference_summary.tsv");
    let header = REFERENCE_SUMMARY_COLUMNS.join("\t");
    match std::fs::write(&path, format!("{header}\n{body}")) {
        Ok(()) => info!("QSM reference per run -> {}", path.display()),
        Err(e) => warn!("Could not write the reference summary: {}", e),
    }
}

/// Columns of `desc-reference_summary.tsv`: the run's entities, which map (its BIDS suffix with
/// any `desc-`, as in the statistics table), and what it was referenced to.
pub const REFERENCE_SUMMARY_COLUMNS: [&str; 10] = [
    "subject", "session", "acquisition", "reconstruction", "inversion", "run",
    "map", "reference", "offset_ppm", "n_voxels",
];

/// `--clean-intermediates` for one completed run: its workflow images, and the working-grid copies
/// of outputs returned to the acquired grid. Best-effort: a run whose outputs are all written must
/// not fail because a temporary file could not be removed.
pub fn clean_run(output: &DerivativeOutputs, run: &discovery::QsmRun) {
    if let Err(e) = crate::pipeline::graph::clean_intermediates(&output.output_dir, &run.key) {
        warn!("{}: could not clean the intermediates: {}", run.key, e);
    }
    let stash = output.working_grid_dir(&run.key);
    if stash.exists() {
        if let Err(e) = std::fs::remove_dir_all(&stash) {
            warn!("{}: could not remove {}: {}", run.key, stash.display(), e);
        }
    }
}

/// Draw the cohort figures from the dataset-level table.
///
/// Each bar is the mean of the runs' means and its whisker is the spread *across* runs — a
/// different quantity from the per-run figures, whose whiskers are the spread of voxels inside one
/// structure. The subtitle says which, because the two look identical and a reader would otherwise
/// take cohort variability for within-structure variability.
///
/// Best-effort throughout: the table is the deliverable and is already written.
pub fn write_group_figures(derivatives_dir: &std::path::Path, table_path: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(table_path) else { return };
    let (rows, n_runs) = crate::pipeline::figure::aggregate_runs(
        &crate::pipeline::stats::parse_tsv(&text));
    if rows.is_empty() || n_runs < 2 {
        // With a single run the cohort view would just restate that run's figure, with every
        // whisker at zero.
        return;
    }
    let subtitle = format!("Mean of {n_runs} run means; whiskers ±1 SD across runs");
    let vol_subtitle = format!("Mean volume over {n_runs} runs; ± is 1 SD across runs");
    match crate::pipeline::figure::render_all(
        &rows, &format!("All runs (n = {n_runs})"), &subtitle, &vol_subtitle) {
        Ok(figs) => for f in figs {
            match f.save_in(derivatives_dir, "") {
                Ok(p) => info!("Cohort figure -> {}", p.display()),
                Err(e) => warn!("Could not write {}: {}", f.stem, e),
            }
        },
        Err(e) => warn!("Could not draw the cohort figures: {}", e),
    }
}
