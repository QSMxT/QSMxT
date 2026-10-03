//! Combining an orientation group into one COSMOS map or one susceptibility tensor.
//!
//! # Where this sits in the pipeline
//!
//! The fan-in happens **after background removal and before inversion**. Background removal
//! wants each orientation in its own native space: the mask is native, there is no
//! interpolation blur yet, and PDF needs that acquisition's own B0 direction rather than the
//! reference frame's. So every member run goes through the ordinary pipeline, which leaves a
//! local field and a background-removal mask on disk, and this module picks those up.
//!
//! Members therefore also produce their own single-orientation χ maps. That is deliberate
//! rather than incidental: the first thing to check when a COSMOS map looks wrong is whether
//! the orientations were actually aligned, and per-orientation maps are how you see that.
//!
//! # Getting the orientations onto one grid
//!
//! COSMOS and STI need the orientations on one common grid, and this module now produces that
//! rather than only demanding it. The first member's grid is the common one, and every other
//! member is rigidly co-registered onto it
//! ([`qsm_core::registration::register_rigid`]) before its local field is resampled across.
//!
//! Two deliberate choices in that sentence:
//!
//! - **Registration is driven by magnitude, never by the field.** The local field is not an
//!   anatomical image: its contrast is the dipole response, which *changes with orientation* by
//!   construction, so correlating two orientations' fields would be matching the very thing the
//!   reconstruction is trying to measure. The magnitude is the orientation-invariant picture of
//!   the anatomy, so a group with no magnitude cannot be registered here.
//! - **Registration runs only when the group needs it.** Already on one grid with declared
//!   `B0_dir`s, there is nothing to find and an interpolation would only blur; the group is
//!   passed through untouched. It runs when the members disagree on dimensions or affine, or
//!   when the only directions available came from affines that cannot tell the orientations
//!   apart — the case [`crate::multiorient`] documents at length and used to have to refuse.
//!
//! The recovered rotation is also where the B0 direction comes from, because the object rotated
//! and B0 did not — see [`crate::multiorient::resolve_directions_registered`]. The degeneracy
//! check still runs afterwards, on the recovered directions: if registration finds that the
//! orientations really are the same, the group is still refused.
//!
//! `--no-orientation-registration` turns registration off for users who co-registered
//! externally. A group that then still needs it is refused, which is what this module did
//! before it could register at all.

use std::path::PathBuf;

use log::{info, warn};

use crate::bids::derivatives::DerivativeOutputs;
use crate::bids::discovery::QsmRun;
use crate::bids::entities::AcquisitionKey;
use crate::bids::orientation::OrientationGroup;
use crate::error::QsmxtError;
use crate::multiorient::{
    check_directions, direction_table, resolve_directions_registered, DirectionSource,
    MultiOrientKind, Orientation, MIN_SPREAD_DEG,
};
use qsm_core::registration::{register_rigid, RigidParams, RigidTransform};
use crate::pipeline::config::PipelineConfig;

/// The output key for a group: every entity the members agree on, with the ones that varied
/// between orientations dropped.
///
/// Dropping only what actually varied matters. A dataset whose orientations differ in `acq`
/// but share a meaningful `rec` should keep the `rec` in the output name; blanking every
/// entity that *could* have varied would throw that away.
pub fn group_key(members: &[&QsmRun]) -> AcquisitionKey {
    let first = members[0].key.clone();
    let agree = |f: fn(&AcquisitionKey) -> &Option<String>| {
        members.iter().all(|m| f(&m.key) == f(&first))
    };
    AcquisitionKey {
        subject: first.subject.clone(),
        session: first.session.clone(),
        acquisition: agree(|k| &k.acquisition).then(|| first.acquisition.clone()).flatten(),
        reconstruction: agree(|k| &k.reconstruction).then(|| first.reconstruction.clone()).flatten(),
        inversion: agree(|k| &k.inversion).then(|| first.inversion.clone()).flatten(),
        run: agree(|k| &k.run).then(|| first.run.clone()).flatten(),
        suffix: first.suffix.clone(),
    }
}

/// Resolve each member's B0 direction in the common frame.
///
/// A sidecar `B0_dir` wins over the affine, exactly as it does for a single-orientation run —
/// and here it is usually the *only* thing that can be right, because a group whose members
/// were resampled into a common frame has identical affines by construction.
/// Display label per member: what distinguishes it, not the whole key — the shared part is
/// already in the group label.
fn member_labels(members: &[&QsmRun]) -> Vec<String> {
    members
        .iter()
        .map(|m| {
            m.key
                .acquisition
                .as_ref()
                .map(|a| format!("acq-{a}"))
                .or_else(|| m.key.run.as_ref().map(|r| format!("run-{r}")))
                .unwrap_or_else(|| m.key.to_string())
        })
        .collect()
}

fn orientations_for(members: &[&QsmRun], affines: &[[f64; 16]]) -> Vec<Orientation> {
    let labels = member_labels(members);
    let declared: Vec<Option<(f64, f64, f64)>> = members.iter().map(|m| m.b0_dir).collect();
    crate::multiorient::resolve_directions(&labels, &declared, affines, DirectionSource::Sidecar)
}

/// Why a group has to be co-registered, or `None` if it does not.
///
/// One place for the decision, because `reconstruct_group` and `preview_group` must agree on it
/// or `--dry` would promise something the run does not do. `grids_agree` is the caller's, since
/// the preview compares source headers and the run compares the derivative grids.
///
/// Registration can rescue a direction set only when **every** entry came off an affine. If the
/// directions were declared and still do not differ, the dataset is stating that the
/// orientations are the same, and contradicting that from the images would be guessing — which
/// is the thing [`crate::multiorient`] exists to not do. The other two ways a check can fail —
/// too few orientations, or coplanar ones — are properties of the acquisition that no amount of
/// registration changes.
fn registration_reason(
    grids_agree: bool,
    orientations: &[Orientation],
    check: &crate::multiorient::OrientationCheck,
) -> Option<&'static str> {
    if !grids_agree {
        return Some("the members are not on a common grid");
    }
    let directions_unusable = check.verdict.is_err()
        && check.max_pairwise_deg < MIN_SPREAD_DEG
        && orientations.iter().all(|o| o.source == DirectionSource::Affine);
    if directions_unusable {
        return Some("the affines are identical, so they cannot describe the rotations");
    }
    None
}

/// What `--dry` can say about a group's registration from headers alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationPlan {
    /// One grid, usable directions: nothing to register, and the previewed directions are the
    /// ones the reconstruction will use.
    NotNeeded,
    /// Will be co-registered. The previewed directions came off the affines and will be
    /// *replaced* by ones read from the recovered rotations, so a degenerate-looking preview
    /// here is not a refusal.
    WillRegister,
    /// Registration is needed but switched off, so the group will be refused.
    Disabled,
}

/// Preview a group's direction table and registration plan without running anything.
///
/// Reads each member's first phase header for its affine, which is cheap enough for `--dry`
/// and is the only way to show what the reconstruction would actually use. The TUI cannot do
/// this on every keystroke, so it previews structure and defers the physics to here.
///
/// The geometry comparison is on the *source* headers, while the reconstruction compares the
/// derivative grids the pipeline will actually produce. Those differ when a stage resamples —
/// an oblique acquisition taken to an axial grid, say — so the plan is a forecast, not a
/// promise. It can only be wrong in the safe direction: a run that turns out to need
/// registration gets it, because the decision is remade on the real grids.
pub fn preview_group(
    members: &[&QsmRun],
    kind: MultiOrientKind,
    register_allowed: bool,
) -> (Vec<Orientation>, crate::multiorient::OrientationCheck, RegistrationPlan) {
    let affines: Vec<[f64; 16]> = members
        .iter()
        .map(|m| {
            m.echoes
                .first()
                .and_then(|e| qsm_core::io::read_nifti_file(&e.phase_nifti).ok())
                .map(|n| n.affine)
                // No header to read: fall back to an identity affine, which yields (0,0,1) and
                // so shows up as the degenerate set it effectively is.
                .unwrap_or([
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ])
        })
        .collect();
    let orientations = orientations_for(members, &affines);
    let check = check_directions(&orientations, kind);

    let grids_agree = members.iter().all(|m| m.dims == members[0].dims)
        && affines.iter().all(|a| affine_drift(a, &affines[0]) <= AFFINE_TOLERANCE);
    let plan = match (registration_reason(grids_agree, &orientations, &check), register_allowed) {
        (None, _) => RegistrationPlan::NotNeeded,
        (Some(_), true) => RegistrationPlan::WillRegister,
        (Some(_), false) => RegistrationPlan::Disabled,
    };
    (orientations, check, plan)
}

/// Which reconstruction the config asks for.
pub fn kind_of(config: &PipelineConfig) -> MultiOrientKind {
    match config.multi_orientation.algorithm {
        crate::pipeline::config::MultiOrientAlgorithm::Cosmos => MultiOrientKind::Cosmos,
        crate::pipeline::config::MultiOrientAlgorithm::Sti => MultiOrientKind::Sti,
    }
}

/// The grid a group is reconstructed on: dimensions, voxel size, affine. Every member must
/// agree on all three, which is what "already co-registered" means in practice.
type Geometry = ((usize, usize, usize), (f64, f64, f64), [f64; 16]);

/// Whether two members sit on the same grid — the operational meaning of "co-registered".
fn geometry_matches(a: &Geometry, b: &Geometry) -> bool {
    a.0 == b.0 && affine_drift(&a.2, &b.2) <= AFFINE_TOLERANCE
}

/// Largest absolute element-wise difference between two affines.
fn affine_drift(a: &[f64; 16], b: &[f64; 16]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).fold(0.0f64, f64::max)
}

/// How far two affines may drift and still count as one grid. Loose enough to absorb the
/// round-trip through a NIfTI header's `f32` fields, tight enough that a real repositioning
/// cannot hide under it.
const AFFINE_TOLERANCE: f64 = 1e-3;

/// One member's contribution: its local field, the mask that field is defined on, the magnitude
/// that drives registration, and the geometry all three sit in.
struct MemberData {
    field: Vec<f64>,
    mask: Vec<u8>,
    /// Combined magnitude on the same grid as `field`. `None` for a phase-only run — such a
    /// group can still be combined if it is already co-registered, but cannot be registered.
    magnitude: Option<Vec<f64>>,
    geometry: Geometry,
}

/// Load one member's local field and background-removal mask.
fn load_member(output: &DerivativeOutputs, run: &QsmRun) -> crate::Result<MemberData> {
    let field_path = output.local_field_path(&run.key);
    if !field_path.exists() {
        return Err(QsmxtError::Config(format!(
            "no local field for {} at {} — the orientation's own pipeline must run before the \
             group is combined (and --clean-intermediates removes exactly this file, so leave \
             it off for multi-orientation runs)",
            run.key,
            field_path.display()
        )));
    }
    let field = qsm_core::io::read_nifti_file(&field_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", field_path.display(), e)))?;

    // The background-removal mask is the one the local field is actually defined on; the
    // brain mask is larger wherever the BFR eroded.
    let mask_path = {
        let bg = output.bg_mask_path(&run.key);
        if bg.exists() { bg } else { output.mask_path(&run.key) }
    };
    let mask_nifti = qsm_core::io::read_nifti_file(&mask_path)
        .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", mask_path.display(), e)))?;
    let mask: Vec<u8> = mask_nifti.data.iter().map(|&v| (v > 0.5) as u8).collect();

    // The combined magnitude, which is what registration correlates. Written by the same stage
    // on the same working grid as the local field, so a size mismatch means something upstream
    // wrote to the wrong geometry rather than that this member is unregisterable.
    let mag_path = output.magnitude_path(&run.key);
    let magnitude = if mag_path.exists() {
        let m = qsm_core::io::read_nifti_file(&mag_path)
            .map_err(|e| QsmxtError::NiftiIo(format!("{}: {}", mag_path.display(), e)))?;
        if m.dims != field.dims {
            return Err(QsmxtError::DimensionMismatch(format!(
                "{}: magnitude {} is {:?} but its local field is {:?}",
                run.key,
                mag_path.display(),
                m.dims,
                field.dims
            )));
        }
        Some(m.data)
    } else {
        None
    };

    Ok(MemberData {
        field: field.data,
        mask,
        magnitude,
        geometry: (field.dims, field.voxel_size, field.affine),
    })
}

/// One member's registration against the reference, for the log and the sidecar.
struct Registered {
    transform: RigidTransform,
    field: Vec<f64>,
    mask: Vec<u8>,
}

/// NCC below which a registration is treated as failed rather than merely imperfect. Two
/// orientations of one head, both masked to the brain, correlate far above this; anything near
/// it means the magnitudes are not of the same object, or the search left the basin. Refusing is
/// the point — a bad registration produces a plausible susceptibility map, which is the whole
/// failure mode this module exists to prevent.
const MIN_REGISTRATION_NCC: f64 = 0.5;
/// NCC below which the registration is reported but allowed through, because "imperfect" is
/// normal: the receive field is fixed to the coil, so it rotates with the coil and not with the
/// anatomy, and no global similarity metric is invariant to that.
const WARN_REGISTRATION_NCC: f64 = 0.8;

/// Co-register every member onto the first member's grid.
///
/// Member 0 gets the identity, so it is the reference by construction and its own field and mask
/// pass through without interpolation. Everything else is registered on magnitude, then its field
/// (continuous, so trilinear is safe) and its mask (nearest neighbour) are resampled across.
fn register_group(
    loaded: &[MemberData],
    members: &[&QsmRun],
    progress: &dyn Fn(&str),
) -> crate::Result<Vec<Registered>> {
    let (ref_dims, _, ref_affine) = loaded[0].geometry;
    let reference_mag = loaded[0].magnitude.as_ref().ok_or_else(|| {
        QsmxtError::Config(format!(
            "{} has no combined magnitude at {}, so the group cannot be co-registered —              registration is driven by magnitude, because a local field's contrast changes with              orientation by construction. Either supply magnitude for every orientation,              co-register them externally and pass --no-orientation-registration, or declare a              `B0_dir` in each sidecar if they are already on one grid",
            members[0].key,
            "the magnitude derivative"
        ))
    })?;

    let params = RigidParams::default();
    let mut out = Vec::with_capacity(loaded.len());
    for (t, member) in loaded.iter().enumerate() {
        let (dims, _, affine) = member.geometry;
        if t == 0 {
            out.push(Registered {
                transform: RigidTransform::identity(ref_dims, &ref_affine, dims, &affine),
                field: member.field.clone(),
                mask: member.mask.clone(),
            });
            continue;
        }

        let mag = member.magnitude.as_ref().ok_or_else(|| {
            QsmxtError::Config(format!(
                "{} has no combined magnitude, so it cannot be co-registered onto {}",
                members[t].key, members[0].key
            ))
        })?;

        progress(&format!(
            "Registering {} onto {}",
            members[t].key, members[0].key
        ));
        // The reference's own mask keeps the metric on brain: air contributes no anatomy and the
        // receive-field falloff outside the head is exactly the shading NCC cannot absorb.
        let transform = register_rigid(
            reference_mag,
            ref_dims,
            &ref_affine,
            mag,
            dims,
            &affine,
            Some(&loaded[0].mask),
            &params,
        )
        .ok_or_else(|| {
            QsmxtError::Config(format!(
                "could not register {} onto {} — one of the affines is singular, or the volumes                  never overlap",
                members[t].key, members[0].key
            ))
        })?;

        // NaN counts as failure, not as "not worse than the floor": NCC is undefined when a
        // candidate had no variance to correlate, and that is not an alignment either.
        if transform.ncc.is_nan() || transform.ncc <= MIN_REGISTRATION_NCC {
            return Err(QsmxtError::Config(format!(
                "registering {} onto {} reached a correlation of only {:.3} (overlap {:.2}) —                  that is not an alignment. Check that the two magnitudes are of the same                  subject and that the masks cover the brain; if they were co-registered                  elsewhere, pass --no-orientation-registration",
                members[t].key, members[0].key, transform.ncc, transform.overlap
            )));
        }
        if transform.ncc < WARN_REGISTRATION_NCC {
            warn!(
                "  {} registered with a correlation of only {:.3} — usable, but check the                  per-orientation susceptibility maps before trusting the combination",
                members[t].key, transform.ncc
            );
        }

        let field = transform.resample(&member.field).ok_or_else(|| {
            QsmxtError::Config(format!("could not resample {}'s local field", members[t].key))
        })?;
        let mask = transform.resample_mask(&member.mask).ok_or_else(|| {
            QsmxtError::Config(format!("could not resample {}'s mask", members[t].key))
        })?;
        out.push(Registered { transform, field, mask });
    }
    Ok(out)
}

/// The per-orientation registration table, logged next to the direction table.
///
/// The rotation column is the point: it is both the alignment that was applied and, read the
/// other way, the B0 direction the orientation contributes. A row showing 0.0 degrees for a
/// non-reference orientation means registration found nothing to align, which is worth seeing.
fn registration_table(members: &[&QsmRun], registered: &[Registered]) -> Vec<String> {
    registered
        .iter()
        .enumerate()
        .map(|(t, r)| {
            let tr = r.transform.translation_mm();
            if t == 0 {
                format!("{:<16} reference", members[t].key.to_string())
            } else {
                format!(
                    "{:<16} rotation {:>5.2}°  translation [{:>6.2} {:>6.2} {:>6.2}] mm                       NCC {:.4}  overlap {:.2}",
                    members[t].key.to_string(),
                    r.transform.rotation_magnitude_deg(),
                    tr[0],
                    tr[1],
                    tr[2],
                    r.transform.ncc,
                    r.transform.overlap
                )
            }
        })
        .collect()
}

/// Reconstruct one orientation group.
///
/// Returns the paths written. Errors are the caller's to report per group — one bad group
/// should not abandon the rest of the dataset.
pub fn reconstruct_group(
    group: &OrientationGroup,
    members: &[&QsmRun],
    config: &PipelineConfig,
    output: &DerivativeOutputs,
    progress: &dyn Fn(&str),
) -> crate::Result<Vec<PathBuf>> {
    let kind = kind_of(config);
    info!("{kind} for {} ({} orientations)", group.label, members.len());

    let loaded: Vec<MemberData> = members
        .iter()
        .map(|run| load_member(output, run))
        .collect::<crate::Result<Vec<_>>>()?;
    // The first member's grid is the common one. Picking a reference rather than inventing a
    // grid means one orientation is never interpolated at all, which is the one to look at when
    // a combination comes out wrong.
    let (dims, voxel_size, affine) = loaded[0].geometry;

    // What the group looks like before anything is moved.
    let header_affines: Vec<[f64; 16]> = loaded.iter().map(|m| m.geometry.2).collect();
    let header_orientations = orientations_for(members, &header_affines);
    let header_check = check_directions(&header_orientations, kind);
    let grids_agree = loaded.iter().all(|m| geometry_matches(&m.geometry, &loaded[0].geometry));

    let reason = registration_reason(grids_agree, &header_orientations, &header_check);

    let (fields, masks, orientations, registered) = match (reason, config.multi_orientation.register) {
        (None, _) => {
            info!("  orientations already share a grid and carry usable directions — no registration");
            let fields: Vec<Vec<f64>> = loaded.iter().map(|m| m.field.clone()).collect();
            let masks: Vec<Vec<u8>> = loaded.iter().map(|m| m.mask.clone()).collect();
            (fields, masks, header_orientations, None)
        }
        (Some(why), true) => {
            info!("  co-registering: {why}");
            let registered = register_group(&loaded, members, progress)?;
            info!("  registration:");
            for row in registration_table(members, &registered) {
                info!("    {row}");
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
            (fields, masks, orientations, Some(registered))
        }
        (Some(why), false) => {
            return Err(QsmxtError::DimensionMismatch(format!(
                "{}: {why}, and registration is switched off by --no-orientation-registration.                  Either drop that flag and let QSMxT co-register the orientations, or                  co-register them externally onto one grid first",
                group.label
            )));
        }
    };

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
            "the intersection of the orientation masks keeps {kept} of {largest} voxels \
             ({:.0}%) — that much loss usually means the orientations are not well aligned",
            100.0 * kept as f64 / largest as f64
        );
    }

    info!("  directions:");
    for row in direction_table(&orientations) {
        info!("    {row}");
    }
    // Re-checked on whatever the directions ended up being: if registration found that the
    // orientations really are the same, the group is still refused.
    let check = check_directions(&orientations, kind);
    match (&check.verdict, config.multi_orientation.force) {
        (Ok(()), _) => info!("  {}", check.summary()),
        (Err(why), false) => return Err(QsmxtError::Config(why.clone())),
        (Err(why), true) => warn!("  force: reconstructing anyway, but {why}"),
    }

    let bdirs: Vec<_> = orientations.iter().map(|o| o.b0).collect();
    let grid = qsm_core::Grid::new(dims.0, dims.1, dims.2, voxel_size.0, voxel_size.1, voxel_size.2);
    let key = group_key(members);
    let mut written = Vec::new();

    let write = |path: &PathBuf, data: &[f64]| -> crate::Result<()> {
        crate::nifti::write::write_volume(path, data, dims, voxel_size, &affine)
    };

    match kind {
        MultiOrientKind::Cosmos => {
            progress(&format!("COSMOS ({} orientations)", members.len()));
            let params = qsm_core::inversion::CosmosParams {
                lambda: config.multi_orientation.lambda,
                ..Default::default()
            };
            let chi = qsm_core::inversion::cosmos(&fields, &bdirs, &mask, &grid, &params);
            let path = output.cosmos_path(&key);
            write(&path, &chi)?;
            written.push(path);
        }
        MultiOrientKind::Sti => {
            progress(&format!("STI ({} orientations)", members.len()));
            let params = qsm_core::inversion::StiParams { lambda: config.multi_orientation.lambda };
            let tensor = qsm_core::inversion::sti(&fields, &bdirs, &mask, &grid, &params);
            let maps = qsm_core::inversion::tensor_maps(&tensor, &mask);
            // qsm-core stores the symmetric tensor as [X11, X12, X13, X22, X23, X33].
            for (name, data) in ["11", "12", "13", "22", "23", "33"]
                .iter()
                .zip(tensor.components.iter())
            {
                let path = output.sti_path(&key, &format!("sti{name}"), "Chitensor");
                write(&path, data)?;
                written.push(path);
            }
            for (desc, data) in [("mms", &maps.mms), ("msa", &maps.msa)] {
                let path = output.sti_path(&key, desc, "Chimap");
                write(&path, data)?;
                written.push(path);
            }
            for (axis, data) in ["x", "y", "z"].iter().zip(maps.pev.iter()) {
                let path = output.sti_path(&key, &format!("pev{axis}"), "Chitensor");
                write(&path, data)?;
                written.push(path);
            }
        }
    }

    // The direction table is the half of the input that does not live in the NIfTI files, so
    // without it the reconstruction cannot be reproduced or audited.
    let desc = match kind {
        MultiOrientKind::Cosmos => "cosmos",
        MultiOrientKind::Sti => "sti",
    };
    let sidecar = serde_json::json!({
        "Description": format!("{kind} reconstruction over {} orientations", members.len()),
        "Sources": members.iter().map(|m| m.key.to_string()).collect::<Vec<_>>(),
        "B0_dirs": orientations.iter().map(|o| vec![o.b0.0, o.b0.1, o.b0.2]).collect::<Vec<_>>(),
        "B0_dir_sources": orientations.iter().map(|o| o.source.to_string()).collect::<Vec<_>>(),
        "Lambda": config.multi_orientation.lambda,
        "MaxPairwiseAngleDeg": check.max_pairwise_deg,
        "MinPairwiseAngleDeg": check.min_pairwise_deg,
        "DirectionRank": check.rank,
        "MaskVoxels": kept,
        // The transforms are the other half of the input that does not live in the NIfTI files.
        // Without them a registered reconstruction cannot be re-derived or audited, and the
        // rotation is also the provenance of the B0 direction beside it.
        "Registration": registered.as_ref().map(|regs| {
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
    let sidecar_path = output.multiorient_sidecar_path(&key, desc);
    if let Some(parent) = sidecar_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| QsmxtError::Config(format!("multi-orientation sidecar: {e}")))?;
    std::fs::write(&sidecar_path, text)?;
    written.push(sidecar_path);

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bids::entities::AcquisitionKey;

    fn run_with(acq: Option<&str>, rec: Option<&str>, run_e: Option<&str>) -> QsmRun {
        QsmRun {
            key: AcquisitionKey {
                subject: "01".into(),
                session: Some("01".into()),
                acquisition: acq.map(Into::into),
                reconstruction: rec.map(Into::into),
                inversion: None,
                run: run_e.map(Into::into),
                suffix: "MEGRE".into(),
            },
            echoes: vec![],
            coils: None,
            magnetic_field_strength: 3.0,
            echo_times: vec![],
            b0_dir: None,
            dims: (1, 1, 1),
            has_magnitude: false,
            mese: None,
        }
    }

    /// The entity that varied is dropped; an entity every member agrees on is kept, because
    /// it still describes the output.
    #[test]
    fn group_key_drops_only_what_varied() {
        let a = run_with(Some("dir1"), Some("mcpc3ds"), None);
        let b = run_with(Some("dir2"), Some("mcpc3ds"), None);
        let key = group_key(&[&a, &b]);
        assert_eq!(key.acquisition, None, "acq varied, so it is dropped");
        assert_eq!(key.reconstruction.as_deref(), Some("mcpc3ds"), "rec agrees, so it is kept");
        assert_eq!(key.basename(), "sub-01_ses-01_rec-mcpc3ds");
    }

    #[test]
    fn group_key_drops_a_varying_run_entity() {
        let a = run_with(Some("gre"), None, Some("1"));
        let b = run_with(Some("gre"), None, Some("2"));
        let key = group_key(&[&a, &b]);
        assert_eq!(key.run, None);
        assert_eq!(key.acquisition.as_deref(), Some("gre"));
        assert_eq!(key.basename(), "sub-01_ses-01_acq-gre");
    }

    #[test]
    fn cosmos_output_lands_beside_the_other_derivatives() {
        let a = run_with(Some("dir1"), None, None);
        let b = run_with(Some("dir2"), None, None);
        let out = DerivativeOutputs::new(std::path::Path::new("/out"));
        let path = out.cosmos_path(&group_key(&[&a, &b]));
        assert_eq!(
            path,
            std::path::PathBuf::from("/out/sub-01/ses-01/anat/sub-01_ses-01_desc-cosmos_Chimap.nii")
        );
    }

    /// A lumpy, orientation-identifiable phantom. A smooth blob would register to anything.
    fn phantom(dims: (usize, usize, usize)) -> Vec<f64> {
        let (nx, ny, nz) = dims;
        let mut v = vec![0.0f64; nx * ny * nz];
        let blobs = [
            (0.50, 0.50, 0.50, 0.34, 0.26, 0.30, 1.0f64),
            (0.36, 0.44, 0.52, 0.10, 0.09, 0.11, 2.2),
            (0.63, 0.57, 0.44, 0.08, 0.13, 0.07, 1.7),
            (0.50, 0.62, 0.64, 0.07, 0.06, 0.09, 2.8),
        ];
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let (x, y, z) =
                        (i as f64 / nx as f64, j as f64 / ny as f64, k as f64 / nz as f64);
                    let mut acc = 0.0;
                    for &(cx, cy, cz, rx, ry, rz, val) in &blobs {
                        let d = ((x - cx) / rx).powi(2)
                            + ((y - cy) / ry).powi(2)
                            + ((z - cz) / rz).powi(2);
                        if d <= 1.0 {
                            acc += val * (1.0 - d).sqrt();
                        }
                    }
                    v[i + j * nx + k * nx * ny] = acc;
                }
            }
        }
        v
    }

    /// Rigid 4×4 rotating about world x through the volume's centre, built from literals.
    fn rotate_x(deg: f64, dims: (usize, usize, usize), affine: &[f64; 16]) -> [f64; 16] {
        let (s, c) = deg.to_radians().sin_cos();
        let cy = (dims.1 as f64 - 1.0) * 0.5 * affine[5];
        let cz = (dims.2 as f64 - 1.0) * 0.5 * affine[10];
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, c, -s, cy - (c * cy - s * cz), //
            0.0, s, c, cz - (s * cy + c * cz), //
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    /// Write the derivatives one member of a group would have left on disk: a local field, a
    /// background-removal mask and a combined magnitude, all on the same grid.
    #[allow(clippy::too_many_arguments)] // A grid plus three volumes is what a member is.
    fn write_member(
        output: &DerivativeOutputs,
        key: &AcquisitionKey,
        dims: (usize, usize, usize),
        voxel_size: (f64, f64, f64),
        affine: &[f64; 16],
        magnitude: &[f64],
        field: &[f64],
        mask: &[u8],
    ) {
        let w = |p: std::path::PathBuf, d: &[f64]| {
            crate::nifti::write::write_volume(&p, d, dims, voxel_size, affine).unwrap()
        };
        w(output.magnitude_path(key), magnitude);
        w(output.local_field_path(key), field);
        w(
            output.bg_mask_path(key),
            &mask.iter().map(|&m| m as f64).collect::<Vec<_>>(),
        );
    }

    /// `reconstruct_group` end to end on the shape that used to be a hard error: three
    /// orientations on byte-identical affines, with the anatomy rotated in voxel space.
    ///
    /// Everything the pipeline would have on disk is written out — magnitude, local field,
    /// background mask, per orientation — and then the real entry point is called. What it has
    /// to do is register on magnitude, resample the fields, read the B0 directions off the
    /// recovered rotations, and reconstruct. The directions are checked against the rotations
    /// the data was built with, so an inverted transform fails here rather than producing a
    /// plausible map.
    #[test]
    fn reconstructs_a_group_that_needs_registering() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(dir.path());
        let dims = (40, 40, 40);
        let voxel_size = (1.0, 1.0, 1.0);
        let affine = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let mag = phantom(dims);
        // A mask over the phantom's body, which is also what the registration metric sees.
        let mask: Vec<u8> = mag.iter().map(|&v| (v > 0.05) as u8).collect();
        // A local field with its own structure, so resampling it is visible in the output.
        let field: Vec<f64> = mag.iter().map(|v| 0.01 * v).collect();

        let tilts = [0.0f64, 15.0, -20.0];
        let runs: Vec<QsmRun> = (1..=3)
            .map(|i| run_with(Some(&format!("dir{i}")), None, None))
            .collect();
        for (t, &deg) in tilts.iter().enumerate() {
            let w = mat4(&rotate_x(deg, dims, &affine), &affine);
            let m = qsm_core::geometry::resample_onto(&mag, dims, &w, dims, &affine).unwrap();
            let f = qsm_core::geometry::resample_onto(&field, dims, &w, dims, &affine).unwrap();
            let k = qsm_core::geometry::resample_mask_onto(&mask, dims, &w, dims, &affine).unwrap();
            write_member(&output, &runs[t].key, dims, voxel_size, &affine, &m, &f, &k);
        }

        let members: Vec<&QsmRun> = runs.iter().collect();
        let group = OrientationGroup { label: "sub-01_ses-01".into(), members: vec![0, 1, 2] };
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();

        let written = reconstruct_group(&group, &members, &config, &output, &|_| {})
            .expect("the group should reconstruct now that it can be registered");
        assert!(
            written.iter().any(|p| p.to_string_lossy().contains("desc-cosmos")),
            "{written:?}"
        );

        // The sidecar is the audit trail; it has to carry the transforms and the directions.
        let sidecar = written
            .iter()
            .find(|p| p.extension().is_some_and(|e| e == "json"))
            .expect("a sidecar should be written");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(sidecar).unwrap()).unwrap();
        let sources = json["B0_dir_sources"].as_array().unwrap();
        assert!(
            sources.iter().all(|s| s == "registration"),
            "directions should be attributed to the registration, got {sources:?}"
        );
        let reg = json["Registration"].as_array().unwrap();
        assert_eq!(reg.len(), 3);
        assert!(reg[0]["Reference"].as_bool().unwrap(), "member 0 is the reference");

        // The recovered directions must match the rotations the data was built with. Rotating
        // the object by `deg` about world x puts B0 at (0, sin deg, cos deg) in the common
        // frame — so the y component carries the sign, and getting it backwards fails here.
        let dirs = json["B0_dirs"].as_array().unwrap();
        for (t, &deg) in tilts.iter().enumerate() {
            let got = dirs[t].as_array().unwrap();
            let (y, z) = (got[1].as_f64().unwrap(), got[2].as_f64().unwrap());
            let a = deg.to_radians();
            println!(
                "orientation {t} ({deg} deg): B0 y={y:.4} z={z:.4}, expected y={:.4} z={:.4}",
                a.sin(), a.cos()
            );
            assert!((y - a.sin()).abs() < 0.03, "orientation {t}: y={y}, expected {}", a.sin());
            assert!((z - a.cos()).abs() < 0.03, "orientation {t}: z={z}, expected {}", a.cos());
        }
        // ...and they are a usable set, which the affines alone were not.
        assert!(
            json["MaxPairwiseAngleDeg"].as_f64().unwrap() > 30.0,
            "spread {:?}",
            json["MaxPairwiseAngleDeg"]
        );
    }

    /// The same group with `--no-orientation-registration`: refused, and the message says why
    /// and which flag caused it. This is the opt-out for externally registered data, so it has
    /// to actually opt out rather than silently reconstruct.
    #[test]
    fn registration_can_be_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(dir.path());
        let dims = (32, 32, 32);
        let voxel_size = (1.0, 1.0, 1.0);
        let mag = phantom(dims);
        let mask: Vec<u8> = mag.iter().map(|&v| (v > 0.05) as u8).collect();
        let runs: Vec<QsmRun> = (1..=2)
            .map(|i| run_with(Some(&format!("dir{i}")), None, None))
            .collect();
        // Different grids *and* a real rotation, so registration is required and the recovered
        // directions genuinely differ once it is allowed to run.
        for (t, &(shift, tilt)) in [(0.0f64, 0.0f64), (3.0, 20.0)].iter().enumerate() {
            let affine = [
                1.0, 0.0, 0.0, shift, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ];
            let base = [
                1.0, 0.0, 0.0, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ];
            let w = mat4(&rotate_x(tilt, dims, &base), &affine);
            let m = qsm_core::geometry::resample_onto(&mag, dims, &w, dims, &affine).unwrap();
            let k = qsm_core::geometry::resample_mask_onto(&mask, dims, &w, dims, &affine).unwrap();
            write_member(&output, &runs[t].key, dims, voxel_size, &affine, &m, &m, &k);
        }

        let members: Vec<&QsmRun> = runs.iter().collect();
        let group = OrientationGroup { label: "sub-01_ses-01".into(), members: vec![0, 1] };
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();
        config.multi_orientation.register = false;

        let err = reconstruct_group(&group, &members, &config, &output, &|_| {})
            .expect_err("registration is off, so a group that needs it must be refused");
        let msg = err.to_string();
        assert!(msg.contains("--no-orientation-registration"), "{msg}");
        assert!(msg.contains("common grid"), "{msg}");

        // And with it back on, the same group goes through.
        config.multi_orientation.register = true;
        reconstruct_group(&group, &members, &config, &output, &|_| {})
            .expect("the same group should reconstruct once registration is allowed");
    }

    /// Registration must not manufacture a spread that is not in the data.
    ///
    /// Two members on different grids but related by a pure *translation*: registration has real
    /// work to do (they are not on one grid) and does it, but the recovered rotations are
    /// identities, so the B0 directions come out identical and the group is still refused. The
    /// degeneracy gate has to survive the new path, or registration would become a way to
    /// launder a single-orientation dataset into something COSMOS accepts.
    #[test]
    fn registration_does_not_invent_a_spread() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(dir.path());
        let dims = (24, 24, 24);
        let voxel_size = (1.0, 1.0, 1.0);
        let mag = phantom(dims);
        let mask = vec![1u8; mag.len()];
        let runs: Vec<QsmRun> = (1..=2)
            .map(|i| run_with(Some(&format!("dir{i}")), None, None))
            .collect();
        for (t, shift) in [0.0f64, 3.0].iter().enumerate() {
            let affine = [
                1.0, 0.0, 0.0, *shift, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ];
            write_member(&output, &runs[t].key, dims, voxel_size, &affine, &mag, &mag, &mask);
        }

        let members: Vec<&QsmRun> = runs.iter().collect();
        let group = OrientationGroup { label: "sub-01_ses-01".into(), members: vec![0, 1] };
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();

        let err = reconstruct_group(&group, &members, &config, &output, &|_| {})
            .expect_err("two translated copies of one orientation are not a multi-orientation set");
        assert!(err.to_string().contains("not a multi-orientation set"), "{err}");
    }

    /// A group already on one grid with declared directions must be left alone — registering it
    /// would only add an interpolation. Pinned by the absence of a `Registration` block, which
    /// is the only observable difference in the output.
    #[test]
    fn an_already_registered_group_is_not_registered_again() {
        let dir = tempfile::tempdir().unwrap();
        let output = DerivativeOutputs::new(dir.path());
        let dims = (24, 24, 24);
        let voxel_size = (1.0, 1.0, 1.0);
        let affine = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let mag = phantom(dims);
        let mask = vec![1u8; mag.len()];
        let mut runs: Vec<QsmRun> = (1..=2)
            .map(|i| run_with(Some(&format!("dir{i}")), None, None))
            .collect();
        runs[0].b0_dir = Some((0.0, 0.0, 1.0));
        runs[1].b0_dir = Some((0.0, 0.5, 0.866));
        for run in &runs {
            write_member(&output, &run.key, dims, voxel_size, &affine, &mag, &mag, &mask);
        }

        let members: Vec<&QsmRun> = runs.iter().collect();
        let group = OrientationGroup { label: "sub-01_ses-01".into(), members: vec![0, 1] };
        let mut config = PipelineConfig::default();
        config.multi_orientation.group_by = "acq".into();

        let written = reconstruct_group(&group, &members, &config, &output, &|_| {}).unwrap();
        let sidecar = written
            .iter()
            .find(|p| p.extension().is_some_and(|e| e == "json"))
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(sidecar).unwrap()).unwrap();
        assert!(json["Registration"].is_null(), "should not have registered: {:?}", json["Registration"]);
        let sources = json["B0_dir_sources"].as_array().unwrap();
        assert!(sources.iter().all(|s| s == "sidecar"), "{sources:?}");
    }

    /// Row-major 4×4 product, for building the test warps.
    fn mat4(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
        let mut out = [0.0f64; 16];
        for i in 0..4 {
            for j in 0..4 {
                out[4 * i + j] = (0..4).map(|k| a[4 * i + k] * b[4 * k + j]).sum();
            }
        }
        out
    }

    #[test]
    fn registration_reason_only_fires_where_registration_can_help() {
        use crate::multiorient::check_directions;
        let o = |s: DirectionSource, b: (f64, f64, f64)| Orientation {
            label: "x".into(),
            b0: b,
            source: s,
        };

        // Identical affine-derived directions: registration is exactly the fix.
        let affine_degenerate =
            vec![o(DirectionSource::Affine, (0.0, 0.0, 1.0)), o(DirectionSource::Affine, (0.0, 0.0, 1.0))];
        let check = check_directions(&affine_degenerate, MultiOrientKind::Cosmos);
        assert!(registration_reason(true, &affine_degenerate, &check).is_some());

        // Declared and still identical: the dataset says the orientations are the same, and
        // contradicting it from the images would be guessing.
        let declared_degenerate = vec![
            o(DirectionSource::Sidecar, (0.0, 0.0, 1.0)),
            o(DirectionSource::Sidecar, (0.0, 0.0, 1.0)),
        ];
        let check = check_directions(&declared_degenerate, MultiOrientKind::Cosmos);
        assert!(registration_reason(true, &declared_degenerate, &check).is_none());

        // Coplanar is a property of the acquisition; no registration changes it.
        let coplanar: Vec<_> = (0..6)
            .map(|i| {
                let a = (i as f64) * 12.0f64.to_radians();
                o(DirectionSource::Affine, (0.0, a.sin(), a.cos()))
            })
            .collect();
        let check = check_directions(&coplanar, MultiOrientKind::Sti);
        assert!(check.verdict.is_err(), "the coplanar set should be refused");
        assert!(registration_reason(true, &coplanar, &check).is_none());

        // Grids disagreeing is enough on its own, even with perfectly good directions.
        let fine = vec![o(DirectionSource::Sidecar, (0.0, 0.0, 1.0)), o(DirectionSource::Sidecar, (0.0, 0.5, 0.866))];
        let check = check_directions(&fine, MultiOrientKind::Cosmos);
        assert!(registration_reason(true, &fine, &check).is_none());
        assert!(registration_reason(false, &fine, &check).is_some());
    }

    /// A sidecar direction beats the affine — which is the only thing that can be right for a
    /// group already resampled into one frame, where every affine is identical.
    #[test]
    fn sidecar_directions_win_over_identical_affines() {
        let mut a = run_with(Some("dir1"), None, None);
        let mut b = run_with(Some("dir2"), None, None);
        a.b0_dir = Some((0.0, 0.0, 1.0));
        b.b0_dir = Some((0.0, 0.5, 0.866));
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let orientations = orientations_for(&[&a, &b], &[identity, identity]);
        assert_eq!(orientations[0].label, "acq-dir1");
        assert!(orientations.iter().all(|o| o.source == DirectionSource::Sidecar));
        let check = check_directions(&orientations, MultiOrientKind::Cosmos);
        assert!(check.is_ok(), "{:?}", check.verdict);
    }

    /// ...and without sidecars, that same group is refused rather than silently reconstructed.
    #[test]
    fn identical_affines_without_sidecars_are_refused() {
        let a = run_with(Some("dir1"), None, None);
        let b = run_with(Some("dir2"), None, None);
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let orientations = orientations_for(&[&a, &b], &[identity, identity]);
        assert!(orientations.iter().all(|o| o.source == DirectionSource::Affine));
        let why = check_directions(&orientations, MultiOrientKind::Cosmos).verdict.unwrap_err();
        assert!(why.contains("affines"), "{why}");
    }
}
