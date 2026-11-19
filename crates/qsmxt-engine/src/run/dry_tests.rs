//! Dry runs ([`dry_run`]): what a run would do, checked against what it then does.

use super::tests::*;
use super::*;
use crate::enums::QsmAlgorithm;
use crate::masking::MaskOp;
use crate::observe::VolumeData;
use crate::store::MemoryStore;

/// The states of the steps named, in the dry run's answer.
fn states<'a>(expected: &'a [(String, ExpectedState)], steps: &[&str]) -> Vec<(&'a str, ExpectedState)> {
    expected.iter().filter(|(id, _)| steps.contains(&id.as_str())).map(|(id, s)| (id.as_str(), *s)).collect()
}

/// Before the first run every step will run; after it every step is cached; a new inversion
/// threshold changes the inversion and makes referencing stale; a new mask makes everything
/// after it stale; new input data changes the load step.
#[test]
fn a_dry_run_says_what_the_run_then_does() {
    use ExpectedState::*;
    let (facts, mut store) = phantom();
    let mut config = config();
    let first = expect_then_run(&config, &facts, &mut store);
    assert!(first.iter().all(|(_, s)| matches!(s, WillRun | Skipped)), "{first:?}");
    let again = expect_then_run(&config, &facts, &mut store);
    assert!(again.iter().all(|(_, s)| matches!(s, Cached | Skipped)), "{again:?}");

    config.inversion.tkd.threshold *= 2.0;
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(states(&e, &["bgremove", "invert", "reference"]), [("bgremove", Cached), ("invert", Changed), ("reference", Stale)]);

    erode_the_brain_mask(&mut config);
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(states(&e, &["magnitude", "mask", "unwrap", "bgremove", "invert", "reference"]),
               [("magnitude", Cached), ("mask", Changed), ("unwrap", Stale), ("bgremove", Stale), ("invert", Stale), ("reference", Stale)]);

    store.set_identity(json!("phantom-2"));
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(e[0], ("load".to_string(), Changed));
    assert!(e[1..].iter().all(|(_, s)| matches!(s, Stale | Skipped)), "{e:?}");
}

/// A configuration as a front end holds it, with only the outputs asked for, is dry-run as
/// its normalised configuration is (what `qsmxt run` takes): the same steps under the same
/// keys, the outputs those need included.
#[test]
fn a_dry_run_of_a_configuration_as_given_is_that_of_the_normalised_one() {
    let (facts, store) = phantom();
    type Edit = fn(&mut PipelineConfig);
    let cases: [(Edit, &str); 6] = [
        (|c| c.pipeline.do_r2primemap = true, "t2star_r2star"),
        (|c| c.pipeline.do_analysis = true, "segmentation"),
        (|c| { c.pipeline.do_qsm = false; c.pipeline.do_smwi = true; }, "invert"),
        (|c| { c.pipeline.do_chi_separation = true; c.separation.algorithm = crate::enums::SeparationAlgorithm::R2starQsm; },
         "t2star_r2star"),
        (|c| { c.qsm.reference = crate::enums::QsmReference::Region; c.qsm.reference_region = Some("thalamus".into()); },
         "segmentation"),
        // MEDI's PDF: the background removal the run takes is keyed as such.
        (|c| c.inversion.algorithm = crate::enums::QsmAlgorithm::Medi, "bgremove"),
    ];
    for (edit, needed) in cases {
        let mut raw = config();
        edit(&mut raw);
        let normalised = raw.normalised().0;
        let given = dry_run(&crate::plan::plan(&raw, &facts), &raw, &facts, &store).unwrap();
        let taken = dry_run(&crate::plan::plan(&normalised, &facts), &normalised, &facts, &store).unwrap();
        assert_eq!(given, taken);
        assert!(given.iter().any(|e| e.id == needed && e.state == ExpectedState::WillRun), "{needed}: {given:?}");
    }
}

/// A switch to a deep-learning inversion on axial data: only the inversion and what reads it
/// are computed (#355). Said before the run, which then fails here for want of weights.
#[test]
fn a_dry_run_of_an_axial_only_inversion_on_axial_data() {
    use ExpectedState::*;
    let (facts, mut store) = phantom();
    let mut config = config();
    run_once(&config, &facts, &mut store);
    config.inversion.algorithm = QsmAlgorithm::Xqsm;
    let plan = crate::plan::plan(&config, &facts);
    let e: Vec<_> = dry_run(&plan, &config, &facts, &store).unwrap().into_iter().map(|e| (e.id, e.state)).collect();
    assert_eq!(states(&e, &["load", "bgremove", "invert", "reference"]),
               [("load", Cached), ("bgremove", Cached), ("invert", Changed), ("reference", Stale)]);
}

/// Two-pass, motion correction, SWI with relaxometry, and a resampled run, each changed after
/// a first run: the dry run agrees with the run every time.
#[test]
fn a_dry_run_agrees_with_the_run_on_every_kind_of_change() {
    let (facts, mut store) = phantom();
    let mut config = two_pass_config();
    expect_then_run(&config, &facts, &mut store);
    config.masking.two_pass_sections.as_mut().unwrap()[0].refinements.push(MaskOp::Erode { iterations: 1 });
    let e = expect_then_run(&config, &facts, &mut store);
    assert!(e.iter().any(|(id, s)| id == "mask-reliable" && *s == ExpectedState::Changed), "{e:?}");
    assert!(e.iter().any(|(id, s)| id == "invert" && *s == ExpectedState::Cached), "{e:?}");

    let (facts, mut store) = phantom();
    let mut config = super::tests::config();
    config.pipeline.do_swi = true;
    config.pipeline.do_smwi = true;
    config.pipeline.do_t2starmap = true;
    config.pipeline.do_r2starmap = true;
    config.swi.mip_window = 4;
    config.smwi.mip_window = 4;
    expect_then_run(&config, &facts, &mut store);
    config.motion_correction.enabled = true;
    expect_then_run(&config, &facts, &mut store);
    config.motion_correction.enabled = false;
    expect_then_run(&config, &facts, &mut store);
    config.swi.mip_window = 3;
    expect_then_run(&config, &facts, &mut store);

    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (facts, mut store) = phantom_on(affine);
    let mut config = super::tests::config();
    config.pipeline.axial_resampling = true;
    config.pipeline.obliquity_threshold = 5.0;
    expect_then_run(&config, &facts, &mut store);
    expect_then_run(&config, &facts, &mut store);
    config.inversion.tkd.threshold *= 2.0;
    expect_then_run(&config, &facts, &mut store);
    config.pipeline.output_space = crate::OutputSpace::Working;
    expect_then_run(&config, &facts, &mut store);
}

/// R2 and R2′ from a MESE, and chi-separation, before and after a new mask. χ-sep iLSQR makes
/// its own QSM from the local field, so a new inversion leaves it as it was.
#[test]
fn a_dry_run_follows_relaxometry_and_chi_separation() {
    let (mut facts, mut store) = phantom();
    with_mese(&mut facts, &mut store);
    let mut config = config();
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_r2map = true;
    config.pipeline.do_r2primemap = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
    expect_then_run(&config, &facts, &mut store);
    expect_then_run(&config, &facts, &mut store);
    config.inversion.tkd.threshold *= 2.0;
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(states(&e, &["r2_r2prime", "reference", "chi_separation"]),
               [("r2_r2prime", ExpectedState::Cached), ("reference", ExpectedState::Stale),
                ("chi_separation", ExpectedState::Cached)]);
    // The iLSQR of the QSM χ-sep iLSQR makes for itself is in its key.
    config.separation.chi_sep_ilsqr.qsm_ilsqr.max_iter = 5;
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(states(&e, &["chi_separation"]), [("chi_separation", ExpectedState::Changed)]);
    erode_the_brain_mask(&mut config);
    expect_then_run(&config, &facts, &mut store);
}

/// Segmentation and the per-structure statistics, which keys itself outside `begin`.
#[test]
fn a_dry_run_follows_the_statistics() {
    let (mut facts, mut store) = phantom();
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
    let dseg: Vec<f64> = m.iter().enumerate()
        .map(|(i, &v)| if v > 50.0 { if i % N < N / 2 { 10.0 } else { 49.0 } } else { 0.0 }).collect();
    store.insert_on_grid(Item::SuppliedSegmentation, dseg,
                         crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None });
    facts.custom_dseg = true;
    let mut config = config();
    config.pipeline.do_segmentation = true;
    config.pipeline.do_analysis = true;
    config.pipeline.do_r2starmap = true;
    config.segmentation.custom_dseg_tool = Some("mine".into());
    expect_then_run(&config, &facts, &mut store);
    expect_then_run(&config, &facts, &mut store);
    config.inversion.tkd.threshold *= 2.0;
    let e = expect_then_run(&config, &facts, &mut store);
    assert_eq!(states(&e, &["segmentation", "analysis"]),
               [("segmentation", ExpectedState::Cached), ("analysis", ExpectedState::Stale)]);
}

/// A store restored from an earlier one's record over the same volumes ([`MemoryStore::restore`])
/// answers the same.
#[test]
fn a_dry_run_on_a_restored_store() {
    let (facts, mut store) = phantom();
    let mut config = config();
    run_once(&config, &facts, &mut store);
    let header = store.header().unwrap();
    let recorded = store.recorded();
    let mut restored = MemoryStore::with_volumes(header, json!("phantom-1"), std::mem::take(store.volumes()));
    restored.restore(recorded);
    assert!(expect_then_run(&config, &facts, &mut restored).iter().all(|(_, s)| matches!(s, ExpectedState::Cached | ExpectedState::Skipped)));
    config.inversion.tkd.threshold *= 2.0;
    let e = expect_then_run(&config, &facts, &mut restored);
    assert_eq!(states(&e, &["invert", "reference"]), [("invert", ExpectedState::Changed), ("reference", ExpectedState::Stale)]);
}
