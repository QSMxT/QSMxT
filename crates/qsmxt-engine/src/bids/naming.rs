//! The names of everything QSMxT writes: one vocabulary for the pipeline and every standalone
//! command (#275).
//!
//! A name is a prefix (a run's BIDS basename in the pipeline, the `--output` prefix of a standalone
//! command) followed by a [`Tail`]: the entities a derivative adds (`echo-`, `part-`, `space-`,
//! `desc-`, ...) and its suffix. [`compose`] joins the two. When the prefix is itself made of BIDS
//! entities (`sub-01_ses-pre_run-1`), the result lists every entity in the order the BIDS spec
//! gives them, so a tail entity lands where it belongs rather than after the prefix; otherwise the
//! tail is appended as is.
//!
//! The tails are built only here — by [`outputs`](super::outputs) and `qsmxt`'s derivative paths
//! for the pipeline and directly by `qsmxt invert sti`, `separate`, `combine` and `swi --mip` — so
//! a standalone command and the pipeline cannot name the same thing two ways.
//!
//! Names that are not (yet) BIDS are kept on purpose and listed in the derivatives' `.bidsignore`
//! ([`BIDSIGNORED_SUFFIXES`]): `R2primemap` (no BIDS suffix for R2′), `swi`/`minIP`/`smwi` (BEP004's
//! proposed SWI suffixes) and `evec` (no BIDS suffix for an orientation field).

/// BIDS entities in the order the specification lists them (BIDS 1.10, "Entity table").
pub const ENTITY_ORDER: [&str; 29] = [
    "sub", "ses", "sample", "task", "tracksys", "acq", "nuc", "voi", "ce", "trc", "stain", "rec",
    "dir", "run", "mod", "echo", "flip", "inv", "mt", "part", "proc", "hemi", "space", "seg", "res",
    "den", "label", "split", "desc",
];

/// Suffixes QSMxT publishes that the released BIDS schema does not define, each kept deliberately
/// and listed in `.bidsignore` so the derivatives validate.
pub const BIDSIGNORED_SUFFIXES: [&str; 5] = ["R2primemap", "swi", "minIP", "smwi", "evec"];

/// The label of the `space-` entity a run's outputs carry when they are left on the axial working
/// grid it was resampled to (`--output-space working`). Outputs on the acquired grid carry none.
pub const WORKING_SPACE: &str = "axial";

/// What a derivative adds to its prefix: entities, then a suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tail {
    entities: Vec<(&'static str, String)>,
    suffix: String,
}

impl Tail {
    pub fn new(suffix: &str) -> Self {
        Self { entities: Vec::new(), suffix: suffix.to_string() }
    }

    /// Set an entity, replacing any value it had.
    pub fn with(mut self, entity: &'static str, value: impl Into<String>) -> Self {
        debug_assert!(ENTITY_ORDER.contains(&entity), "{entity} is not a BIDS entity");
        let value = value.into();
        match self.entities.iter_mut().find(|(e, _)| *e == entity) {
            Some(slot) => slot.1 = value,
            None => self.entities.push((entity, value)),
        }
        self
    }

    /// The same tail with a `space-` entity, or unchanged for `None`.
    pub fn in_space(self, space: Option<&str>) -> Self {
        match space {
            Some(s) => self.with("space", s),
            None => self,
        }
    }

    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// The tail on its own, entities in BIDS order: `echo-1_part-phase_desc-scaled_MEGRE`.
    pub fn render(&self) -> String {
        let mut parts: Vec<String> = sorted(self.entities.iter().map(|(e, v)| (e.to_string(), v.clone())))
            .into_iter().map(|(e, v)| format!("{e}-{v}")).collect();
        parts.push(self.suffix.clone());
        parts.join("_")
    }
}

fn rank(entity: &str) -> usize {
    ENTITY_ORDER.iter().position(|e| *e == entity).unwrap_or(ENTITY_ORDER.len())
}

fn sorted(it: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = it.collect();
    v.sort_by_key(|(e, _)| rank(e));
    v
}

/// The prefix's entities, when every `_`-separated part of it is one (`sub-01_run-2`); `None` for
/// a free-form prefix (`out`, `my_scan`).
fn prefix_entities(prefix: &str) -> Option<Vec<(String, String)>> {
    prefix.split('_').map(|seg| {
        let (e, v) = seg.split_once('-')?;
        (ENTITY_ORDER.contains(&e) && !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric()))
            .then(|| (e.to_string(), v.to_string()))
    }).collect()
}

/// `prefix` + `tail`, without an extension. See the module documentation.
pub fn compose(prefix: &str, tail: &Tail) -> String {
    if prefix.is_empty() {
        return tail.render();
    }
    match prefix_entities(prefix) {
        Some(mut entities) => {
            for (e, v) in &tail.entities {
                match entities.iter_mut().find(|(pe, _)| pe == e) {
                    Some(slot) => slot.1 = v.clone(),
                    None => entities.push((e.to_string(), v.clone())),
                }
            }
            let mut parts: Vec<String> = sorted(entities.into_iter())
                .into_iter().map(|(e, v)| format!("{e}-{v}")).collect();
            parts.push(tail.suffix.clone());
            parts.join("_")
        }
        None => format!("{prefix}_{}", tail.render()),
    }
}

/// The NIfTI file `prefix` + `tail` names, next to the prefix: the standalone commands' outputs.
pub fn nifti_beside(prefix: &std::path::Path, tail: &Tail) -> std::path::PathBuf {
    let base = prefix.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    prefix.with_file_name(format!("{}.nii", compose(&base, tail)))
}

/// The value of `entity` in a file name (`sub-01_space-axial_Chimap.nii` → `space` → `axial`).
pub fn entity_of(file_name: &str, entity: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".gz").unwrap_or(file_name);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    let prefix = format!("{entity}-");
    stem.split('_').find_map(|seg| seg.strip_prefix(&prefix).map(str::to_string))
}

// ─── The vocabulary ───

/// The susceptibility map.
pub fn chimap() -> Tail { Tail::new("Chimap") }
/// A `Chimap` with a `desc-` (`singlepass`, `cosmos`, the chi-separation sources, ...).
pub fn chimap_desc(desc: &str) -> Tail { chimap().with("desc", desc) }
/// χ+ / χ− / χ from chi-separation.
pub fn chi_paramagnetic() -> Tail { chimap_desc("paramagnetic") }
pub fn chi_diamagnetic() -> Tail { chimap_desc("diamagnetic") }
pub fn chi_total() -> Tail { chimap_desc("total") }

/// The STI tensor: one 4D `Chimap`, its six independent components in [`STI_COMPONENTS`] order
/// along the fourth axis (recorded in the sidecar as `TensorComponents`).
///
/// One file rather than six because the components are one quantity — a symmetric tensor in ppm
/// — and BIDS has no tensor suffix in `anat`; `Chimap` is the suffix for a susceptibility in ppm
/// and says nothing about the number of volumes.
pub fn sti_tensor() -> Tail { chimap_desc("sti") }
/// The order of the tensor's components in [`sti_tensor`], as QSM.rs stores them.
pub const STI_COMPONENTS: [&str; 6] = ["11", "12", "13", "22", "23", "33"];
/// Mean magnetic susceptibility and magnetic susceptibility anisotropy: scalar ppm maps.
pub fn sti_mms() -> Tail { chimap_desc("mms") }
pub fn sti_msa() -> Tail { chimap_desc("msa") }
/// The tensor's principal eigenvector: one 4D unit-vector field (x, y, z along the fourth axis).
///
/// Not a `Chimap`: an eigenvector is a direction, not a susceptibility. BIDS has no suffix for an
/// orientation field in `anat`, so it takes `evec` (the name BEP016's diffusion derivatives use for
/// tensor eigenvectors) and is listed in `.bidsignore`.
pub fn sti_eigenvector() -> Tail { Tail::new("evec").with("desc", "sti") }

/// The brain mask, and masks with a `desc-` (`qsm`, `reliable`, `r2prime`, `motioncoverage`, ...).
pub fn mask() -> Tail { Tail::new("mask") }
pub fn mask_desc(desc: &str) -> Tail { mask().with("desc", desc) }

/// The combined magnitude, a T2*-weighted image (`part-mag_T2starw`).
pub fn magnitude() -> Tail { Tail::new("T2starw").with("part", "mag") }

/// SWI and its minimum-intensity projection, under BEP004's proposed suffixes (`.bidsignore`d).
pub fn swi() -> Tail { Tail::new("swi") }
pub fn swi_minip() -> Tail { Tail::new("minIP") }
/// SMWI per contrast (`paramagnetic`/`diamagnetic`, as chi-separation splits), on the SWI scheme:
/// its own `smwi` suffix, and its projection a `minIP` whose `desc-` names the image projected
/// (`desc-paramagneticsmwi_minIP`), so it cannot be mistaken for the SWI's.
pub fn smwi(contrast: &str) -> Tail { Tail::new("smwi").with("desc", contrast) }
pub fn smwi_minip(contrast: &str) -> Tail { swi_minip().with("desc", format!("{contrast}smwi")) }

pub fn dseg() -> Tail { Tail::new("dseg") }
pub fn t2starmap() -> Tail { Tail::new("T2starmap") }
pub fn r2starmap() -> Tail { Tail::new("R2starmap") }
pub fn r2map() -> Tail { Tail::new("R2map") }
/// R2′ = R2* − R2. No BIDS suffix exists for it; kept as `R2primemap` and `.bidsignore`d.
pub fn r2primemap() -> Tail { Tail::new("R2primemap") }

/// The total (unwrapped, echo-combined) field in Hz — a BIDS `fieldmap`, which allows Hz, rad/s
/// or T but not ppm.
pub fn total_fieldmap() -> Tail { Tail::new("fieldmap").with("desc", "total") }
/// The magnitude BIDS requires beside a `fieldmap` (the first echo's), named to pair with it.
pub fn total_fieldmap_magnitude() -> Tail { Tail::new("magnitude").with("desc", "total") }
/// A run's prefix for its `fmap/` files: BIDS allows only `sub`, `ses`, `acq`, `run` (and the
/// derivative entities) on a `fieldmap`, so the run's `rec-` and `inv-` labels are folded into its
/// `acq-` label (`acq-swi` + `rec-uncombined` → `acq-swirecuncombined`), keeping two runs that
/// differ only by those apart.
pub fn fmap_key(key: &crate::bids::entities::AcquisitionKey) -> crate::bids::entities::AcquisitionKey {
    if key.reconstruction.is_none() && key.inversion.is_none() {
        return key.clone();
    }
    let mut acq = key.acquisition.clone().unwrap_or_default();
    if let Some(rec) = &key.reconstruction { acq.push_str(&format!("rec{rec}")); }
    if let Some(inv) = &key.inversion { acq.push_str(&format!("inv{inv}")); }
    crate::bids::entities::AcquisitionKey {
        acquisition: Some(acq), reconstruction: None, inversion: None, ..key.clone()
    }
}
/// The suffixes published under `fmap/` rather than `anat/`.
pub const FMAP_SUFFIXES: [&str; 2] = ["fieldmap", "magnitude"];
/// The same field in ppm, the unit the inversions take (a workflow intermediate).
pub fn total_fieldmap_ppm() -> Tail { Tail::new("fieldmap").with("desc", "totalppm") }
/// QSMART's echo-fit reliability map, kept beside the total field (a workflow intermediate).
pub fn echofit_reliability_mask() -> Tail { mask_desc("echofit") }
/// A susceptibility map before referencing (a workflow intermediate).
pub fn unreferenced_chimap() -> Tail { chimap_desc("unreferenced") }

/// One of QSMART's intermediates, by the name QSM.rs reports it under
/// (`qsm_core::pipeline::qsmart::outputs`): the vessel mask (`desc-vessels_mask`), and each
/// stage's local field (`desc-stage1_localfield`, `desc-stage2_localfield`) and unreferenced
/// susceptibility map (`desc-stage1_Chimap`, `desc-stage2_Chimap`). `None` for a name QSMxT does
/// not know.
pub fn qsmart_intermediate(name: &str) -> Option<Tail> {
    use qsm_core::pipeline::qsmart::outputs;
    Some(match name {
        crate::stages::qsmart::VESSEL_MAGNITUDE => magnitude().with("desc", "vessels"),
        outputs::VESSEL_MASK => mask_desc("vessels"),
        outputs::LOCAL_FIELD_STAGE1 => Tail::new("localfield").with("desc", "stage1"),
        outputs::CHI_STAGE1 => chimap_desc("stage1"),
        outputs::LOCAL_FIELD_STAGE2 => Tail::new("localfield").with("desc", "stage2"),
        outputs::CHI_STAGE2 => chimap_desc("stage2"),
        _ => return None,
    })
}

/// One echo of a MCPC-3D-S coil combination (`rec-mcpc3ds_echo-N_part-{phase,mag}_<suffix>`).
pub fn combined_echo(echo: usize, part: &str, suffix: &str) -> Tail {
    Tail::new(suffix).with("rec", "mcpc3ds").with("echo", echo.to_string()).with("part", part)
}
/// The mask MCPC-3D-S estimated its phase offsets on.
pub fn mcpc3ds_mask() -> Tail { mask_desc("mcpc3ds") }

/// One echo's phase scaled to radians, and its magnitude (workflow intermediates).
pub fn scaled_phase_echo(echo: usize, suffix: &str) -> Tail {
    Tail::new(suffix).with("echo", echo.to_string()).with("part", "phase").with("desc", "scaled")
}
pub fn magnitude_echo(echo: usize, suffix: &str) -> Tail {
    Tail::new(suffix).with("echo", echo.to_string()).with("part", "mag")
}

/// Where `qsmxt swi --mip` puts the projection when `--mip-output` is not given: the SWI's name
/// with its `swi` suffix replaced by `minIP` (`sub-01_swi.nii` → `sub-01_minIP.nii`, as the pipeline
/// names them), or `minIP` appended to a name without it (`out.nii` → `out_minIP.nii`).
pub fn minip_beside_swi(swi_path: &std::path::Path) -> std::path::PathBuf {
    let name = swi_path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = name.strip_suffix(".nii.gz").or_else(|| name.strip_suffix(".nii")).unwrap_or(&name);
    let prefix = stem.strip_suffix(&format!("_{}", swi().suffix())).unwrap_or(stem);
    swi_path.with_file_name(format!("{}.nii", compose(prefix, &swi_minip())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_entities_land_in_bids_order() {
        assert_eq!(compose("sub-01_run-2", &combined_echo(1, "phase", "MEGRE")),
                   "sub-01_rec-mcpc3ds_run-2_echo-1_part-phase_MEGRE");
        assert_eq!(compose("sub-01_run-1_inv-2", &magnitude().in_space(Some("axial"))),
                   "sub-01_run-1_inv-2_part-mag_space-axial_T2starw");
        assert_eq!(compose("sub-01", &mask_desc("qsm").in_space(Some("axial"))),
                   "sub-01_space-axial_desc-qsm_mask");
        // A tail entity replaces the prefix's (the combined echoes replace `rec-uncombined`).
        assert_eq!(compose("sub-1_rec-uncombined", &combined_echo(2, "mag", "MEGRE")),
                   "sub-1_rec-mcpc3ds_echo-2_part-mag_MEGRE");
    }

    #[test]
    fn a_free_form_prefix_is_kept_and_the_tail_appended() {
        assert_eq!(compose("out", &chi_paramagnetic()), "out_desc-paramagnetic_Chimap");
        assert_eq!(compose("my_scan", &sti_tensor()), "my_scan_desc-sti_Chimap");
        assert_eq!(compose("", &swi()), "swi");
    }

    #[test]
    fn every_name_is_entity_safe() {
        // No hyphenated suffixes and no unknown "entities": everything before the suffix parses as
        // a known BIDS entity, and desc labels are alphanumeric.
        let tails = [
            chimap(), chi_paramagnetic(), chi_diamagnetic(), chi_total(), sti_tensor(), sti_mms(),
            sti_msa(), sti_eigenvector(), mask(), mask_desc("qsm"), magnitude(), swi(), swi_minip(),
            smwi("paramagnetic"), smwi_minip("diamagnetic"), dseg(), t2starmap(), r2starmap(),
            r2map(), r2primemap(), total_fieldmap(), total_fieldmap_magnitude(), total_fieldmap_ppm(), unreferenced_chimap(),
            combined_echo(3, "phase", "MEGRE"), mcpc3ds_mask(), scaled_phase_echo(2, "MEGRE"),
            magnitude_echo(2, "GRE"),
        ];
        for t in tails {
            let name = compose("sub-01", &t);
            let mut segs: Vec<&str> = name.split('_').collect();
            let suffix = segs.pop().unwrap();
            assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric()), "{name}: suffix {suffix}");
            for s in segs {
                let (e, v) = s.split_once('-').unwrap_or_else(|| panic!("{name}: {s} is not an entity"));
                assert!(ENTITY_ORDER.contains(&e), "{name}: {e} is not a BIDS entity");
                assert!(v.chars().all(|c| c.is_ascii_alphanumeric()), "{name}: {s}");
            }
        }
    }

    #[test]
    fn swi_minip_default_matches_the_pipeline() {
        use std::path::{Path, PathBuf};
        assert_eq!(minip_beside_swi(Path::new("/o/sub-01_swi.nii")), PathBuf::from("/o/sub-01_minIP.nii"));
        assert_eq!(minip_beside_swi(Path::new("/o/out.nii")), PathBuf::from("/o/out_minIP.nii"));
        assert_eq!(minip_beside_swi(Path::new("out.nii.gz")), PathBuf::from("out_minIP.nii"));
    }

    #[test]
    fn entity_of_reads_a_value() {
        assert_eq!(entity_of("sub-01_space-axial_Chimap.nii", "space").as_deref(), Some("axial"));
        assert_eq!(entity_of("sub-01_Chimap.nii", "space"), None);
    }

    /// Every intermediate QSMART reports has a name of its own, and an unknown one has none.
    #[test]
    fn every_qsmart_intermediate_is_named_once() {
        use qsm_core::pipeline::qsmart::outputs;
        let names: Vec<String> = [outputs::VESSEL_MASK, outputs::LOCAL_FIELD_STAGE1, outputs::CHI_STAGE1,
                                  outputs::LOCAL_FIELD_STAGE2, outputs::CHI_STAGE2]
            .iter().map(|n| compose("sub-01", &qsmart_intermediate(n).unwrap())).collect();
        assert_eq!(names, ["sub-01_desc-vessels_mask", "sub-01_desc-stage1_localfield", "sub-01_desc-stage1_Chimap",
                           "sub-01_desc-stage2_localfield", "sub-01_desc-stage2_Chimap"]);
        assert!(qsmart_intermediate("something_else").is_none());
    }
}
