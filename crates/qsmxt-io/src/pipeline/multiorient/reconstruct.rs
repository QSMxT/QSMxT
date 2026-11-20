//! Reconstructing one orientation group: the cache, the members put on one grid, the COSMOS or
//! STI solve, and the group's sidecar, statistics and provenance.

use std::path::PathBuf;

use log::{info, warn};

use super::{
    affine_drift, eigenvector_sidecar, file_identity, geometry_matches, group_key, kind_of, load_member,
    member_labels, member_paths, on_working_grid, orientations_for, register_group, registration_reason,
    registration_table, returned_to_acquired, spatial_reference, tensor_sidecar_fields, was_resampled, Geometry,
    MemberData, MemberPaths, Registered, AFFINE_TOLERANCE,
};
use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::bids::entities::AcquisitionKey;
use crate::bids::orientation::OrientationGroup;
use crate::error::QsmxtError;
use crate::multiorient::{
    check_directions, direction_table, resolve_directions_registered, DirectionSource, MultiOrientKind,
    Orientation, OrientationCheck,
};
use qsmxt_engine::PipelineConfig;

/// The log target these messages had before they moved into this module.
const LOG_TARGET: &str = "qsmxt_io::pipeline::multiorient";

/// Reconstruct one orientation group.
///
/// The group's map is a first-class output (#270): its members are read on their working grids,
/// COSMOS χ is referenced with `--qsm-reference` over the voxels every orientation covers, the
/// result goes back to the reference member's acquired grid under `--output-space acquired`,
/// the step is cached (keyed on every member input, the directions and the settings), and, with
/// `--do-analysis`, it gets a per-structure table on the reference member's segmentation.
///
/// Returns the paths written (or, from the cache, the ones written before). Errors are the
/// caller's to report per group — one bad group should not abandon the rest of the dataset.
pub fn reconstruct_group(
    group: &OrientationGroup,
    members: &[&QsmRun],
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    force: bool,
    progress: &dyn Fn(&str),
) -> crate::Result<Vec<PathBuf>> {
    let kind = kind_of(config);
    let desc = match kind {
        MultiOrientKind::Cosmos => "cosmos",
        MultiOrientKind::Sti => "sti",
    };
    let key = group_key(members);
    let started = std::time::Instant::now();

    // ── Cache ──
    let GroupCache { paths, reference_dseg, final_dseg, settings_hash, hash, state_path, mut state } =
        open_cache(members, config, output, &key, desc, force);
    if let Some(cached) = cached_outputs(&mut state, &paths, kind, desc, &settings_hash, &hash, &group.label) {
        return Ok(cached);
    }

    info!(target: LOG_TARGET, "{kind} for {} ({} orientations)", group.label, members.len());
    let Aligned { geometry: (dims, voxel_size, affine), fields, masks, magnitudes, orientations, registered } =
        align_members(group, members, config, &paths, kind, progress)?;

    let (mask, kept) = intersect_masks(&masks);

    info!(target: LOG_TARGET, "  directions:");
    for row in direction_table(&orientations) {
        info!(target: LOG_TARGET, "    {row}");
    }
    // Re-checked on whatever the directions ended up being: if registration found that the
    // orientations really are the same, the group is still refused.
    let check = check_directions(&orientations, kind);
    match (&check.verdict, config.multi_orientation.force) {
        (Ok(()), _) => info!(target: LOG_TARGET, "  {}", check.summary()),
        (Err(why), false) => return Err(QsmxtError::Config(why.clone())),
        (Err(why), true) => warn!(target: LOG_TARGET, "  force: reconstructing anyway, but {why}"),
    }

    let bdirs: Vec<_> = orientations.iter().map(|o| o.b0).collect();
    let grid = qsm_core::Grid::new(dims.0, dims.1, dims.2, voxel_size.0, voxel_size.1, voxel_size.2);
    let mut written = Vec::new();

    let acquired = acquired_grid(config, output, members, dims, &affine);
    // Maps left on the reference member's working grid, when that is not the grid it was
    // acquired on, are named in that space (`space-axial`), like the member's own outputs would be.
    let space = (acquired.is_none() && was_resampled(output, &members[0].key))
        .then_some(crate::bids::naming::WORKING_SPACE);
    let output = &output.in_space(space);
    let r = Recon { group, members, config, output, key: &key, dims, voxel_size, affine, acquired, progress };
    let solve = Solve { fields: &fields, bdirs: &bdirs, mask: &mask, grid: &grid };

    let mut reference_fields = serde_json::Map::new();
    let mut extra_inputs: Vec<PathBuf> = Vec::new();
    let mut magnitudes_used = false;
    match kind {
        MultiOrientKind::Cosmos => {
            (reference_fields, magnitudes_used) =
                write_cosmos(&r, &solve, magnitudes, &reference_dseg, &mut written, &mut extra_inputs)?;
        }
        MultiOrientKind::Sti => write_sti(&r, &solve, &mut written)?,
    }

    let facts = Facts {
        kind, orientations: &orientations, check: &check, kept, magnitudes_used,
        registered: registered.as_deref(), space,
    };
    let sidecar_path = write_group_sidecar(&r, &facts, desc, reference_fields)?;

    // Per-structure statistics on the reference member's segmentation, which is on the grid the
    // COSMOS map was just written to.
    if config.pipeline.do_analysis && kind == MultiOrientKind::Cosmos {
        match group_statistics(output, &key, &final_dseg, config) {
            Ok(Some(p)) => {
                extra_inputs.push(final_dseg.clone());
                written.push(p);
            }
            Ok(None) => warn!(
                target: LOG_TARGET,
                "{}: no segmentation for the reference orientation {} at {}, so the COSMOS map \
                 is not in the statistics", group.label, members[0].key, final_dseg.display()),
            Err(e) => warn!(target: LOG_TARGET, "{}: per-structure statistics of the COSMOS map failed: {e}", group.label),
        }
    }

    write_group_provenance(&r, &facts, desc, &paths, extra_inputs, &written, started)?;
    written.push(sidecar_path);

    state.space = space.map(str::to_string);
    state.mark_completed_with_metadata(
        desc, written.clone(), Some(hash),
        Some(serde_json::json!({ "settings": settings_hash })));
    state.save(&state_path)?;
    Ok(written)
}

/// A group's step state and what its cache is keyed on.
struct GroupCache {
    paths: Vec<MemberPaths>,
    /// The reference member's segmentation on its working grid, and as published.
    reference_dseg: PathBuf,
    final_dseg: PathBuf,
    /// The hash of the settings alone, and of the settings and every input.
    settings_hash: String,
    hash: String,
    state_path: PathBuf,
    state: crate::pipeline::graph::PipelineState,
}

/// The group's state, and the hashes its cached outputs are checked against.
fn open_cache(
    members: &[&QsmRun], config: &PipelineConfig, output: &DerivativeOutputs, key: &AcquisitionKey,
    desc: &str, force: bool,
) -> GroupCache {
    let paths: Vec<MemberPaths> = members.iter().map(|run| member_paths(output, run)).collect();
    let reference_outputs = output.published(&members[0].key);
    let reference_dseg = on_working_grid(output, &members[0].key, reference_outputs.dseg_path(&members[0].key));
    let final_dseg = reference_outputs.dseg_path(&members[0].key);
    let settings = serde_json::json!({
        "kind": desc,
        "multi_orientation": config.multi_orientation,
        "registration": config.registration,
        "reference": config.qsm.reference_spec(),
        "segmentation_version": format!("{}", config.segmentation.version),
        "output_space": format!("{:?}", config.pipeline.output_space),
        "analysis": config.pipeline.do_analysis,
        "b0_dirs": members.iter().map(|m| m.b0_dir).collect::<Vec<_>>(),
        "members": members.iter().map(|m| m.key.to_string()).collect::<Vec<_>>(),
    });
    let mut inputs_id: Vec<serde_json::Value> = Vec::new();
    for p in &paths {
        inputs_id.extend([file_identity(&p.field), file_identity(&p.mask), file_identity(&p.magnitude)]);
    }
    inputs_id.extend([file_identity(&reference_dseg), file_identity(&final_dseg)]);
    let settings_hash = crate::pipeline::graph::step_params_hash(Some(desc), &settings);
    let hash = crate::pipeline::graph::step_params_hash(
        Some(desc), &serde_json::json!({ "settings": settings, "inputs": inputs_id }));
    let state_path = output.state_path(key);
    let state = crate::pipeline::graph::PipelineState::load_or_create(&state_path, config, key, force);
    GroupCache { paths, reference_dseg, final_dseg, settings_hash, hash, state_path, state }
}

/// The outputs of the group's step when they stand: cached with these inputs, or made with these
/// settings from intermediates that have since been cleaned.
fn cached_outputs(
    state: &mut crate::pipeline::graph::PipelineState, paths: &[MemberPaths], kind: MultiOrientKind, desc: &str,
    settings_hash: &str, hash: &str, label: &str,
) -> Option<Vec<PathBuf>> {
    if state.is_step_cached_with_hash(desc, Some(hash)) {
        info!(target: LOG_TARGET, "{kind} for {}: cached", label);
        return Some(state.step_outputs(desc).map(|o| o.to_vec()).unwrap_or_default());
    }
    // `--clean-intermediates` removed the members' local fields after the run that made this map.
    // Nothing can be recomputed from them, and nothing about the group changed, so the map stands.
    let cleaned = paths.iter().all(|p| !p.field.exists());
    if cleaned {
        if let Some(rec) = state.completed_steps.get(desc) {
            let same = rec.metadata.as_ref().and_then(|m| m.get("settings")).and_then(|v| v.as_str())
                == Some(settings_hash);
            if same && rec.outputs.iter().all(|o| o.exists()) {
                info!(target: LOG_TARGET, "{kind} for {}: cached (the members' intermediates were cleaned)", label);
                return Some(rec.outputs.clone());
            }
        }
    }
    None
}

/// The members' fields, masks and magnitudes on one grid, with their directions.
struct Aligned {
    geometry: Geometry,
    fields: Vec<Vec<f64>>,
    masks: Vec<Vec<u8>>,
    magnitudes: Vec<Option<Vec<f64>>>,
    orientations: Vec<Orientation>,
    /// Each member's registration, when the group was registered.
    registered: Option<Vec<Registered>>,
}

/// Load the members and put them on the first member's grid, registering them when they need it
/// and registration is on.
fn align_members(
    group: &OrientationGroup, members: &[&QsmRun], config: &PipelineConfig, paths: &[MemberPaths],
    kind: MultiOrientKind, progress: &dyn Fn(&str),
) -> crate::Result<Aligned> {
    let loaded: Vec<MemberData> = members
        .iter()
        .zip(paths)
        .map(|(run, p)| load_member(p, run))
        .collect::<crate::Result<Vec<_>>>()?;
    // The first member's grid is the common one. Picking a reference rather than inventing a
    // grid means one orientation is never interpolated at all, which is the one to look at when
    // a combination comes out wrong.
    let geometry = loaded[0].geometry;

    // What the group looks like before anything is moved.
    let header_affines: Vec<[f64; 16]> = loaded.iter().map(|m| m.geometry.2).collect();
    let header_orientations = orientations_for(members, &header_affines);
    let header_check = check_directions(&header_orientations, kind);
    let grids_agree = loaded.iter().all(|m| geometry_matches(&m.geometry, &loaded[0].geometry));

    let reason = registration_reason(grids_agree, &header_orientations, &header_check);

    let (fields, masks, magnitudes, orientations, registered) = match (reason, config.multi_orientation.register) {
        (None, _) => {
            info!(target: LOG_TARGET, "  orientations already share a grid and carry usable directions — no registration");
            let fields: Vec<Vec<f64>> = loaded.iter().map(|m| m.field.clone()).collect();
            let masks: Vec<Vec<u8>> = loaded.iter().map(|m| m.mask.clone()).collect();
            let mags: Vec<Option<Vec<f64>>> = loaded.iter().map(|m| m.magnitude.clone()).collect();
            (fields, masks, mags, header_orientations, None)
        }
        (Some(why), true) => {
            info!(target: LOG_TARGET, "  co-registering: {why}");
            let registered = register_group(&loaded, members, &config.registration, progress)?;
            info!(target: LOG_TARGET, "  registration:");
            for row in registration_table(members, &registered) {
                info!(target: LOG_TARGET, "    {row}");
            }
            let labels: Vec<String> = member_labels(members);
            let declared: Vec<Option<(f64, f64, f64)>> = members.iter().map(|m| m.b0_dir).collect();
            let transforms: Vec<_> = registered.iter().map(|r| r.transform.clone()).collect();
            let orientations = resolve_directions_registered(
                &labels,
                &declared,
                &transforms,
                DirectionSource::Sidecar,
            );
            let fields: Vec<Vec<f64>> = registered.iter().map(|r| r.field.clone()).collect();
            let masks: Vec<Vec<u8>> = registered.iter().map(|r| r.mask.clone()).collect();
            let mags: Vec<Option<Vec<f64>>> = registered.iter().map(|r| r.magnitude.clone()).collect();
            (fields, masks, mags, orientations, Some(registered))
        }
        (Some(why), false) => {
            return Err(QsmxtError::DimensionMismatch(format!(
                "{}: {why}, and registration is switched off by --no-orientation-registration. \
                Either drop that flag and let QSMxT co-register the orientations, or \
                co-register them externally onto one grid first",
                group.label
            )));
        }
    };
    Ok(Aligned { geometry, fields, masks, magnitudes, orientations, registered })
}

/// The intersection of the members' masks, and how many voxels it keeps.
fn intersect_masks(masks: &[Vec<u8>]) -> (Vec<u8>, usize) {
    // Every orientation must contribute at every voxel that is reconstructed, so the masks
    // intersect. A voxel one orientation eroded away has no field value there to combine.
    let mut mask = masks[0].clone();
    for m in &masks[1..] {
        for (a, b) in mask.iter_mut().zip(m.iter()) {
            *a &= *b;
        }
    }
    let kept = mask.iter().filter(|&&m| m == 1).count();
    let largest = masks.iter().map(|m| m.iter().filter(|&&v| v == 1).count()).max().unwrap_or(0);
    if largest > 0 && kept * 10 < largest * 7 {
        warn!(
            target: LOG_TARGET,
            "the intersection of the orientation masks keeps {kept} of {largest} voxels \
             ({:.0}%) — that much loss usually means the orientations are not well aligned",
            100.0 * kept as f64 / largest as f64
        );
    }
    (mask, kept)
}

/// The reference member's acquired grid, when the group's outputs go back to it.
fn acquired_grid(
    config: &PipelineConfig, output: &DerivativeOutputs, members: &[&QsmRun], dims: (usize, usize, usize),
    affine: &[f64; 16],
) -> Option<((usize, usize, usize), [f64; 16])> {
    // Where the outputs go: back onto the reference member's acquired grid when its own outputs
    // went there, so the group map lies on the same grid as the reference member's maps and
    // segmentation. `None` = they stay on the working grid.
    (config.pipeline.output_space
        == qsmxt_engine::OutputSpace::Acquired
        && returned_to_acquired(output, &members[0].key))
        .then(|| members[0].echoes.first()
            .and_then(|e| qsm_core::io::read_nifti_file(&e.phase_nifti).ok())
            .map(|n| (n.dims, n.affine)))
        .flatten()
        .filter(|(d, a)| !(*d == dims && affine_drift(a, affine) <= AFFINE_TOLERANCE))
}

/// One group's reconstruction: what it is of, and the grid and place its maps are written to.
struct Recon<'a> {
    group: &'a OrientationGroup,
    members: &'a [&'a QsmRun],
    config: &'a PipelineConfig,
    /// In the space the maps are named in.
    output: &'a DerivativeOutputs,
    key: &'a AcquisitionKey,
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: [f64; 16],
    /// The reference member's acquired grid, when the maps go back to it.
    acquired: Option<((usize, usize, usize), [f64; 16])>,
    progress: &'a dyn Fn(&str),
}

impl Recon<'_> {
    fn write(&self, path: &std::path::Path, data: &[f64]) -> crate::Result<()> {
        crate::nifti::write::write_volume(path, data, self.dims, self.voxel_size, &self.affine)
    }
}

/// What the solvers combine: the fields, their B0 directions, the common mask and grid.
struct Solve<'a> {
    fields: &'a [Vec<f64>],
    bdirs: &'a [(f64, f64, f64)],
    mask: &'a [u8],
    grid: &'a qsm_core::Grid,
}

/// Solve and write the COSMOS map, referenced. Returns its reference sidecar fields and whether
/// the solve was magnitude-weighted.
fn write_cosmos(
    r: &Recon, s: &Solve, magnitudes: Vec<Option<Vec<f64>>>, reference_dseg: &std::path::Path,
    written: &mut Vec<PathBuf>, extra_inputs: &mut Vec<PathBuf>,
) -> crate::Result<(serde_json::Map<String, serde_json::Value>, bool)> {
    let (config, members, key, dims, affine) = (r.config, r.members, r.key, r.dims, &r.affine);
    let mut magnitudes_used = false;
    (r.progress)(&format!("COSMOS ({} orientations)", members.len()));
    let m = &config.multi_orientation;
    let params = qsm_core::inversion::CosmosParams {
        lambda: m.lambda,
        tol: m.cosmos_tol,
        max_iter: m.cosmos_max_iter,
    };
    let raw = if m.magnitude_weighting {
        let weights: Vec<Vec<f64>> = magnitudes.into_iter().zip(members)
            .map(|(w, run)| w.ok_or_else(|| QsmxtError::Config(format!(
                "--cosmos-magnitude-weighting: {} has no combined magnitude to weight by",
                run.key))))
            .collect::<crate::Result<_>>()?;
        magnitudes_used = true;
        info!(target: LOG_TARGET, "  magnitude-weighted COSMOS (conjugate gradient, up to {} iterations)", params.max_iter);
        qsm_core::inversion::cosmos_weighted(s.fields, &weights, s.bdirs, s.mask, s.grid, &params)
    } else {
        qsm_core::inversion::cosmos(s.fields, s.bdirs, s.mask, s.grid, &params)
    };

    // Referenced like a single-orientation map, over the voxels every orientation covers.
    let dseg = if config.qsm.reference == qsmxt_engine::QsmReference::Region {
        let d = qsm_core::io::read_nifti_file(reference_dseg).map_err(|e| QsmxtError::Config(format!(
            "--qsm-reference: the group is referenced on {}'s segmentation, which could not \
             be read at {}: {e}", members[0].key, reference_dseg.display())))?;
        extra_inputs.push(reference_dseg.to_path_buf());
        Some(d.data)
    } else {
        None
    };
    let referenced = qsmxt_engine::referencing::reference_chi(config, &raw, &[s.mask], dseg.as_deref())?;
    let reference_fields = qsmxt_engine::referencing::sidecar_fields(config, referenced.offset.as_ref());

    let path = r.output.cosmos_path(key);
    match r.acquired {
        Some((adims, aaff)) => {
            let out = qsm_core::geometry::resample_onto(&referenced.chi, dims, affine, adims, &aaff)
                .ok_or_else(|| QsmxtError::Config(format!(
                    "{}: cannot return the COSMOS map to {}'s acquired grid",
                    r.group.label, members[0].key)))?;
            let avs = qsm_core::geometry::voxel_sizes_from_affine(&aaff);
            crate::nifti::write::write_volume(&path, &out, adims, avs, &aaff)?;
            info!(target: LOG_TARGET, "  returned to {}'s acquired grid", members[0].key);
        }
        None => r.write(&path, &referenced.chi)?,
    }
    written.push(path);
    Ok((reference_fields, magnitudes_used))
}

/// Solve and write the STI tensor, its derived maps and its eigenvector.
fn write_sti(r: &Recon, s: &Solve, written: &mut Vec<PathBuf>) -> crate::Result<()> {
    let (key, dims, voxel_size, affine) = (r.key, r.dims, r.voxel_size, &r.affine);
    (r.progress)(&format!("STI ({} orientations)", r.members.len()));
    let params = qsm_core::inversion::StiParams { lambda: r.config.multi_orientation.lambda };
    let tensor = qsm_core::inversion::sti(s.fields, s.bdirs, s.mask, s.grid, &params);
    let maps = qsm_core::inversion::tensor_maps(&tensor, s.mask);
    if r.acquired.is_some() {
        warn!(
            target: LOG_TARGET,
            "{}: the STI outputs stay on the working grid. Returning a tensor to the \
             acquired grid needs it rotated as well as resampled, which QSM.rs does not \
             provide; the reference member's other outputs are on its acquired grid.",
            r.group.label
        );
    }
    // One 4D tensor, components in QSM.rs's order [X11, X12, X13, X22, X23, X33]
    // (`naming::STI_COMPONENTS`, recorded in the sidecar), and one 4D eigenvector.
    let components: Vec<&[f64]> = tensor.components.iter().map(|c| c.as_slice()).collect();
    let path = r.output.sti_tensor_path(key);
    crate::nifti::write::write_volumes_4d(&path, &components, dims, voxel_size, affine)?;
    written.push(path);
    for (path, data) in [(r.output.sti_mms_path(key), &maps.mms), (r.output.sti_msa_path(key), &maps.msa)] {
        r.write(&path, data)?;
        written.push(path);
    }
    let pev: Vec<&[f64]> = maps.pev.iter().map(|c| c.as_slice()).collect();
    let path = r.output.sti_eigenvector_path(key);
    crate::nifti::write::write_volumes_4d(&path, &pev, dims, voxel_size, affine)?;
    if let Some(sidecar) = crate::bids::entities::sidecar_path(&path) {
        fs_err::write(&sidecar, serde_json::to_string_pretty(&eigenvector_sidecar())
            .map_err(|e| QsmxtError::Config(format!("STI eigenvector sidecar: {e}")))?)?;
        written.push(sidecar);
    }
    written.push(path);
    Ok(())
}

/// What a reconstruction found and did, for its sidecar and provenance.
struct Facts<'a> {
    kind: MultiOrientKind,
    orientations: &'a [Orientation],
    check: &'a OrientationCheck,
    /// Voxels in the intersected mask.
    kept: usize,
    magnitudes_used: bool,
    registered: Option<&'a [Registered]>,
    /// The space the maps are named in, when they stay on a resampled working grid.
    space: Option<&'static str>,
}

/// Write the group's sidecar; returns its path.
fn write_group_sidecar(
    r: &Recon, f: &Facts, desc: &str, reference_fields: serde_json::Map<String, serde_json::Value>,
) -> crate::Result<PathBuf> {
    let (members, kind, orientations, check) = (r.members, f.kind, f.orientations, f.check);
    // The direction table is the half of the input that does not live in the NIfTI files, so
    // without it the reconstruction cannot be reproduced or audited.
    let mut sidecar = serde_json::json!({
        "Description": format!("{kind} reconstruction over {} orientations", members.len()),
        "Sources": members.iter().map(|m| m.key.to_string()).collect::<Vec<_>>(),
        "B0_dirs": orientations.iter().map(|o| vec![o.b0.0, o.b0.1, o.b0.2]).collect::<Vec<_>>(),
        "B0_dir_sources": orientations.iter().map(|o| o.source.to_string()).collect::<Vec<_>>(),
        "Lambda": r.config.multi_orientation.lambda,
        "MagnitudeWeighting": f.magnitudes_used,
        "MaxPairwiseAngleDeg": check.max_pairwise_deg,
        "MinPairwiseAngleDeg": check.min_pairwise_deg,
        "DirectionRank": check.rank,
        "MaskVoxels": f.kept,
        "Space": if r.acquired.is_some() { "acquired" } else { "working" },
        // The transforms are the other half of the input that does not live in the NIfTI files.
        // Without them a registered reconstruction cannot be re-derived or audited, and the
        // rotation is also the provenance of the B0 direction beside it.
        "Registration": f.registered.map(|regs| {
            regs.iter().enumerate().map(|(t, r)| serde_json::json!({
                "Source": members[t].key.to_string(),
                "Reference": t == 0,
                "RotationDeg": r.transform.rotation_magnitude_deg(),
                "RotationEulerDeg": r.transform.rotation_degrees().to_vec(),
                "TranslationMm": r.transform.translation_mm().to_vec(),
                "Ncc": r.transform.ncc,
                "Overlap": r.transform.overlap,
                "Matrix": r.transform.matrix.to_vec(),
            })).collect::<Vec<_>>()
        }),
        "ReferenceOrientation": members[0].key.to_string(),
    });
    if let Some(obj) = sidecar.as_object_mut() {
        obj.extend(reference_fields);
        if kind == MultiOrientKind::Sti {
            obj.extend(tensor_sidecar_fields());
        }
        if let Some(space) = f.space {
            obj.insert("SpatialReference".into(), serde_json::json!(
                spatial_reference(r.output, &members[0].key, space)));
        }
    }

    let sidecar_path = r.output.multiorient_sidecar_path(r.key, desc);
    if let Some(parent) = sidecar_path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| QsmxtError::Config(format!("multi-orientation sidecar: {e}")))?;
    fs_err::write(&sidecar_path, text)?;
    Ok(sidecar_path)
}

/// The group's step provenance: what was combined, how.
fn write_group_provenance(
    r: &Recon, f: &Facts, desc: &str, paths: &[MemberPaths], extra_inputs: Vec<PathBuf>, written: &[PathBuf],
    started: std::time::Instant,
) -> crate::Result<()> {
    let config = r.config;
    // Provenance, keyed by the group (see `bidsprov::GROUP_STEPS`): what was combined, how.
    let mut inputs: Vec<PathBuf> = Vec::new();
    for p in paths {
        inputs.push(p.field.clone());
        inputs.push(p.mask.clone());
        if f.registered.is_some() || f.magnitudes_used {
            inputs.push(p.magnitude.clone());
        }
    }
    inputs.extend(extra_inputs);
    let input_refs: Vec<&std::path::Path> = inputs.iter().map(|p| p.as_path()).collect();
    crate::pipeline::runner::write_step_provenance(
        &r.output.provenance_path(r.key, desc),
        desc,
        Some(desc),
        &serde_json::json!({
            "lambda": config.multi_orientation.lambda,
            "magnitude_weighting": f.magnitudes_used,
            "cosmos_tol": config.multi_orientation.cosmos_tol,
            "cosmos_max_iter": config.multi_orientation.cosmos_max_iter,
            "orientations": r.members.len(),
            "registered": f.registered.is_some(),
            "force": config.multi_orientation.force,
            "group_by": config.multi_orientation.group_by,
            "reference": config.qsm.reference_spec(),
            "space": if r.acquired.is_some() { "acquired" } else { "working" },
        }),
        &input_refs,
        written,
        started,
    )?;
    Ok(())
}

/// The COSMOS map's per-structure table, on the reference member's segmentation. `None` when
/// there is no segmentation to summarise over.
fn group_statistics(
    output: &DerivativeOutputs, key: &AcquisitionKey, dseg_path: &std::path::Path,
    config: &PipelineConfig,
) -> crate::Result<Option<PathBuf>> {
    use crate::pipeline::stats;
    if !dseg_path.exists() {
        return Ok(None);
    }
    let seg = qsm_core::io::read_nifti_file(dseg_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {e}", dseg_path.display())))?.data;
    let spec = stats::MapSpec { name: stats::COSMOS_CHIMAP, unit: "ppm", path: output.cosmos_path(key) };
    let map = qsm_core::io::read_nifti_file(&spec.path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {e}", spec.path.display())))?.data;
    if map.len() != seg.len() {
        return Err(QsmxtError::DimensionMismatch(format!(
            "segmentation has {} voxels but the COSMOS map has {}", seg.len(), map.len())));
    }
    let labels = config.segmentation.version.to_core().labels();
    let reference = crate::bids::entities::sidecar_path(&spec.path)
        .as_deref()
        .and_then(stats::reference_from_sidecar);
    let mut table = String::from(stats::HEADER);
    table.push_str(&stats::map_rows(spec.name, spec.unit, &map, &seg, labels, None, reference.as_ref()));
    let out = output.segmentation_stats_path(key);
    fs_err::write(&out, table)?;
    info!(target: LOG_TARGET, "  per-structure statistics -> {}", out.display());
    Ok(Some(out))
}
