use std::path::{Path, PathBuf};

use crate::bids::entities::AcquisitionKey;
use crate::bids::naming::{self, Tail};

/// Manages BIDS derivative output paths.
///
/// Final outputs (QSM, mask, magnitude, SWI, T2*, R2*) go to `output_dir/sub-XX/anat/`.
/// Intermediates go to `output_dir/workflow/sub-XX/anat/<step>/`.
/// Each step's workflow folder also contains a `provenance.json`.
///
/// Every file name comes from [`naming`]: a run's basename and the derivative's [`Tail`].
#[derive(Debug, Clone)]
pub struct DerivativeOutputs {
    pub output_dir: PathBuf,
    /// The `space-` label of the published outputs: `Some` for a run left on the axial working
    /// grid it was resampled to (`--output-space working`), `None` on the acquired grid.
    pub space: Option<String>,
}

impl DerivativeOutputs {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_owned(),
            space: None,
        }
    }

    /// The same derivatives, naming published outputs in `space` (see [`Self::space`]).
    pub fn in_space(&self, space: Option<&str>) -> Self {
        Self { output_dir: self.output_dir.clone(), space: space.map(str::to_string) }
    }

    /// The paths a run (or a multi-orientation group) published under, read from its state file:
    /// the space its last run recorded. For the readers that run after the pipeline — provenance,
    /// the reference summary, the multi-orientation members.
    pub fn published(&self, key: &AcquisitionKey) -> Self {
        let space = std::fs::read_to_string(self.state_path(key)).ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("space").and_then(|s| s.as_str()).map(str::to_string));
        self.in_space(space.as_deref())
    }

    /// Build the subject/session anat directory for final outputs.
    pub fn anat_dir(&self, key: &AcquisitionKey) -> PathBuf {
        let mut dir = self.output_dir.join(format!("sub-{}", key.subject));
        if let Some(ref ses) = key.session {
            dir = dir.join(format!("ses-{}", ses));
        }
        dir.join("anat")
    }

    /// Build the subject/session extra_files directory for auxiliary outputs.
    fn extra_files_dir(&self, key: &AcquisitionKey) -> PathBuf {
        let mut dir = self.output_dir.join(format!("sub-{}", key.subject));
        if let Some(ref ses) = key.session {
            dir = dir.join(format!("ses-{}", ses));
        }
        dir.join("extra_files")
    }

    /// Directory holding an exported DICOM series for the given map suffix,
    /// e.g. `sub-01/extra_files/sub-01_Chimap_dicoms/`.
    pub fn dicom_dir(&self, key: &AcquisitionKey, suffix: &str) -> PathBuf {
        self.extra_files_dir(key)
            .join(format!("{}_{}_dicoms", key.basename(), suffix))
    }

    /// Build the per-run workflow directory.
    ///
    /// Structure: `workflow/sub-{subject}/[ses-{session}/][acq-{acq}[_run-{run}]/]`
    fn workflow_run_dir(&self, key: &AcquisitionKey) -> PathBuf {
        let mut dir = self.output_dir.join("workflow").join(format!("sub-{}", key.subject));
        if let Some(ref ses) = key.session {
            dir = dir.join(format!("ses-{}", ses));
        }
        // Build a combined directory name from remaining entities
        let mut parts = Vec::new();
        if let Some(ref acq) = key.acquisition {
            parts.push(format!("acq-{}", acq));
        }
        if let Some(ref rec) = key.reconstruction {
            parts.push(format!("rec-{}", rec));
        }
        if let Some(ref inv) = key.inversion {
            parts.push(format!("inv-{}", inv));
        }
        if let Some(ref run) = key.run {
            parts.push(format!("run-{}", run));
        }
        if !parts.is_empty() {
            dir = dir.join(parts.join("_"));
        }
        dir
    }

    /// The run's `fmap/` directory, beside `anat/`: where the total field map and its magnitude go
    /// (BIDS defines `fieldmap` only under the `fmap` datatype).
    pub fn fmap_dir(&self, key: &AcquisitionKey) -> PathBuf {
        self.anat_dir(key).with_file_name("fmap")
    }

    /// Every directory a run publishes into: `anat/` and `fmap/`.
    pub fn published_dirs(&self, key: &AcquisitionKey) -> [PathBuf; 2] {
        [self.anat_dir(key), self.fmap_dir(key)]
    }

    /// The directory a published file named `file_name` belongs in: `fmap/` for the field map and
    /// its magnitude, `anat/` for everything else. For putting a working-grid copy back.
    pub fn published_dir_for(&self, key: &AcquisitionKey, file_name: &str) -> PathBuf {
        let stem = file_name.strip_suffix(".gz").unwrap_or(file_name);
        let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
        let suffix = stem.rsplit('_').next().unwrap_or("");
        if naming::FMAP_SUFFIXES.contains(&suffix) { self.fmap_dir(key) } else { self.anat_dir(key) }
    }

    /// This run's published NIfTIs, across [`Self::published_dirs`], sorted.
    pub fn run_published_niftis(&self, key: &AcquisitionKey) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.published_dirs(key).iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flat_map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())))
            .filter(|p| p.extension().is_some_and(|e| e == "nii"))
            .filter(|p| self.is_run_output(key, p))
            .collect();
        files.sort();
        files
    }

    /// Where a run resampled to axial keeps the working-grid copies of its final outputs.
    ///
    /// Returning the outputs to the acquired grid overwrites them in `anat/`, but later steps
    /// read them back on the working grid (the mask, the combined magnitude, ...), so a re-run
    /// restores them from here first.
    pub fn working_grid_dir(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_run_dir(key).join("working-grid")
    }

    /// Whether `path` is one of this run's final outputs rather than another run's.
    ///
    /// Every run of a subject/session writes into the same `anat/` directory, and one run's
    /// basename can be a prefix of another's (`acq-a` and `acq-a_run-2`), so the file's run
    /// entities are compared one by one rather than its name by prefix.
    pub fn is_run_output(&self, key: &AcquisitionKey, path: &Path) -> bool {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { return false };
        // `fmap/` files carry the run's `rec-`/`inv-` folded into `acq-` (see `naming::fmap_key`).
        if path.parent().and_then(|p| p.file_name()).is_some_and(|d| d == "fmap") {
            let fk = naming::fmap_key(key);
            if fk != *key {
                return self.is_run_output(&fk, path);
            }
        }
        let of = |e: &str| naming::entity_of(&name, e);
        of("sub").as_deref() == Some(key.subject.as_str())
            && of("ses") == key.session
            && of("acq") == key.acquisition
            && (of("rec") == key.reconstruction
                // The combined echoes of a per-coil run replace its `rec-uncombined`.
                || (of("rec").as_deref() == Some("mcpc3ds") && key.reconstruction.as_deref() == Some("uncombined")))
            && of("inv") == key.inversion
            && of("run") == key.run
    }

    /// Build the workflow step directory for a given step.
    fn workflow_step_dir(&self, key: &AcquisitionKey, step: &str) -> PathBuf {
        self.workflow_run_dir(key).join(step)
    }

    /// A published NIfTI of this run: in `anat/` (`fmap/` for a field map), in this run's space.
    fn published_nifti(&self, key: &AcquisitionKey, tail: Tail) -> PathBuf {
        let tail = tail.in_space(self.space.as_deref());
        if naming::FMAP_SUFFIXES.contains(&tail.suffix()) {
            let name = format!("{}.nii", naming::compose(&naming::fmap_key(key).basename(), &tail));
            return self.fmap_dir(key).join(name);
        }
        self.anat_dir(key).join(format!("{}.nii", naming::compose(&key.basename(), &tail)))
    }

    /// A published non-NIfTI file of this run, named like its NIfTI (`.tsv`, `.json`).
    fn published_file(&self, key: &AcquisitionKey, tail: Tail, ext: &str) -> PathBuf {
        let tail = tail.in_space(self.space.as_deref());
        self.anat_dir(key).join(format!("{}.{ext}", naming::compose(&key.basename(), &tail)))
    }

    /// Build a NIfTI output path in a workflow step directory.
    fn workflow_nifti_path(&self, key: &AcquisitionKey, step: &str, tail: Tail) -> PathBuf {
        self.workflow_step_dir(key, step).join(format!("{}.nii", naming::compose(&key.basename(), &tail)))
    }

    /// Path to provenance.json for a given step.
    pub fn provenance_path(&self, key: &AcquisitionKey, step: &str) -> PathBuf {
        self.workflow_step_dir(key, step).join("provenance.json")
    }

    // Final outputs
    pub fn qsm_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::chimap()) }
    pub fn mask_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::mask()) }
    /// Combined magnitude image.
    ///
    /// Named as a `part-mag` T2*-weighted anatomical (`_part-mag_T2starw`)
    /// rather than the BIDS `magnitude` suffix, which is reserved for fieldmap
    /// schemes that require accompanying Phase/Phase-difference/Fieldmap maps.
    pub fn magnitude_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::magnitude()) }
    pub fn swi_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::swi()) }
    pub fn swi_mip_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::swi_minip()) }
    /// Discrete whole-brain segmentation (`_dseg.nii`) and its BIDS label lookup table.
    ///
    /// BIDS pairs a `dseg` image with a `.tsv` carrying `index` and `name` columns, so the integer
    /// labels in the volume mean something without consulting FreeSurfer's table.
    pub fn dseg_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::dseg()) }
    pub fn dseg_lookup_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_file(key, naming::dseg(), "tsv")
    }
    /// Per-structure statistics over the segmentation, for every quantitative map the run made.
    ///
    /// One table per run rather than one per map: the rows carry the map's name, so a single file
    /// holds χ, χ+/χ−, T2*, R2*, R2 and R2' together and nothing has to be joined to compare them.
    pub fn segmentation_stats_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.anat_dir(key).join(format!("{}_desc-segmentation_stats.tsv", key.basename()))
    }

    /// The run's methods text, in its workflow directory: what was done to it, from its plan and
    /// what it decided as it ran.
    pub fn run_methods_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_run_dir(key).join(format!("{}_methods.md", key.basename()))
    }
    /// What the run did, step by step (`qsmxt_engine::record::RunRecord`, JSON), in its workflow
    /// directory.
    pub fn run_record_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_run_dir(key).join(format!("{}_runrecord.json", key.basename()))
    }
    /// The run's pipeline graph (Mermaid; [`write_graph`] puts the SVG beside it) in its workflow
    /// directory: as planned, or (`after`) as the run took it, written only when that differs from
    /// the plan.
    pub fn run_graph_path(&self, key: &AcquisitionKey, after: bool) -> PathBuf {
        let desc = if after { "_desc-ran" } else { "" };
        self.workflow_run_dir(key).join(format!("{}{desc}_pipeline.mmd", key.basename()))
    }
    /// The dataset's pipeline graph (Mermaid, and the SVG beside it), beside `methods.md`.
    pub fn dataset_graph_path(&self) -> PathBuf {
        self.output_dir.join("pipeline.mmd")
    }
    /// Where the runs that differ from the dataset's methods keep what was done to them.
    pub fn changes_dir(&self) -> PathBuf {
        self.output_dir.join("changes")
    }
    /// A run that differs from the dataset: its methods text, or (`graph`) its pipeline graph as
    /// it ran (Mermaid, and the SVG beside it), in [`Self::changes_dir`].
    pub fn change_path(&self, key: &AcquisitionKey, graph: bool) -> PathBuf {
        let name = if graph { "desc-ran_pipeline.mmd" } else { "methods.md" };
        self.changes_dir().join(format!("{}_{name}", key.basename()))
    }

    /// SMWI and its minIP, per contrast. The `desc-` values match the chi-separation outputs, which
    /// split the same two ways.
    pub fn smwi_path(&self, key: &AcquisitionKey, contrast: &str) -> PathBuf {
        self.published_nifti(key, naming::smwi(contrast))
    }
    pub fn smwi_mip_path(&self, key: &AcquisitionKey, contrast: &str) -> PathBuf {
        self.published_nifti(key, naming::smwi_minip(contrast))
    }
    pub fn t2star_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::t2starmap()) }
    pub fn r2star_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::r2starmap()) }
    /// R2 map (Hz) from the MESE acquisition (EPG fit).
    pub fn r2_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::r2map()) }
    /// R2' = R2* − R2 (Hz), used by the R2'-based chi-separation methods.
    pub fn r2prime_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::r2primemap()) }
    /// The total field in Hz (`fmap/<run>_desc-total_fieldmap`, `Units: "Hz"`), published
    /// whenever the run computed one, and the magnitude BIDS pairs with it
    /// (`fmap/<run>_desc-total_magnitude`, the first echo's).
    pub fn total_field_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::total_fieldmap())
    }
    pub fn total_field_magnitude_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::total_fieldmap_magnitude())
    }
    /// Multi-orientation outputs. The key is the group key — the entities every member of the
    /// orientation set shares, with the one that varied between orientations dropped — so a
    /// COSMOS map made from `acq-dir1/2/3` lands at `sub-01_ses-01_desc-cosmos_Chimap.nii`
    /// rather than claiming to belong to any one orientation.
    pub fn cosmos_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::chimap_desc("cosmos"))
    }
    /// The STI outputs: the 4D tensor, MMS, MSA and the 4D principal eigenvector.
    pub fn sti_tensor_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::sti_tensor()) }
    pub fn sti_mms_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::sti_mms()) }
    pub fn sti_msa_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::sti_msa()) }
    pub fn sti_eigenvector_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::sti_eigenvector())
    }
    /// Sidecar recording the direction table a multi-orientation reconstruction actually used.
    /// Without it the result is not reproducible: the directions are the half of the input
    /// that does not live in the NIfTI files. It is the sidecar of `desc-<desc>_Chimap` — the
    /// COSMOS map, or the STI tensor.
    pub fn multiorient_sidecar_path(&self, key: &AcquisitionKey, desc: &str) -> PathBuf {
        self.published_file(key, naming::chimap_desc(desc), "json")
    }

    /// Chi-separation outputs, using the BIDS `desc-` entity on the `Chimap` suffix.
    pub fn chi_para_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::chi_paramagnetic()) }
    pub fn chi_dia_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::chi_diamagnetic()) }
    pub fn chi_sep_total_path(&self, key: &AcquisitionKey) -> PathBuf { self.published_nifti(key, naming::chi_total()) }

    /// The conventional single-pass map, kept alongside the combined one when two-pass runs.
    ///
    /// Named `_desc-singlepass_Chimap` as v8 named it, so scripts that already read it keep working.
    pub fn singlepass_qsm_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::chimap_desc("singlepass"))
    }
    /// The reliable-phase mask of a two-pass run — worth keeping, since it is what decides which
    /// pass won where.
    pub fn two_pass_mask_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::mask_desc("reliable"))
    }

    /// Where the final susceptibility map is defined: the brain mask less the rim that
    /// background-field removal (or the inversion itself) eroded, and the union of both passes'
    /// supports for a two-pass map. The `Chimap` is referenced over exactly these voxels and is 0
    /// outside them, so a 0 in the map outside this mask means "not reconstructed", not χ = 0.
    pub fn qsm_support_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::mask_desc("qsm"))
    }

    /// Where a measured R2′ (R2* − R2) is a measurement: the brain mask ∩ the MESE's field of
    /// view ∩ R2 > 0. The `R2primemap` is 0 outside it because nothing was measured there.
    pub fn r2prime_coverage_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::mask_desc("r2prime"))
    }

    // Intermediate outputs (in workflow step directories)
    /// The total field in ppm, the unit the inversions take.
    pub fn field_ppm_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "unwrap", naming::total_fieldmap_ppm())
    }
    pub fn local_field_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "bgremove", Tail::new("localfield")) }
    pub fn bg_mask_path(&self, key: &AcquisitionKey) -> PathBuf { self.workflow_nifti_path(key, "bgremove", Tail::new("bgmask")) }
    pub fn chi_raw_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "invert", naming::unreferenced_chimap())
    }
    /// The reliable pass's intermediates, in step directories of their own so the two passes
    /// cannot overwrite each other's background-removal or inversion outputs.
    pub fn reliable_local_field_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "bgremove-reliable", Tail::new("localfield"))
    }
    pub fn reliable_bg_mask_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "bgremove-reliable", Tail::new("bgmask"))
    }
    pub fn reliable_chi_raw_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "invert-reliable", naming::unreferenced_chimap())
    }
    /// One of QSMART's intermediates ([`naming::qsmart_intermediate`]), in the workflow directory
    /// of its step: `qsmart`, or `qsmart-reliable` for the reliable pass of a two-pass run.
    pub fn qsmart_intermediate_path(&self, key: &AcquisitionKey, step: &str, name: &str) -> Option<PathBuf> {
        naming::qsmart_intermediate(name).map(|tail| self.workflow_nifti_path(key, step, tail))
    }
    /// The combined (two-pass) raw map, before referencing.
    pub fn two_pass_chi_raw_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "twopass", naming::unreferenced_chimap())
    }

    // Per-echo intermediates (in scale_phase step directory)
    /// MCPC-3D-S combined phase of one echo, exported next to the other derivatives
    /// (`<basename>_rec-mcpc3ds_echo-N_part-phase_MEGRE.nii`; `rec-mcpc3ds` replaces the run's
    /// `rec-uncombined`).
    pub fn combined_phase_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.published_nifti(key, naming::combined_echo(echo, "phase", &key.suffix))
    }
    /// MCPC-3D-S combined magnitude of one echo (see [`Self::combined_phase_path`]).
    pub fn combined_mag_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.published_nifti(key, naming::combined_echo(echo, "mag", &key.suffix))
    }
    /// Robust mask used for the MCPC-3D-S phase-offset estimation (workflow intermediate).
    pub fn combine_mask_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_nifti_path(key, "scale_phase", naming::mcpc3ds_mask())
    }

    pub fn phase_scaled_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_nifti_path(key, "scale_phase", naming::scaled_phase_echo(echo, &key.suffix))
    }
    pub fn mag_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_nifti_path(key, "scale_phase", naming::magnitude_echo(echo, &key.suffix))
    }
    /// Motion-corrected echoes, the mask every echo covers, and the motion report.
    pub fn motion_phase_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_nifti_path(key, "motion", naming::scaled_phase_echo(echo, &key.suffix))
    }
    pub fn motion_mag_path(&self, key: &AcquisitionKey, echo: usize) -> PathBuf {
        self.workflow_nifti_path(key, "motion", naming::magnitude_echo(echo, &key.suffix))
    }
    /// The brain mask less the voxels some echo no longer covers once motion correction has moved
    /// it — what every stage after motion correction is confined to. Published, so the shrink in
    /// the susceptibility map's support can be traced to it.
    pub fn motion_coverage_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.published_nifti(key, naming::mask_desc("motioncoverage"))
    }
    pub fn motion_report_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_step_dir(key, "motion").join(format!("{}_motion.json", key.basename()))
    }

    // Pipeline state (per-run, in workflow directory)
    pub fn state_path(&self, key: &AcquisitionKey) -> PathBuf {
        self.workflow_run_dir(key).join(".pipeline_state.json")
    }
}

/// `graph` as Mermaid at `path` (`*.mmd`) and as SVG beside it (`*.svg`).
pub fn write_graph(path: &Path, graph: &qsmxt_engine::pipeline_graph::PipelineGraph) -> std::io::Result<()> {
    std::fs::write(path, graph.to_mermaid())?;
    std::fs::write(path.with_extension("svg"), graph.to_svg())
}

/// Remove the graph [`write_graph`] wrote at `path`, if it is there.
pub fn remove_graph(path: &Path) -> std::io::Result<()> {
    for p in [path.to_path_buf(), path.with_extension("svg")] {
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_no_session() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".to_string(),
            session: None,
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        }
    }

    #[test]
    fn combined_echo_paths_replace_uncombined_with_mcpc3ds() {
        let out = DerivativeOutputs::new(Path::new("/o"));
        let key = AcquisitionKey { reconstruction: Some("uncombined".into()), ..key_no_session() };
        let p = out.combined_phase_path(&key, 2);
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, "sub-01_rec-mcpc3ds_echo-2_part-phase_MEGRE.nii");
        assert!(!name.contains("uncombined"));
        let m = out.combined_mag_path(&key, 1);
        assert!(m.file_name().unwrap().to_string_lossy().ends_with("_rec-mcpc3ds_echo-1_part-mag_MEGRE.nii"));
    }

    fn key_with_session() -> AcquisitionKey {
        AcquisitionKey {
            subject: "01".to_string(),
            session: Some("pre".to_string()),
            acquisition: None,
            reconstruction: None,
            inversion: None,
            run: None,
            suffix: "MEGRE".to_string(),
        }
    }

    fn output() -> DerivativeOutputs {
        DerivativeOutputs::new(Path::new("/out"))
    }

    // --- Final output paths (not under workflow/) ---

    #[test]
    fn test_qsm_path_no_session() {
        let path = output().qsm_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_Chimap.nii"));
    }

    #[test]
    fn test_qsm_path_with_session() {
        let path = output().qsm_path(&key_with_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/ses-pre/anat/sub-01_ses-pre_Chimap.nii"));
    }

    #[test]
    fn test_mask_path() {
        let path = output().mask_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_mask.nii"));
    }

    #[test]
    fn test_qsm_support_path() {
        let path = output().qsm_support_path(&key_with_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/ses-pre/anat/sub-01_ses-pre_desc-qsm_mask.nii"));
    }

    #[test]
    fn test_magnitude_path() {
        let path = output().magnitude_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/sub-01/anat/sub-01_part-mag_T2starw.nii"));
    }

    #[test]
    fn test_swi_and_mip_paths() {
        let o = output();
        let key = key_no_session();
        assert!(!o.swi_path(&key).to_str().unwrap().contains("workflow"));
        assert!(!o.swi_mip_path(&key).to_str().unwrap().contains("workflow"));
    }

    #[test]
    fn test_t2star_and_r2star_paths() {
        let o = output();
        let key = key_no_session();
        assert!(!o.t2star_path(&key).to_str().unwrap().contains("workflow"));
        assert!(!o.r2star_path(&key).to_str().unwrap().contains("workflow"));
    }

    // --- Workflow step paths ---

    #[test]
    fn test_dicom_dir_no_session() {
        let path = output().dicom_dir(&key_no_session(), "Chimap");
        assert_eq!(path, PathBuf::from("/out/sub-01/extra_files/sub-01_Chimap_dicoms"));
    }

    #[test]
    fn test_dicom_dir_with_session() {
        let path = output().dicom_dir(&key_with_session(), "swi");
        assert_eq!(
            path,
            PathBuf::from("/out/sub-01/ses-pre/extra_files/sub-01_ses-pre_swi_dicoms")
        );
    }

    #[test]
    fn test_field_ppm_path() {
        let path = output().field_ppm_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/unwrap/sub-01_desc-totalppm_fieldmap.nii"));
    }

    #[test]
    fn test_local_field_and_bgmask_paths() {
        let o = output();
        let key = key_no_session();
        assert_eq!(o.local_field_path(&key), PathBuf::from("/out/workflow/sub-01/bgremove/sub-01_localfield.nii"));
        assert_eq!(o.bg_mask_path(&key), PathBuf::from("/out/workflow/sub-01/bgremove/sub-01_bgmask.nii"));
    }

    #[test]
    fn test_chi_raw_path() {
        let path = output().chi_raw_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/invert/sub-01_desc-unreferenced_Chimap.nii"));
    }

    #[test]
    fn test_phase_scaled_path() {
        let path = output().phase_scaled_path(&key_no_session(), 2);
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/scale_phase/sub-01_echo-2_part-phase_desc-scaled_MEGRE.nii"));
    }

    #[test]
    fn test_mag_path() {
        let path = output().mag_path(&key_no_session(), 1);
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/scale_phase/sub-01_echo-1_part-mag_MEGRE.nii"));
    }

    #[test]
    fn test_state_path() {
        let path = output().state_path(&key_no_session());
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/.pipeline_state.json"));
    }

    #[test]
    fn test_provenance_path() {
        let path = output().provenance_path(&key_no_session(), "bgremove");
        assert_eq!(path, PathBuf::from("/out/workflow/sub-01/bgremove/provenance.json"));
    }

    // --- Session paths ---

    #[test]
    fn test_workflow_path_with_session() {
        let o = output();
        let key = key_with_session();
        assert_eq!(
            o.field_ppm_path(&key),
            PathBuf::from("/out/workflow/sub-01/ses-pre/unwrap/sub-01_ses-pre_desc-totalppm_fieldmap.nii")
        );
        assert_eq!(
            o.provenance_path(&key, "unwrap"),
            PathBuf::from("/out/workflow/sub-01/ses-pre/unwrap/provenance.json")
        );
    }

    #[test]
    fn test_workflow_run_dir_with_acq_run() {
        let key = AcquisitionKey {
            subject: "05".to_string(),
            session: None,
            acquisition: Some("mygrea".to_string()),
            reconstruction: None,
            inversion: None,
            run: Some("1".to_string()),
            suffix: "MEGRE".to_string(),
        };
        let o = output();
        assert_eq!(
            o.field_ppm_path(&key),
            PathBuf::from("/out/workflow/sub-05/acq-mygrea_run-1/unwrap/sub-05_acq-mygrea_run-1_desc-totalppm_fieldmap.nii")
        );
        assert_eq!(
            o.state_path(&key),
            PathBuf::from("/out/workflow/sub-05/acq-mygrea_run-1/.pipeline_state.json")
        );
    }

    #[test]
    fn test_path_with_all_entities() {
        let key = AcquisitionKey {
            subject: "02".to_string(),
            session: Some("post".to_string()),
            acquisition: Some("gre".to_string()),
            reconstruction: None,
            inversion: None,
            run: Some("1".to_string()),
            suffix: "MEGRE".to_string(),
        };
        let path = output().qsm_path(&key);
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.contains("sub-02"));
        assert!(name.contains("ses-post"));
        assert!(name.contains("acq-gre"));
        assert!(name.contains("run-1"));
        assert!(name.ends_with("_Chimap.nii"));
    }

    #[test]
    fn working_space_labels_published_outputs_only() {
        let o = output().in_space(Some("axial"));
        let k = key_no_session();
        assert_eq!(o.qsm_path(&k), PathBuf::from("/out/sub-01/anat/sub-01_space-axial_Chimap.nii"));
        assert_eq!(o.magnitude_path(&k), PathBuf::from("/out/sub-01/anat/sub-01_part-mag_space-axial_T2starw.nii"));
        assert_eq!(o.qsm_support_path(&k), PathBuf::from("/out/sub-01/anat/sub-01_space-axial_desc-qsm_mask.nii"));
        assert_eq!(o.dseg_lookup_path(&k), PathBuf::from("/out/sub-01/anat/sub-01_space-axial_dseg.tsv"));
        // Intermediates and the state are where they always were.
        assert_eq!(o.field_ppm_path(&k), output().field_ppm_path(&k));
        assert_eq!(o.state_path(&k), output().state_path(&k));
        assert!(o.is_run_output(&k, &o.qsm_path(&k)));
    }

    #[test]
    fn non_bids_rows_of_issue_275_are_renamed() {
        let o = output();
        let k = key_no_session();
        let name = |p: PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name(o.sti_tensor_path(&k)), "sub-01_desc-sti_Chimap.nii");
        assert_eq!(name(o.sti_eigenvector_path(&k)), "sub-01_desc-sti_evec.nii");
        assert_eq!(name(o.sti_mms_path(&k)), "sub-01_desc-mms_Chimap.nii");
        assert_eq!(name(o.multiorient_sidecar_path(&k, "sti")), "sub-01_desc-sti_Chimap.json");
        assert_eq!(name(o.swi_path(&k)), "sub-01_swi.nii");
        assert_eq!(name(o.swi_mip_path(&k)), "sub-01_minIP.nii");
        assert_eq!(name(o.smwi_path(&k, "paramagnetic")), "sub-01_desc-paramagnetic_smwi.nii");
        assert_eq!(name(o.smwi_mip_path(&k, "diamagnetic")), "sub-01_desc-diamagneticsmwi_minIP.nii");
        assert_eq!(o.total_field_path(&k), PathBuf::from("/out/sub-01/fmap/sub-01_desc-total_fieldmap.nii"));
        assert_eq!(o.total_field_magnitude_path(&k), PathBuf::from("/out/sub-01/fmap/sub-01_desc-total_magnitude.nii"));
        assert_eq!(name(o.r2prime_path(&k)), "sub-01_R2primemap.nii");
    }

    /// One run's basename is a prefix of another's (`acq-a` / `acq-a_run-2`); `run-` before `inv-`.
    #[test]
    fn run_outputs_are_told_apart_by_entity() {
        let o = output();
        let a = AcquisitionKey { acquisition: Some("a".into()), ..key_no_session() };
        let a2 = AcquisitionKey { run: Some("2".into()), ..a.clone() };
        assert!(o.is_run_output(&a, &o.qsm_path(&a)));
        assert!(!o.is_run_output(&a, &o.qsm_path(&a2)));
        assert!(!o.is_run_output(&a2, &o.qsm_path(&a)));
        let inv = AcquisitionKey { run: Some("1".into()), inversion: Some("2".into()), ..key_no_session() };
        assert_eq!(o.qsm_path(&inv), PathBuf::from("/out/sub-01/anat/sub-01_run-1_inv-2_Chimap.nii"));
        // The combined echoes of a rec-uncombined run belong to it, and echo- sorts before inv-.
        let unc = AcquisitionKey { reconstruction: Some("uncombined".into()), ..inv.clone() };
        let p = o.combined_phase_path(&unc, 1);
        assert_eq!(p.file_name().unwrap().to_string_lossy(),
                   "sub-01_rec-mcpc3ds_run-1_echo-1_inv-2_part-phase_MEGRE.nii");
        assert!(o.is_run_output(&unc, &p));
        // BIDS allows no `rec-`/`inv-` on a field map, so they fold into `acq-` there.
        let f = o.total_field_path(&unc);
        assert_eq!(f, PathBuf::from("/out/sub-01/fmap/sub-01_acq-recuncombinedinv2_run-1_desc-total_fieldmap.nii"));
        assert!(o.is_run_output(&unc, &f));
        assert!(!o.is_run_output(&inv, &f));
        assert!(o.is_run_output(&inv, &o.total_field_path(&inv)));
    }
}
