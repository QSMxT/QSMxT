use crate::pipeline::runner::*;

const DIMS: (usize, usize, usize) = (8, 8, 8);
const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

pub(super) fn meta() -> RunMetadata {
    RunMetadata {
        dims: DIMS, voxel_size: (1.0, 1.0, 1.0), affine: IDENTITY, n_echoes: 1,
        echo_times: vec![0.004], b0_direction: (0.0, 0.0, 1.0), field_strength: 3.0,
        has_magnitude: true, source_geometry: None, slice_geometry: None, orientation: None,
    }
}

/// A box `lo..hi` on every axis.
fn cube(lo: usize, hi: usize) -> Vec<u8> {
    let (nx, ny, nz) = DIMS;
    let mut m = vec![0u8; nx * ny * nz];
    for k in lo..hi {
        for j in lo..hi {
            for i in lo..hi {
                m[i + j * nx + k * nx * ny] = 1;
            }
        }
    }
    m
}

pub(super) struct Fixture {
    _dir: tempfile::TempDir,
    pub(super) output: DerivativeOutputs,
    pub(super) run: QsmRun,
    pub(super) brain: Vec<u8>,
    pub(super) support: Vec<u8>,
    pub(super) chi_raw: Vec<f64>,
}

/// A brain mask of 6³ voxels, of which background removal kept the inner 4³: the raw map is
/// non-zero (and offset from zero by 0.05 ppm) on the support and exactly 0 on the eroded rim.
pub(super) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let output = DerivativeOutputs::new(&dir.path().join("out"));
    let mut run = super::run_with_echoes(vec![(dir.path().join("p.nii"), None)]);
    run.dims = DIMS;
    let (brain, support) = (cube(1, 7), cube(2, 6));
    let chi_raw: Vec<f64> = support.iter().enumerate()
        .map(|(i, &s)| if s != 0 { 0.05 + 0.001 * (i % 7) as f64 } else { 0.0 })
        .collect();
    let m = meta();
    save_mask(&output.mask_path(&run.key), &brain, &m).unwrap();
    save_mask(&output.bg_mask_path(&run.key), &support, &m).unwrap();
    save_volume(&output.chi_raw_path(&run.key), &chi_raw, &m).unwrap();
    // Compare against what is on disk, in whatever precision the NIfTI holds it.
    let chi_raw = load_volume(&output.chi_raw_path(&run.key)).unwrap();
    Fixture { _dir: dir, output, run, brain, support, chi_raw }
}

fn mean_over(chi: &[f64], mask: &[u8]) -> f64 {
    let (s, n) = chi.iter().zip(mask).filter(|(_, &m)| m != 0)
        .fold((0.0, 0usize), |(s, n), (&c, _)| (s + c, n + 1));
    s / n as f64
}

/// Run the single-pass `reference` step over the fixture with `config`.
pub(super) fn reference(f: &Fixture, config: &PipelineConfig) -> Vec<f64> {
    run_engine_step(&f.run, config, &f.output, &meta(), Stage::Reference).unwrap();
    load_volume(&f.output.qsm_path(&f.run.key)).unwrap()
}

fn sidecar_offset(f: &Fixture) -> f64 {
    let sidecar = crate::bids::entities::sidecar_path(&f.output.qsm_path(&f.run.key)).unwrap();
    read_json_object(&sidecar)["QsmReferenceOffsetPpm"].as_f64().unwrap()
}

fn rim(f: &Fixture) -> impl Iterator<Item = usize> + '_ {
    (0..f.brain.len()).filter(|&i| f.brain[i] != 0 && f.support[i] == 0)
}

#[test]
fn mean_reference_is_measured_over_the_support_and_the_rim_stays_zero() {
    let f = fixture();
    let out = reference(&f, &PipelineConfig::default());

    let true_mean = mean_over(&f.chi_raw, &f.support);
    let brain_mean = mean_over(&f.chi_raw, &f.brain);
    // The fixture has a real rim, and the brain-mask mean really is biased by it.
    assert!(rim(&f).count() > 0);
    assert!((true_mean - brain_mean).abs() > 0.01, "{true_mean} vs {brain_mean}");

    for i in rim(&f) {
        assert_eq!(out[i], 0.0, "eroded rim voxel {i} must be 0, not -offset");
    }
    for (i, &s) in f.support.iter().enumerate() {
        if s != 0 {
            assert!((out[i] - (f.chi_raw[i] - true_mean)).abs() < 1e-6, "voxel {i}");
        }
    }
    assert!(mean_over(&out, &f.support).abs() < 1e-6, "the support's mean is the zero");
    assert!((sidecar_offset(&f) - true_mean).abs() < 1e-9, "the recorded offset is the support mean");

    // The support is published, and it is exactly where the map is defined.
    assert_eq!(load_mask(&f.output.qsm_support_path(&f.run.key)).unwrap(), f.support);
}

#[test]
fn region_reference_is_measured_only_where_the_region_meets_the_support() {
    let f = fixture();
    let mut config = PipelineConfig::default();
    config.qsm.reference = QsmReference::Region;
    config.qsm.reference_region = Some("left-thalamus".into());
    let ids = qsmxt_engine::regions::resolve("left-thalamus", config.segmentation.version).unwrap();

    // The region covers the lower half of the brain: half of it lies in the eroded rim, which
    // is where a small deep structure (the accumbens) lost about half its voxels in practice.
    let nxy = DIMS.0 * DIMS.1;
    let roi: Vec<u8> = f.brain.iter().enumerate()
        .map(|(i, &b)| u8::from(b != 0 && i / nxy < 4)).collect();
    let dseg: Vec<f64> = roi.iter().map(|&r| if r != 0 { ids[0] as f64 } else { 0.0 }).collect();
    save_volume(&f.output.dseg_path(&f.run.key), &dseg, &meta()).unwrap();

    let out = reference(&f, &config);
    let in_both: Vec<u8> = roi.iter().zip(&f.support).map(|(&r, &s)| r & s).collect();
    let expected = mean_over(&f.chi_raw, &in_both);
    assert!((expected - mean_over(&f.chi_raw, &roi)).abs() > 0.01, "the rim would bias it");
    assert!((sidecar_offset(&f) - expected).abs() < 1e-9);
    for i in rim(&f) {
        assert_eq!(out[i], 0.0, "eroded rim voxel {i} must be 0");
    }
    assert!(mean_over(&out, &in_both).abs() < 1e-6);
}

/// `none` never left a shell — the raw rim is already 0 — and must keep not doing so.
/// Each map's rows in the statistics table carry that map's own reference (#280): the two-pass
/// single-pass map has its own offset, and the χ-separation maps, never referenced, have none.
#[test]
fn each_map_carries_its_own_reference_in_the_table() {
    let f = fixture();
    let config = PipelineConfig::default();
    let m = meta();
    reference(&f, &config);
    // The single-pass map: a different raw map, so a different offset.
    let raw2: Vec<f64> = f.chi_raw.iter().map(|&c| if c != 0.0 { c + 0.02 } else { 0.0 }).collect();
    save_volume(&f.output.chi_raw_path(&f.run.key), &raw2, &m).unwrap();
    run_engine_step(&f.run, &config, &f.output, &m, Stage::SinglePassReference).unwrap();
    // χ-separation maps, which are never referenced.
    for p in [f.output.chi_para_path(&f.run.key), f.output.chi_dia_path(&f.run.key),
              f.output.chi_sep_total_path(&f.run.key)] {
        save_volume(&p, &f.chi_raw, &m).unwrap();
    }
    let dseg: Vec<f64> = f.brain.iter().map(|&b| if b != 0 { 10.0 } else { 0.0 }).collect();
    let dseg_path = f.output.dseg_path(&f.run.key);
    save_volume(&dseg_path, &dseg, &m).unwrap();
    run_engine_step(&f.run, &config, &f.output, &m, Stage::Analysis).unwrap();

    let offset_of = |path: PathBuf| -> f64 {
        read_json_object(&crate::bids::entities::sidecar_path(&path).unwrap())
            ["QsmReferenceOffsetPpm"].as_f64().unwrap()
    };
    let main_off = offset_of(f.output.qsm_path(&f.run.key));
    let single_off = offset_of(f.output.singlepass_qsm_path(&f.run.key));
    assert!((main_off - single_off).abs() > 0.01, "{main_off} vs {single_off}");

    let table = fs_err::read_to_string(f.output.segmentation_stats_path(&f.run.key)).unwrap();
    let rows = stats::parse_tsv(&table);
    let by_map = |name: &str| rows.iter().filter(|r| r.map == name).collect::<Vec<_>>();
    for r in by_map("Chimap") {
        assert_eq!(r.reference.as_deref(), Some("mean"));
        assert!((r.reference_offset_ppm.unwrap() - main_off).abs() < 1e-6);
    }
    let single = by_map("desc-singlepass_Chimap");
    assert!(!single.is_empty());
    for r in single {
        assert_eq!(r.reference.as_deref(), Some("mean"));
        assert!((r.reference_offset_ppm.unwrap() - single_off).abs() < 1e-6, "{r:?}");
    }
    for name in ["desc-paramagnetic_Chimap", "desc-diamagnetic_Chimap", "desc-total_Chimap"] {
        let rows = by_map(name);
        assert!(!rows.is_empty(), "{name} was not summarised");
        for r in rows {
            assert_eq!((r.reference.as_deref(), r.reference_offset_ppm), (None, None), "{r:?}");
        }
    }
}

#[test]
fn no_reference_leaves_the_rim_at_zero_and_the_support_untouched() {
    let f = fixture();
    let mut config = PipelineConfig::default();
    config.qsm.reference = QsmReference::None;
    let out = reference(&f, &config);
    for (i, (&o, (&s, &c))) in out.iter().zip(f.support.iter().zip(&f.chi_raw)).enumerate() {
        let expected = if s != 0 { c } else { 0.0 };
        assert!((o - expected).abs() < 1e-6, "voxel {i}");
    }
}
