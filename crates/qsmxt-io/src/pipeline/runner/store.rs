//! `qsmxt run`'s [`StageStore`]: a run's volumes as files under the derivatives directory.

use std::path::{Path, PathBuf};
use std::time::Instant;

use qsm_core::io;

use super::{
    find_custom_derivative, find_custom_mask, input_identity, load_mask, load_volume, prefetch_weights,
    published_space, read_json_object, restore_working_grid, robust_fit_report_path, save_mask,
    validate_run_dims, write_step_provenance,
};
use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::error::QsmxtError;
use crate::nifti::write::{write_volume, write_volume_f64};
use qsmxt_engine::*;
use crate::pipeline::graph::{PipelineState, RunMetadata};
use crate::pipeline::stats;
use qsmxt_engine::plan::{Pass as EnginePass, Stage};
use qsmxt_engine::record::Decision;
use qsmxt_engine::store::{Completed, Contrast, Data, Header, Item, StageStore, Supplied};

/// The log target these messages had before they moved into this module.
const LOG_TARGET: &str = "qsmxt_io::pipeline::runner";

/// `qsmxt run`'s [`StageStore`]: a run's volumes as files under the derivatives directory, named
/// by [`DerivativeOutputs`], its step cache in the run's state file ([`PipelineState`]), and a
/// provenance record for every step.
///
/// Returning the outputs to the acquired grid rewrites them in place; the working-grid originals
/// are kept aside first, and put back when a later run starts ([`restore_working_grid`]).
pub(super) struct FileStore<'a> {
    pub(super) run: &'a QsmRun,
    pub(super) config: &'a PipelineConfig,
    /// The run's outputs, named in the space they are published in once the grid is known.
    pub(super) output: DerivativeOutputs,
    /// The run's grid, once loaded.
    pub(super) meta: Option<RunMetadata>,
    pub(super) state: PipelineState,
    pub(super) state_path: PathBuf,
    /// When the step being computed started, for its provenance.
    pub(super) started: Option<Instant>,
    /// The step being computed.
    pub(super) step: Option<String>,
    /// The supplied brain mask, once found.
    pub(super) supplied_mask: Option<PathBuf>,
    /// Files the step being computed wrote beside its outputs (the figures beside the statistics
    /// table), recorded with them.
    pub(super) extra_outputs: Vec<PathBuf>,
    /// What the step being computed keeps in the state file rather than in a file of its own
    /// (SynthSeg's per-label volumes).
    pub(super) metadata: Option<serde_json::Value>,
    /// The working-grid copies of the run's published volumes are kept aside for the return to the
    /// acquired grid being computed.
    pub(super) working_grid_kept: bool,
}

impl FileStore<'_> {
    /// Where `item` is kept, when it has a place.
    pub(super) fn path(&self, item: &Item) -> Option<PathBuf> {
        let (o, key) = (&self.output, &self.run.key);
        let echo = |n: usize| self.run.echoes.get(n.checked_sub(1)?);
        let pass = |p: EnginePass, main: PathBuf, reliable: PathBuf| match p {
            EnginePass::Main => main,
            EnginePass::Reliable => reliable,
        };
        Some(match *item {
            Item::AcquiredPhase(n) => echo(n)?.phase_nifti.clone(),
            Item::AcquiredMagnitude(n) => echo(n)?.magnitude_nifti.clone()?,
            // A real/imaginary echo's real part is its `phase_nifti`, beside its `part-imag` file.
            Item::AcquiredReal(n) => echo(n).filter(|e| e.imaginary_nifti.is_some())?.phase_nifti.clone(),
            Item::AcquiredImaginary(n) => echo(n)?.imaginary_nifti.clone()?,
            Item::CoilPhase(c, n) => self.run.coils.as_ref()?.get(c.checked_sub(1)?)?.echoes.get(n.checked_sub(1)?)?.phase_nifti.clone(),
            Item::CoilMagnitude(c, n) =>
                self.run.coils.as_ref()?.get(c.checked_sub(1)?)?.echoes.get(n.checked_sub(1)?)?.magnitude_nifti.clone()?,
            Item::CombinedEchoPhase(n) => o.combined_phase_path(key, n),
            Item::CombinedEchoMagnitude(n) => o.combined_mag_path(key, n),
            Item::CoilCombinationMask => o.combine_mask_path(key),
            Item::ScaledPhase(n) => o.acquired_grid_phase_path(key, "scale_phase", n),
            Item::ScaledMagnitude(n) => o.acquired_grid_mag_path(key, "scale_phase", n),
            Item::WindowedPhase(n) => o.acquired_grid_phase_path(key, "tukey_window", n),
            Item::WindowedMagnitude(n) => o.acquired_grid_mag_path(key, "tukey_window", n),
            Item::Phase(n) => o.phase_scaled_path(key, n),
            Item::Magnitude(n) => o.mag_path(key, n),
            Item::CorrectedPhase(n) => o.motion_phase_path(key, n),
            Item::CorrectedMagnitude(n) => o.motion_mag_path(key, n),
            // Published unless `--no-magnitude`: then kept among the run's intermediates for the
            // steps that read it.
            Item::CombinedMagnitude if self.config.pipeline.do_magnitude => o.magnitude_path(key),
            Item::CombinedMagnitude => o.intermediate_magnitude_path(key),
            Item::HomogeneousMagnitude(n) => o.homogeneous_mag_path(key, n),
            Item::VesselMagnitude => o.vessel_magnitude_path(key),
            Item::BrainMask => o.mask_path(key),
            // Found before the mask step reads it too, for that step's key.
            Item::SuppliedMask => self.supplied_mask.clone()
                .or_else(|| find_custom_mask(self.run, self.config.masking.custom_mask_tool.as_deref()?))?,
            Item::CoverageMask => o.motion_coverage_path(key),
            Item::FieldPpm => o.field_ppm_path(key),
            Item::TotalFieldHz => o.total_field_path(key),
            Item::TotalFieldMagnitude => o.total_field_magnitude_path(key),
            Item::RobustFitReport => robust_fit_report_path(&o.field_ppm_path(key)),
            Item::FieldReliability => o.field_reliability_path(key),
            Item::FieldMask => o.field_mask_path(key),
            Item::LocalField(p) => pass(p, o.local_field_path(key), o.reliable_local_field_path(key)),
            Item::LocalFieldMask(p) => pass(p, o.bg_mask_path(key), o.reliable_bg_mask_path(key)),
            Item::RawChi(p) => pass(p, o.chi_raw_path(key), o.reliable_chi_raw_path(key)),
            Item::ReliableMask => o.two_pass_mask_path(key),
            Item::QsmartIntermediate(p, name) => o.qsmart_intermediate_path(key, &Stage::Qsmart(p).name(), name)?,
            Item::CombinedRawChi => o.two_pass_chi_raw_path(key),
            Item::EchoField(n) => o.echo_field_path(key, n),
            Item::EchoLocalField(n) => o.echo_local_field_path(key, n),
            Item::EchoLocalFieldMask(n) => o.echo_bg_mask_path(key, n),
            Item::EchoRawChi(n) => o.echo_chi_raw_path(key, n),
            Item::EchoChi(n) => o.echo_qsm_path(key, n),
            Item::Chi => o.qsm_path(key),
            Item::ChiSupport => o.qsm_support_path(key),
            Item::SinglePassChi => o.singlepass_qsm_path(key),
            Item::Segmentation => o.dseg_path(key),
            Item::SuppliedSegmentation => self.custom(self.config.segmentation.custom_dseg_tool.as_deref(), "*_dseg.nii*", &[])?,
            Item::SegmentationLabels => o.dseg_lookup_path(key),
            // Kept in the state file, with the step's record.
            Item::SegmentationVolumes => return None,
            Item::RegionStats => o.segmentation_stats_path(key),
            Item::MotionReport => o.motion_report_path(key),
            Item::Swi => o.swi_path(key),
            Item::SwiMip => o.swi_mip_path(key),
            Item::SwiPhase => o.swi_phase_path(key),
            Item::Smwi(c) => o.smwi_path(key, c),
            Item::SmwiMip(c) => o.smwi_mip_path(key, c),
            Item::R2star => o.r2star_path(key),
            Item::T2star => o.t2star_path(key),
            Item::MeseMagnitude(n) => self.run.mese.as_ref()?.magnitude_niftis.get(n.checked_sub(1)?)?.clone(),
            Item::SuppliedR2 => self.custom(self.config.separation.custom_r2_tool.as_deref(), "*_R2map.nii*", &[])?,
            // χ-sepnet's toolbox route measures R2′ where the scan can, without R2′ or R2 having
            // been asked for: those maps are kept with the run's intermediates, not published.
            Item::R2 if self.config.computes_unasked_r2prime().1 => o.intermediate_item_path(key, "r2_r2prime", Item::R2),
            Item::R2 => o.r2_path(key),
            Item::SuppliedR2prime =>
                self.custom(self.config.separation.custom_r2prime_tool.as_deref(), "*_R2primemap.nii*", &[])?,
            Item::R2prime if self.config.computes_unasked_r2prime().0 =>
                o.intermediate_item_path(key, "r2_r2prime", Item::R2prime),
            Item::R2prime => o.r2prime_path(key),
            Item::R2primeCoverage if self.config.computes_unasked_r2prime().0 =>
                o.intermediate_item_path(key, "r2_r2prime", Item::R2primeCoverage),
            Item::R2primeCoverage => o.r2prime_coverage_path(key),
            Item::SuppliedChi => self.custom(self.config.separation.custom_qsm_tool.as_deref(), "*_Chimap.nii*", &["desc-"])?,
            Item::SeparatedChi(Contrast::Paramagnetic) => o.chi_para_path(key),
            Item::SeparatedChi(Contrast::Diamagnetic) => o.chi_dia_path(key),
            Item::SeparatedTotalChi => o.chi_sep_total_path(key),
        })
    }

    /// The derivative `tool` supplies for this run, when one is configured and found.
    pub(super) fn custom(&self, tool: Option<&str>, glob: &str, exclude: &[&str]) -> Option<PathBuf> {
        find_custom_derivative(self.run, tool?, glob, exclude)
    }

    /// Before the first output is returned to the acquired grid: copy every one of the run's
    /// published volumes aside, since later steps read them back on the working grid (the mask,
    /// the combined magnitude, ...) and a re-run has to start from them rather than from what lands
    /// in `anat/`. Only this run's files: every run of the session shares `anat/`, and another
    /// run's outputs may be on its own working grid, or still being read by a reconstruction in
    /// progress.
    pub(super) fn keep_working_grid_copies(&mut self) -> crate::Result<()> {
        let stash = self.output.working_grid_dir(&self.run.key);
        if stash.exists() {
            fs_err::remove_dir_all(&stash)?;
        }
        fs_err::create_dir_all(&stash)?;
        for f in self.output.run_published_niftis(&self.run.key) {
            // Not a file some other configuration left in another space.
            if f.file_name().and_then(|n| crate::bids::naming::entity_of(&n.to_string_lossy(), "space")).is_some() {
                continue;
            }
            if let Some(name) = f.file_name() {
                fs_err::copy(&f, stash.join(name))?;
            }
        }
        self.working_grid_kept = true;
        Ok(())
    }

    /// One figure per map beside the statistics table. Best-effort: the numbers are the
    /// deliverable, and a figure that fails to draw must not cost the run the statistics it
    /// already computed.
    pub(super) fn draw_figures(&mut self, table: &str) {
        let anat = self.output.anat_dir(&self.run.key);
        match crate::pipeline::figure::render_all(
            &stats::parse_tsv(table),
            &self.run.key.to_string(),
            "Mean per structure; whiskers ±1 SD over the structure's voxels",
            "Volume from the SynthSeg posteriors, partial volume included",
        ) {
            Ok(figs) => for f in figs {
                match f.save_in(&anat, &self.run.key.basename()) {
                    Ok(p) => { log::info!(target: LOG_TARGET, "Figure -> {}", p.display()); self.extra_outputs.push(p); }
                    Err(e) => log::warn!(target: LOG_TARGET, "Could not write {}: {e}", f.stem),
                }
            },
            Err(e) => log::warn!(target: LOG_TARGET, "Could not draw the per-structure figures: {e}"),
        }
    }

    pub(super) fn existing(&self, item: &Item) -> crate::Result<PathBuf> {
        self.path(item).ok_or_else(|| QsmxtError::Config(format!("{}: no {item:?} to read", self.run.key)))
    }

    pub(super) fn meta(&self) -> &RunMetadata {
        self.meta.as_ref().expect("the run's grid is known before any output is written")
    }
}

/// Copy a NIfTI file to `dest`, compressed as `dest`'s name says: a `.nii.gz` source copied to a
/// `.nii` name is decompressed (and the other way round), so every file is what its name says;
/// other readers (nibabel) refuse gzipped data under a `.nii` name.
pub(super) fn copy_nifti(source: &Path, dest: &Path) -> std::io::Result<()> {
    let gz = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gz"));
    match (gz(source), gz(dest)) {
        (true, false) => {
            let mut from = flate2::read::MultiGzDecoder::new(std::io::BufReader::new(fs_err::File::open(source)?));
            let mut to = std::io::BufWriter::new(fs_err::File::create(dest)?);
            std::io::copy(&mut from, &mut to)?;
            std::io::Write::flush(&mut to)
        }
        (false, true) => {
            let mut from = std::io::BufReader::new(fs_err::File::open(source)?);
            let mut to = flate2::write::GzEncoder::new(
                std::io::BufWriter::new(fs_err::File::create(dest)?), flate2::Compression::default());
            std::io::copy(&mut from, &mut to)?;
            std::io::Write::flush(&mut to.finish()?)
        }
        _ => fs_err::copy(source, dest).map(|_| ()),
    }
}

impl StageStore for FileStore<'_> {
    type Error = QsmxtError;

    fn input_identity(&mut self) -> serde_json::Value {
        serde_json::Value::Array(input_identity(self.run))
    }

    fn header(&mut self) -> crate::Result<Header> {
        let first = &self.run.echoes[0];
        let failed = |e: String| QsmxtError::NiftiIo(format!("{}: {}", first.phase_nifti.display(), e));
        let bytes = fs_err::read(&first.phase_nifti).map_err(|e| failed(e.to_string()))?;
        let first_phase = io::load_nifti(&bytes).map_err(failed)?;
        validate_run_dims(self.run, &first_phase)?;
        // N4 is given the image geometry ITK reads from this file's own header.
        let orientation = qsmxt_engine::scan::HeaderOrientation::read(&bytes).map_err(failed)?;
        Ok(Header {
            dims: first_phase.dims, voxel_size: first_phase.voxel_size, affine: first_phase.affine,
            orientation: Some(orientation),
            sidecar: crate::bids::sidecar::read_sidecar(&first.phase_json).ok(),
        })
    }

    fn sidecar(&mut self) -> crate::Result<Option<qsmxt_engine::scan::QsmSidecar>> {
        Ok(Some(crate::bids::sidecar::read_sidecar(&self.run.echoes[0].phase_json)?))
    }

    fn coils(&mut self) -> Vec<u32> {
        self.run.coils.iter().flatten().map(|c| c.coil_number).collect()
    }

    fn cached_scan(&mut self, key: &str) -> Option<RunMetadata> {
        // A cache from before the load step was keyed is taken as it stands.
        if let Some(Some(stored)) = self.state.completed_steps.get("load").map(|r| r.params_hash.as_ref()) {
            if stored != key {
                log::info!(target: LOG_TARGET, "Input files or geometry settings changed (obliquity threshold, algorithm, \
                            motion correction) — recomputing everything");
                self.state.completed_steps.clear();
            }
        }
        if !self.state.is_step_cached("load") {
            return None;
        }
        self.state.run_metadata.clone()
    }

    fn record_scan(&mut self, key: &str, meta: &RunMetadata, decisions: &[Decision]) -> crate::Result<()> {
        self.state.run_metadata = Some(meta.clone());
        self.state.mark_completed("load", vec![], Some(key.to_string()));
        self.state.set_decisions("load", decisions);
        self.state.save(&self.state_path)
    }

    fn decisions(&mut self, step: &str) -> Vec<Decision> {
        self.state.completed_steps.get(step).map(|r| r.decisions.clone()).unwrap_or_default()
    }

    fn begin(&mut self, meta: &RunMetadata) -> crate::Result<()> {
        self.meta = Some(meta.clone());
        restore_working_grid(self.run, &self.output, &mut self.state, &self.state_path)?;
        // A run left on the axial grid it was resampled to names its outputs in that space (#275).
        let space = published_space(self.config, meta);
        if self.state.space.as_deref() != space {
            // Every output of the last run is under the other names: nothing cached can be found,
            // and nothing under those names may stay beside the new ones.
            self.state.remove_recorded_outputs(&self.state_path);
            self.state.completed_steps.retain(|step, _| step == "load");
            self.state.space = space.map(str::to_string);
            self.state.save(&self.state_path)?;
        }
        self.output = self.output.in_space(space);
        // `--no-magnitude`: a combined magnitude an earlier run published does not stay beside
        // this run's outputs (nor reach its DICOM export).
        if !self.config.pipeline.do_magnitude {
            let published = self.output.magnitude_path(&self.run.key);
            for path in std::iter::once(published.clone()).chain(crate::bids::entities::sidecar_path(&published)) {
                if path.exists() {
                    fs_err::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    fn has(&mut self, item: &Item) -> bool {
        self.path(item).is_some_and(|p| p.exists())
    }

    fn real(&mut self, item: &Item) -> crate::Result<Vec<f64>> {
        load_volume(&self.existing(item)?)
    }

    fn mask(&mut self, item: &Item) -> crate::Result<Vec<u8>> {
        load_mask(&self.existing(item)?)
    }

    fn supplied_mask(&mut self) -> Supplied {
        let tool = self.config.masking.custom_mask_tool.as_deref().unwrap_or_default();
        let Some(path) = find_custom_mask(self.run, tool) else {
            let ses = self.run.key.session.as_ref().map(|s| format!("/ses-{}", s)).unwrap_or_default();
            return Supplied::Missing { searched: format!("sub-{}{}", self.run.key.subject, ses) };
        };
        self.supplied_mask = Some(path.clone());
        let origin = path.display().to_string();
        match io::read_nifti_file(&path) {
            Ok(nd) => Supplied::Found { origin, data: nd.data, dims: nd.dims, affine: nd.affine },
            Err(error) => Supplied::Unreadable { origin, error },
        }
    }

    fn mese(&mut self) -> Option<qsmxt_engine::store::Mese> {
        self.run.mese.as_ref().map(|m| qsmxt_engine::store::Mese {
            name: m.key.to_string(), dims: m.dims, affine: m.affine, echo_times: m.echo_times.clone(),
        })
    }

    fn supplied(&mut self, item: &Item) -> Supplied {
        let Some(path) = self.path(item) else {
            return Supplied::Missing { searched: format!("the inputs of {}", self.run.key) };
        };
        let origin = path.display().to_string();
        match io::read_nifti_file(&path) {
            Ok(nd) => Supplied::Found { origin, data: nd.data, dims: nd.dims, affine: nd.affine },
            Err(error) => Supplied::Unreadable { origin, error },
        }
    }

    /// The path, which is what earlier releases keyed a step by.
    fn identity(&mut self, item: &Item) -> serde_json::Value {
        serde_json::json!(self.path(item).unwrap_or_default().to_string_lossy())
    }

    fn recorded_key(&mut self, step: &str) -> Option<String> {
        self.state.completed_steps.get(step).and_then(|r| r.params_hash.clone())
    }

    fn cached(&mut self, step: &str, key: &str) -> bool {
        self.state.is_step_cached_with_hash(step, Some(key))
    }

    fn start(&mut self, step: &str) {
        self.started = Some(Instant::now());
        self.step = Some(step.to_string());
        self.extra_outputs.clear();
        self.metadata = None;
        self.working_grid_kept = false;
    }

    fn models(&mut self, models: &[&'static str]) -> crate::Result<()> {
        for id in models {
            prefetch_weights(id, &self.run.key.to_string())?;
        }
        Ok(())
    }

    fn put(&mut self, item: Item, data: Data<'_>) -> crate::Result<()> {
        if let (Item::SegmentationVolumes, Data::Json(volumes)) = (item, data) {
            self.metadata = Some(serde_json::json!({ "volumes": volumes }));
            return Ok(());
        }
        if self.step.as_deref() == Some("output_space") && !self.working_grid_kept {
            self.keep_working_grid_copies()?;
        }
        let Some(path) = self.path(&item) else {
            match item {
                Item::QsmartIntermediate(_, name) =>
                    log::warn!(target: LOG_TARGET, "QSMART reported an intermediate QSMxT has no name for ({name}); not saved"),
                _ => log::warn!(target: LOG_TARGET, "{}: {item:?} has no place among the outputs; not saved", self.run.key),
            }
            return Ok(());
        };
        // A new output: what its sidecar recorded about the one it replaces (a reference offset,
        // where an R2′ came from) no longer holds, and the step records it afresh. Returned to the
        // acquired grid, it is the same output.
        if self.step.as_deref() != Some("output_space") {
            if let Some(sidecar) = crate::bids::entities::sidecar_path(&path).filter(|p| p.is_file()) {
                fs_err::remove_file(sidecar)?;
            }
        }
        // Float32, as published, except the intermediates later steps must read as computed.
        let write = if item.kept_in_double() { write_volume_f64 } else { write_volume };
        match data {
            Data::Real(v) => {
                let m = self.meta();
                write(&path, v, m.dims, m.voxel_size, &m.affine)?
            }
            Data::Mask(m) => save_mask(&path, m, self.meta())?,
            Data::OnGrid(v, g) => write(&path, v, g.dims, g.voxel_size, &g.affine)?,
            Data::Text(t) => {
                if let Some(parent) = path.parent() {
                    fs_err::create_dir_all(parent)?;
                }
                fs_err::write(&path, t)?;
            }
            Data::Json(j) => {
                let json = serde_json::to_string_pretty(j)
                    .map_err(|e| QsmxtError::Config(format!("{}: {e}", path.display())))?;
                fs_err::write(&path, json)?;
            }
            Data::Same(source) => {
                let source = self.existing(&source)?;
                if let Some(parent) = path.parent() {
                    fs_err::create_dir_all(parent)?;
                }
                copy_nifti(&source, &path)?;
            }
        }
        if let (Item::RegionStats, Data::Text(table)) = (item, data) {
            log::info!(target: LOG_TARGET, "Per-structure statistics -> {}", path.display());
            self.draw_figures(table);
        }
        Ok(())
    }

    /// Merged into the sidecar's other fields: BIDS-Prov adds `GeneratedBy` to the same file.
    fn annotate(&mut self, item: &Item, fields: serde_json::Map<String, serde_json::Value>) -> crate::Result<()> {
        let Some(sidecar) = self.path(item).and_then(|p| crate::bids::entities::sidecar_path(&p)) else { return Ok(()) };
        let mut obj = read_json_object(&sidecar);
        obj.extend(fields);
        if let Some(parent) = sidecar.parent() {
            fs_err::create_dir_all(parent)?;
        }
        fs_err::write(&sidecar, serde_json::to_string_pretty(&obj)
            .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
        Ok(())
    }

    fn report(&mut self, item: &Item) -> Option<serde_json::Value> {
        if *item == Item::SegmentationVolumes {
            return self.state.completed_steps.get("segmentation")?.metadata.as_ref()?.get("volumes").cloned();
        }
        let text = fs_err::read_to_string(self.path(item)?).ok()?;
        Some(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
    }

    /// The item's JSON sidecar.
    fn annotation(&mut self, item: &Item) -> serde_json::Map<String, serde_json::Value> {
        self.path(item).and_then(|p| crate::bids::entities::sidecar_path(&p))
            .map(|sidecar| read_json_object(&sidecar)).unwrap_or_default()
    }

    fn discard(&mut self, step: &str, outputs: &[Item]) -> crate::Result<()> {
        for path in outputs.iter().filter_map(|i| self.path(i)) {
            if path.exists() {
                fs_err::remove_file(&path)?;
            }
        }
        if self.state.completed_steps.contains_key(step) {
            log::info!(target: LOG_TARGET, "{step} is off now: re-running the stages that read its output");
            self.state.invalidate(step);
            self.state.save(&self.state_path)?;
        }
        Ok(())
    }

    /// Each output left on the working grid gets a `SpatialReference`: the BIDS sidecar field a
    /// `space-` label that is not a standard template needs, pointing at the run's combined
    /// magnitude in that space, or its brain mask when the magnitude is not published.
    fn keep_working_grid(&mut self) -> crate::Result<()> {
        let Some(space) = self.output.space.as_deref() else { return Ok(()) };
        let reference = if self.config.pipeline.do_magnitude {
            crate::pipeline::multiorient::spatial_reference(&self.output, &self.run.key, space)
        } else {
            let mask = self.output.in_space(Some(space)).mask_path(&self.run.key);
            mask.strip_prefix(&self.output.output_dir).unwrap_or(&mask).display().to_string()
        };
        let files: Vec<PathBuf> = self.output.run_published_niftis(&self.run.key).into_iter()
            .filter(|p| p.file_name().and_then(|n| crate::bids::naming::entity_of(&n.to_string_lossy(), "space"))
                .as_deref() == Some(space))
            .collect();
        for f in files {
            let Some(sidecar) = crate::bids::entities::sidecar_path(&f) else { continue };
            let mut obj = read_json_object(&sidecar);
            obj.insert("SpatialReference".into(), serde_json::Value::String(reference.clone()));
            fs_err::write(&sidecar, serde_json::to_string_pretty(&serde_json::Value::Object(obj))
                .map_err(|e| QsmxtError::Config(format!("{}: {e}", sidecar.display())))?)?;
        }
        Ok(())
    }

    fn record(&mut self, c: &Completed<'_>) -> crate::Result<()> {
        let mut inputs: Vec<PathBuf> = c.inputs.iter().filter_map(|i| self.path(i)).collect();
        let mut outputs: Vec<PathBuf> = c.outputs.iter().filter_map(|i| self.path(i)).collect();
        if c.step == "output_space" {
            if !self.working_grid_kept {
                self.keep_working_grid_copies()?;
            }
            // Read: the working-grid originals each output was returned from.
            outputs.sort();
            let stash = self.output.working_grid_dir(&self.run.key);
            inputs = outputs.iter().filter_map(|f| f.file_name().map(|n| stash.join(n))).collect();
        }
        outputs.append(&mut self.extra_outputs);
        let input_refs: Vec<&Path> = inputs.iter().map(|p| p.as_path()).collect();
        let start = self.started.take().unwrap_or_else(Instant::now);
        // What the step found in the data is recorded next to its parameters; only the parameters
        // key the cache, since the outcome is not known when the cache is checked.
        let mut recorded = c.params.clone();
        if let Some((name, value)) = &c.outcome {
            recorded[*name] = value.clone();
        }
        let prov_path = self.output.provenance_path(&self.run.key, c.step);
        write_step_provenance(&prov_path, c.step, c.algorithm, &recorded, &input_refs, &outputs, start)?;
        // Another step whose provenance names one of these files as its output no longer made
        // what is there (its record is forgotten below, or already was): it would be named their
        // maker.
        for &other in crate::bids::bidsprov::STEPS.iter().filter(|&&s| s != c.step && s != "output_space") {
            let stale = self.output.provenance_path(&self.run.key, other);
            let Ok(text) = fs_err::read_to_string(&stale) else { continue };
            let theirs: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let theirs = theirs["outputs"].as_array().cloned().unwrap_or_default();
            if theirs.iter().filter_map(|p| p.as_str()).any(|p| outputs.iter().any(|o| o.display().to_string() == p)) {
                fs_err::remove_file(stale)?;
            }
        }

        // Whatever read this step's previous output is stale now that it has been recomputed with
        // other parameters, or for the first time (a step newly switched on, say), which the
        // downstream steps' own parameter hashes would not notice. Recomputed with the same
        // parameters — its files were missing, or cleaned — it made the same output, and what
        // read it stays cached.
        // What read it is what recorded reading one of the files it wrote before or writes now.
        match self.state.completed_steps.get(c.step) {
            Some(r) if r.params_hash.as_deref() == Some(c.key) => {}
            before => {
                let mut written = outputs.clone();
                written.extend(before.into_iter().flat_map(|r| r.outputs.iter().cloned()));
                for ds in self.state.readers(c.step, &written) {
                    self.state.completed_steps.remove(&ds);
                }
            }
        }
        self.state.mark_completed_with_metadata(c.step, outputs, Some(c.key.to_string()), self.metadata.take());
        self.state.set_inputs(c.step, inputs);
        self.state.set_decisions(c.step, c.decisions);
        self.state.save(&self.state_path)
    }

    fn finish(&mut self) -> crate::Result<()> {
        self.state.mark_run_complete(self.config);
        self.state.save(&self.state_path)
    }
}
