//! What a scan is, from its NIfTI headers and BIDS sidecars, and the grid it is reconstructed on.
//!
//! The front end reads the files (`qsmxt` from a BIDS directory, qsmbly from what the user
//! loaded) and hands the values over; everything decided from them is decided here.

use serde::{Deserialize, Serialize};

use crate::config::PipelineConfig;
use crate::error::EngineError;
use crate::record::{Choice, Decision, Topic};

/// The facts about one scan that decide what it can produce and how it is reconstructed: the
/// acquisition from its headers and sidecars, and which bring-your-own derivatives the config
/// names were found for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanFacts {
    /// The matrix size of the first echo, as acquired.
    pub dims: (usize, usize, usize),
    pub n_echoes: usize,
    /// Echo times in seconds, in echo order.
    pub echo_times: Vec<f64>,
    /// Magnitude images, or real/imaginary input (which gives them).
    pub has_magnitude: bool,
    /// Uncombined receive-coil channels; 1 for combined data.
    pub n_coils: usize,
    /// Given as real and imaginary parts rather than magnitude and phase.
    pub real_imaginary: bool,
    /// Tesla.
    pub field_strength: f64,
    /// B0 direction in voxel coordinates, when a sidecar declares `B0_dir`.
    pub b0_dir: Option<(f64, f64, f64)>,
    /// Echo times of a matched multi-echo spin-echo (MESE) acquisition, if there is one.
    pub mese_echo_times: Option<Vec<f64>>,
    /// A brain mask from `--use-custom-masks`' tool was found for this scan.
    pub custom_mask: bool,
    /// A segmentation from `--use-custom-dseg`'s tool was found for this scan.
    pub custom_dseg: bool,
    /// A susceptibility map from `--use-custom-qsm`'s tool was found.
    pub custom_qsm: bool,
    /// An R2 map from `--use-custom-r2`'s tool was found.
    pub custom_r2: bool,
    /// An R2' map from `--use-custom-r2prime`'s tool was found.
    pub custom_r2prime: bool,
}

impl ScanFacts {
    /// The facts of a combined-coil magnitude/phase scan, from its first echo's matrix size and
    /// every echo's sidecar, in echo order: what a front end that holds the files itself (qsmbly)
    /// has. Echo times are each sidecar's; the field strength and `B0_dir` are read as `qsmxt`
    /// reads a BIDS run's (the last echo that has one wins; a `B0_dir` without three components
    /// is ignored, with a warning). No bring-your-own derivatives: a front end that has them sets
    /// those fields itself.
    pub fn from_sidecars(
        dims: (usize, usize, usize), sidecars: &[QsmSidecar], has_magnitude: bool,
    ) -> Result<Self, EngineError> {
        let Some(last) = sidecars.last() else {
            return Err(EngineError::InvalidInput("a scan needs at least one echo".into()));
        };
        let mut b0_dir = None;
        for (n, sc) in sidecars.iter().enumerate() {
            match sc.b0_dir.as_deref() {
                Some(&[x, y, z]) => b0_dir = Some((x, y, z)),
                Some(dir) => log::warn!(
                    "B0 direction has {} components (expected 3), defaulting to (0,0,1): echo {}",
                    dir.len(), n + 1),
                None => {}
            }
        }
        Ok(ScanFacts {
            dims,
            n_echoes: sidecars.len(),
            echo_times: sidecars.iter().map(|sc| sc.echo_time).collect(),
            has_magnitude,
            n_coils: 1,
            real_imaginary: false,
            field_strength: last.magnetic_field_strength,
            b0_dir,
            mese_echo_times: None,
            custom_mask: false,
            custom_dseg: false,
            custom_qsm: false,
            custom_r2: false,
            custom_r2prime: false,
        })
    }
}

/// What the orientation gate cannot read from the config alone.
///
/// Only whether a measured R2 exists, which is what decides whether `--r2prime-strategy auto`
/// falls back to R2PRIMEnet. Both sources are judged optimistically — a declared
/// `--custom-r2-tool` whose derivative turns out to be missing, or a MESE that turns out to be
/// unreadable, both leave `auto` predicting R2′ after all. `qsmxt`'s R2′ stage warns when that
/// happens on a grid that was left oblique.
pub fn orientation_facts(facts: &ScanFacts, config: &PipelineConfig) -> crate::bridge::RunOrientationFacts {
    crate::bridge::RunOrientationFacts {
        has_measured_r2: facts.mese_echo_times.is_some() || config.separation.custom_r2_tool.is_some(),
    }
}

/// Relevant fields from a BIDS JSON sidecar. Serialises back to the sidecar's own field names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QsmSidecar {
    #[serde(rename = "EchoTime")]
    pub echo_time: f64,

    #[serde(rename = "MagneticFieldStrength")]
    pub magnetic_field_strength: f64,

    #[serde(rename = "B0_dir", default, skip_serializing_if = "Option::is_none")]
    pub b0_dir: Option<Vec<f64>>,

    /// `SliceThickness` (mm): the excited thickness, which for a 2D acquisition can be less than
    /// the slice pitch.
    #[serde(rename = "SliceThickness", default, skip_serializing_if = "Option::is_none")]
    pub slice_thickness: Option<f64>,
    /// `MRAcquisitionType`: "2D" or "3D".
    #[serde(rename = "MRAcquisitionType", default, skip_serializing_if = "Option::is_none")]
    pub mr_acquisition_type: Option<String>,
    /// `SliceEncodingDirection`: "i", "j" or "k", optionally with "-".
    #[serde(rename = "SliceEncodingDirection", default, skip_serializing_if = "Option::is_none")]
    pub slice_encoding_direction: Option<String>,
    /// `Manufacturer`, which decides the phase sign convention of real/imaginary input.
    #[serde(rename = "Manufacturer", default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
}

impl QsmSidecar {
    /// A sidecar giving only the echo time and field strength.
    pub fn bare(echo_time: f64, magnetic_field_strength: f64) -> Self {
        QsmSidecar {
            echo_time, magnetic_field_strength, b0_dir: None, slice_thickness: None,
            mr_acquisition_type: None, slice_encoding_direction: None, manufacturer: None,
        }
    }

    /// The QSM-relevant fields of a parsed sidecar. `Err` names the required field that is
    /// missing (or not a number): `EchoTime` or `MagneticFieldStrength`.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, &'static str> {
        let echo_time = value.get("EchoTime").and_then(|v| v.as_f64()).ok_or("EchoTime")?;
        let magnetic_field_strength = value
            .get("MagneticFieldStrength")
            .and_then(|v| v.as_f64())
            .ok_or("MagneticFieldStrength")?;

        let b0_dir = value.get("B0_dir").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_f64())
                    .collect::<Vec<f64>>()
            })
        });

        let string = |k: &str| value.get(k).and_then(|v| v.as_str()).map(str::to_string);
        Ok(QsmSidecar {
            echo_time,
            magnetic_field_strength,
            b0_dir,
            slice_thickness: value.get("SliceThickness").and_then(|v| v.as_f64()),
            mr_acquisition_type: string("MRAcquisitionType"),
            slice_encoding_direction: string("SliceEncodingDirection"),
            manufacturer: string("Manufacturer"),
        })
    }
}

/// Metadata about the run, extracted during the load step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub dims: (usize, usize, usize),
    pub voxel_size: (f64, f64, f64),
    pub affine: [f64; 16],
    pub n_echoes: usize,
    pub echo_times: Vec<f64>,
    pub b0_direction: (f64, f64, f64),
    pub field_strength: f64,
    pub has_magnitude: bool,
    /// Set when the run is being resampled to a cardinal-aligned grid: the dims and affine of the
    /// files as they sit on disk. `dims`/`voxel_size`/`affine` above are then the *working* grid
    /// every stage after `scale_phase` sees, and `b0_direction` is `(0,0,1)`.
    #[serde(default)]
    pub source_geometry: Option<((usize, usize, usize), [f64; 16])>,
    /// `(thickness mm, voxel axis)` of the acquisition's slices on the working grid, when the
    /// sidecar records them and the grid was not resampled; qsm-core's stages check it for a
    /// gap. `None` = not known (or `--ignore-slice-gap`).
    #[serde(default)]
    pub slice_geometry: Option<(f64, usize)>,
}

/// How far B0 may sit from the slice normal before an axial-only algorithm is given resampled
/// data. Acquisitions carry sub-degree tilts from rounding in the affine; resampling for those
/// costs an interpolation and buys nothing.
pub const AXIAL_ONLY_TOLERANCE_DEG: f64 = 1.0;

/// Work out the grid and B0 direction the pipeline will actually reconstruct on, from the first
/// phase image's header (`dims`, `voxel_size`, `affine`).
///
/// The dipole kernel is built in voxel space, so an oblique acquisition has to be handled one of
/// two ways, and doing neither is what leaves susceptibility contrast on the floor:
///
/// 1. **Resample to a cardinal-aligned grid** (`--obliquity-threshold`, as QSMxT 8.x did), after
///    which B0 is `(0,0,1)` by construction. Costs one interpolation of every echo.
/// 2. **Keep the grid and rotate the kernel**, using the true B0 direction from the affine.
///
/// A sidecar `B0_dir` always wins, since it describes the acquisition better than the affine can.
pub fn resolve_geometry(
    dims: (usize, usize, usize),
    voxel_size: (f64, f64, f64),
    affine: [f64; 16],
    facts: &ScanFacts,
    config: &PipelineConfig,
) -> RunMetadata {
    let obliquity = qsm_core::geometry::obliquity_from_affine(&affine);
    let tilt = qsm_core::geometry::b0_angle_from_affine(&affine);
    let threshold = config.pipeline.obliquity_threshold;
    let axes = qsm_core::geometry::obliquity_axes_from_affine(&affine);

    let mut meta = RunMetadata {
        dims,
        voxel_size,
        affine,
        n_echoes: facts.n_echoes,
        echo_times: facts.echo_times.clone(),
        b0_direction: (0.0, 0.0, 1.0),
        field_strength: facts.field_strength,
        has_magnitude: facts.has_magnitude,
        source_geometry: None, slice_geometry: None,
    };

    if obliquity > 0.01 {
        log::info!(
            "Obliquity {:.1}° (per-axis {:.1}/{:.1}/{:.1}°); B0 is {:.1}° from the slice normal",
            obliquity, axes[0], axes[1], axes[2], tilt
        );
    }

    // An algorithm that only understands axial data settles the question before anything else
    // does. It takes no B0 direction, so neither a sidecar nor the affine can help it: the only
    // way to reconstruct correctly is to move the data. Below the obliquity threshold the tilt
    // is small enough not to bother.
    let axial_only = crate::bridge::axial_only_algorithms(config, orientation_facts(facts, config));
    let forced_axial = !axial_only.is_empty() && tilt > AXIAL_ONLY_TOLERANCE_DEG;
    if forced_axial {
        log::info!(
            "Resampling to axial because {} {} no B0 direction and {} assume it is +z; B0 is \
             {:.1}° from the slice normal here",
            axial_only.join(", "),
            if axial_only.len() == 1 { "takes" } else { "take" },
            if axial_only.len() == 1 { "it assumes" } else { "they assume" },
            tilt
        );
    }

    // A sidecar B0_dir is authoritative for anything that can use a direction at all.
    if !forced_axial {
        if let Some(dir) = facts.b0_dir {
            meta.b0_direction = dir;
            log::info!("B0 direction {:?} (from the JSON sidecar)", dir);
            return meta;
        }
    }

    let resample = forced_axial || (config.pipeline.axial_resampling && obliquity > threshold);
    if resample {
        // A matched MESE used to veto this, on both branches: the GRE would move to a cardinal
        // grid and the MESE, read straight from BIDS, would be left behind. It no longer does,
        // because the MESE is no longer assumed to share the GRE's grid — `qsmxt` resamples it
        // onto whatever grid is chosen here. That also makes a MESE acquired on its own grid
        // usable, which is the ordinary case and used to be discarded.
        let grid = qsm_core::geometry::axial_grid_for(dims.0, dims.1, dims.2, &affine);
        let why = if forced_axial {
            "required by the chosen algorithm".to_string()
        } else {
            format!("obliquity {obliquity:.1}° > threshold {threshold:.1}°")
        };
        log::info!(
            "Resampling to axial: {}x{}x{} -> {}x{}x{} ({}); B0 becomes (0, 0, 1)",
            dims.0, dims.1, dims.2,
            grid.dims.0, grid.dims.1, grid.dims.2, why
        );
        if facts.mese_echo_times.is_some() {
            log::info!("The matched MESE acquisition will be resampled onto the same grid");
        }
        meta.source_geometry = Some((dims, affine));
        meta.dims = grid.dims;
        meta.voxel_size = grid.voxel_size;
        meta.affine = grid.affine;
        meta.b0_direction = (0.0, 0.0, 1.0);
        return meta;
    }

    // No resampling: build the kernel on the acquired grid, with B0 where it actually points.
    meta.b0_direction = qsm_core::geometry::b0_direction_from_affine(&affine);
    if tilt > 0.5 {
        log::info!(
            "B0 direction ({:.3}, {:.3}, {:.3}) from the affine; pass --axial-resampling to \
             resample to axial instead",
            meta.b0_direction.0, meta.b0_direction.1, meta.b0_direction.2
        );
    }
    meta
}

/// qsm-core's scan metadata for a stage working on the run's (working) grid, with the slice
/// geometry for qsm-core's slice-gap check and the working grid's affine — after axial
/// resampling that is the resampled grid's, which is the grid the stages run on.
pub fn scan_metadata(meta: &RunMetadata) -> qsm_core::pipeline::ScanMetadata {
    let mut m = crate::bridge::to_scan_metadata(
        meta.dims, meta.voxel_size, &meta.echo_times, meta.field_strength, meta.b0_direction,
        Some(meta.affine),
    );
    m.slice_geometry = meta.slice_geometry.map(|(thickness, axis)| qsm_core::grid::SliceGeometry { thickness, axis });
    m
}

/// [`scan_metadata`] for a stage working on the box `cb` of the run's grid (same voxel size; the
/// affine moved to the box's first voxel).
pub fn scan_metadata_in(meta: &RunMetadata, cb: &qsm_core::crop::CropBox) -> qsm_core::pipeline::ScanMetadata {
    let mut m = scan_metadata(meta);
    m.dims = cb.dims;
    m.affine = Some(cropped_affine(&meta.affine, cb.origin));
    m
}

/// The affine of a sub-grid whose voxel (0,0,0) is voxel `origin` of the grid `affine` describes.
fn cropped_affine(affine: &[f64; 16], origin: (isize, isize, isize)) -> [f64; 16] {
    let o = [origin.0 as f64, origin.1 as f64, origin.2 as f64];
    let mut a = *affine;
    for r in 0..3 {
        a[4 * r + 3] += (0..3).map(|c| affine[4 * r + c] * o[c]).sum::<f64>();
    }
    a
}

/// The acquisition's slices, as `(thickness mm, voxel axis)`, when the sidecar says enough to
/// check them for a gap: `SliceThickness`, and either `MRAcquisitionType` "2D" or a thickness
/// that differs from the pitch (a 3D acquisition records its partition thickness, the pitch, so
/// there is nothing to check). The axis is `SliceEncodingDirection`'s, else the coarsest one.
pub fn slice_geometry_from_sidecar(sc: &QsmSidecar, voxel_size: (f64, f64, f64)) -> Option<(f64, usize)> {
    let thickness = sc.slice_thickness?;
    let vs = [voxel_size.0, voxel_size.1, voxel_size.2];
    let axis = match sc.slice_encoding_direction.as_deref().map(|d| d.trim_end_matches('-')) {
        Some("i") => 0,
        Some("j") => 1,
        Some("k") => 2,
        _ => (0..3).rev().max_by(|&a, &b| vs[a].total_cmp(&vs[b])).unwrap_or(2),
    };
    let is_2d = sc.mr_acquisition_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("2D"));
    let differs = (thickness - vs[axis]).abs() > 0.01 * vs[axis];
    (is_2d || differs).then_some((thickness, axis))
}

/// `(thickness mm, voxel axis)` of an acquisition's slices ([`RunMetadata::slice_geometry`]).
pub type SliceGeometry = (f64, usize);

/// Refuse a gapped acquisition before any work, unless `--ignore-slice-gap` (or nothing that
/// needs a contiguous grid was asked for). Checked on the acquired grid (`dims`, `voxel_size`),
/// so it holds whether or not the run is then resampled. Returns the geometry to hand qsm-core
/// (on the working grid `meta`), and the decision to reconstruct across a gap, when there is one.
pub fn check_slice_gap(
    sc: &QsmSidecar, dims: (usize, usize, usize), voxel_size: (f64, f64, f64), meta: &RunMetadata,
    config: &PipelineConfig,
) -> Result<(Option<SliceGeometry>, Option<Decision>), EngineError> {
    let Some((thickness, axis)) = slice_geometry_from_sidecar(sc, voxel_size) else { return Ok((None, None)) };
    let (nx, ny, nz) = dims;
    let (vx, vy, vz) = voxel_size;
    let grid = qsm_core::Grid::new(nx, ny, nz, vx, vy, vz);
    if let Err(gap) = grid.require_contiguous_slices(thickness, axis) {
        if config.pipeline.ignore_slice_gap {
            log::warn!("{gap}; reconstructing anyway (--ignore-slice-gap): treat the result as unreliable");
            return Ok((None, Some(Decision::new(Topic::SliceGap, Choice::Ignored,
                "The slices are separated by a gap, and were reconstructed as if contiguous.").with_detail(gap.to_string()))));
        }
        if config.pipeline.do_qsm {
            return Err(EngineError::Config(format!(
                "{gap} (from the sidecar's SliceThickness). QSM needs contiguous slices; pass \
                 --ignore-slice-gap to reconstruct anyway")));
        }
        log::warn!("{gap}; no QSM was asked for, so continuing");
        return Ok((None, None));
    }
    // Contiguous: hand the geometry on, unless resampling moved the slices off the grid's axes.
    Ok((if meta.source_geometry.is_none() { Some((thickness, axis)) } else { None }, None))
}


/// What [`resolve_geometry`] decided about the grid of a run on `meta`, where there was a choice:
/// resampled to axial or kept as acquired when `--axial-resampling` is on or an algorithm the run
/// uses needs axial data, and nothing otherwise.
pub fn grid_decision(meta: &RunMetadata, facts: &ScanFacts, config: &PipelineConfig) -> Option<Decision> {
    let acquired_affine = meta.source_geometry.map_or(meta.affine, |(_, a)| a);
    let axial_only = crate::bridge::axial_only_algorithms(config, orientation_facts(facts, config));
    if !config.pipeline.axial_resampling && axial_only.is_empty() {
        return None;
    }
    let obliquity = qsm_core::geometry::obliquity_from_affine(&acquired_affine);
    let tilt = qsm_core::geometry::b0_angle_from_affine(&acquired_affine);
    let detail = format!("{obliquity:.1}° oblique, B0 {tilt:.1}° from the slice normal");
    Some(if meta.source_geometry.is_none() {
        Decision::new(Topic::Grid, Choice::Acquired, "Processed on the acquired grid.").with_detail(detail)
    } else if !axial_only.is_empty() && tilt > AXIAL_ONLY_TOLERANCE_DEG {
        Decision::new(Topic::Grid, Choice::ResampledForAlgorithm,
            "Resampled onto an axial grid, as an algorithm the run uses assumes B0 along the slice normal.")
            .with_detail(format!("{detail}; {}", axial_only.join(", ")))
    } else {
        Decision::new(Topic::Grid, Choice::Resampled, "Resampled onto an axial grid, the acquisition being oblique.")
            .with_detail(detail)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar(thickness: Option<f64>, acq: Option<&str>, dir: Option<&str>) -> QsmSidecar {
        QsmSidecar {
            echo_time: 0.004, magnetic_field_strength: 3.0, b0_dir: None,
            slice_thickness: thickness, mr_acquisition_type: acq.map(str::to_string),
            slice_encoding_direction: dir.map(str::to_string), manufacturer: None,
        }
    }

    #[test]
    fn slice_geometry_is_read_only_where_a_gap_can_be_checked() {
        let vs = (0.8, 0.8, 3.0);
        assert_eq!(slice_geometry_from_sidecar(&sidecar(Some(2.0), Some("2D"), None), vs), Some((2.0, 2)));
        assert_eq!(slice_geometry_from_sidecar(&sidecar(Some(0.8), Some("2D"), Some("j-")), vs), Some((0.8, 1)));
        // 3D: the partition thickness is the pitch; nothing to check.
        assert_eq!(slice_geometry_from_sidecar(&sidecar(Some(3.0), Some("3D"), None), vs), None);
        // No SliceThickness: nothing can be said.
        assert_eq!(slice_geometry_from_sidecar(&sidecar(None, Some("2D"), None), vs), None);
        // A thickness that is not the pitch is checked even without MRAcquisitionType.
        assert_eq!(slice_geometry_from_sidecar(&sidecar(Some(2.0), None, None), vs), Some((2.0, 2)));
    }

    #[test]
    fn facts_are_read_from_the_echoes_sidecars() {
        let echo = |te: f64, dir: Option<Vec<f64>>| QsmSidecar { echo_time: te, b0_dir: dir, ..sidecar(None, None, None) };
        let facts = ScanFacts::from_sidecars(
            (4, 5, 6), &[echo(0.004, Some(vec![0.0, 0.1, 0.99])), echo(0.008, Some(vec![1.0]))], true).unwrap();
        assert_eq!((facts.dims, facts.n_echoes, facts.has_magnitude), ((4, 5, 6), 2, true));
        assert_eq!(facts.echo_times, [0.004, 0.008]);
        assert_eq!(facts.field_strength, 3.0);
        // A B0_dir without three components is ignored; the last usable one stands.
        assert_eq!(facts.b0_dir, Some((0.0, 0.1, 0.99)));
        assert_eq!((facts.n_coils, facts.real_imaginary, facts.custom_mask), (1, false, false));
        assert!(ScanFacts::from_sidecars((4, 5, 6), &[], true).is_err());
    }

    #[test]
    fn a_sidecar_is_read_from_its_json() {
        let v = serde_json::json!({
            "EchoTime": 0.01, "MagneticFieldStrength": 7.0, "B0_dir": [0.0, 0.0, 1.0],
            "MRAcquisitionType": "2D", "Manufacturer": "GE",
        });
        let sc = QsmSidecar::from_json(&v).unwrap();
        assert_eq!((sc.echo_time, sc.magnetic_field_strength), (0.01, 7.0));
        assert_eq!(sc.b0_dir, Some(vec![0.0, 0.0, 1.0]));
        assert_eq!(sc.mr_acquisition_type.as_deref(), Some("2D"));
        assert_eq!(sc.manufacturer.as_deref(), Some("GE"));
        assert!(sc.slice_thickness.is_none());
        assert_eq!(QsmSidecar::from_json(&serde_json::json!({"MagneticFieldStrength": 3})).unwrap_err(), "EchoTime");
        assert_eq!(QsmSidecar::from_json(&serde_json::json!({"EchoTime": 0.02})).unwrap_err(),
                   "MagneticFieldStrength");
    }
}
