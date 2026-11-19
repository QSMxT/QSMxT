//! Each kind of step run in memory on the phantom: what it computes and keeps.

use super::tests::*;
use super::*;
use crate::enums::{QsmAlgorithm, SeparationAlgorithm};
use crate::masking::{MaskOp, MaskSection, MaskThresholdMethod, MaskingInput};
use crate::store::{Header, MemoryStore};

/// `--tukey-window` windows every echo's magnitude and phase before any other step reads
/// them, as qsm-core's `tukey_window_mag_phase` does, and keys `scale_phase` (only when on).
#[test]
fn the_tukey_window_reaches_every_echo_before_the_other_steps() {
    let (facts, mut store) = phantom();
    let acquired: Vec<(Vec<f64>, Vec<f64>)> = (1..=TES.len()).map(|e| {
        let get = |i: Item| match store.get(&i) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!() };
        (get(Item::AcquiredMagnitude(e)), get(Item::AcquiredPhase(e)))
    }).collect();
    let mut c = config();
    c.pipeline.do_qsm = false;
    c.pipeline.do_r2starmap = true;
    assert!(run_once(&c, &facts, &mut store).computed.iter().any(|s| s == "scale_phase"));
    assert_eq!(store.get(&Item::Magnitude(2)), store.get(&Item::AcquiredMagnitude(2)), "off: as acquired");

    c.pipeline.tukey_window = 0.4;
    let on = run_once(&c, &facts, &mut store).computed;
    assert!(on.iter().any(|s| s == "tukey_window"), "{on:?}");
    assert!(on.iter().any(|s| s == "scale_phase"), "phase scaling now leaves the echoes for the window: {on:?}");
    let mut acquired = acquired;
    {
        let mut ps: Vec<&mut Vec<f64>> = acquired.iter_mut().map(|(_, p)| p).collect();
        qsm_core::pipeline::scale_phases_to_pi(&mut ps);
    }
    for (e, (mag, phase)) in acquired.into_iter().enumerate() {
        let (mut m, mut p) = (mag, phase);
        qsm_core::utils::tukey_window::tukey_window_mag_phase(&mut m, &mut p, (N, N, N), 0.4);
        assert_eq!(store.get(&Item::Magnitude(e + 1)), Some(&VolumeData::Real(m)), "echo {}", e + 1);
        assert_eq!(store.get(&Item::Phase(e + 1)), Some(&VolumeData::Real(p)), "echo {}", e + 1);
    }

    // Without magnitude it is refused at plan time.
    let no_mag = ScanFacts { has_magnitude: false, ..facts };
    let err = crate::requirements::scan_check(&no_mag, &c).errors.join("\n");
    assert!(err.contains("--tukey-window needs magnitude"), "{err}");
}

/// The phase steps of a windowed, resampled run do what the single step that did all three
/// did, in the same order: every echo scaled to radians, windowed on the acquired grid, then
/// resampled with its magnitude through the complex domain. Each leaves the echoes on the
/// acquired grid for the next; the working echoes are the resampling's.
#[test]
fn a_windowed_resampled_run_gives_the_echoes_the_single_step_gave() {
    let (cos, sin) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, cos, -sin, 0.0, 0.0, sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0];
    for tukey in [0.0, 0.3] {
        let (facts, mut store) = phantom_on(affine);
        let acquired: Vec<(Vec<f64>, Vec<f64>)> = (1..=TES.len()).map(|e| {
            let get = |i: Item| match store.get(&i) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!() };
            (get(Item::AcquiredMagnitude(e)), get(Item::AcquiredPhase(e)))
        }).collect();
        let mut c = config();
        c.pipeline.do_qsm = false;
        c.pipeline.do_r2starmap = true;
        c.pipeline.axial_resampling = true;
        c.pipeline.obliquity_threshold = 5.0;
        c.pipeline.tukey_window = tukey;
        let events = run_once(&c, &facts, &mut store);
        let phase_steps: Vec<&str> = events.computed.iter().map(String::as_str)
            .filter(|s| ["scale_phase", "tukey_window", "resample_axial"].contains(s)).collect();
        let want: &[&str] = if tukey > 0.0 { &["scale_phase", "tukey_window", "resample_axial"] } else { &["scale_phase", "resample_axial"] };
        assert_eq!(phase_steps, want);
        let meta = store.grid().unwrap().clone();
        assert!(meta.source_geometry.is_some());
        let mut acquired = acquired;
        {
            let mut ps: Vec<&mut Vec<f64>> = acquired.iter_mut().map(|(_, p)| p).collect();
            qsm_core::pipeline::scale_phases_to_pi(&mut ps);
        }
        for (e, (mag, phase)) in acquired.into_iter().enumerate() {
            let e = e + 1;
            // The single step's operations, called directly.
            let (mut m, mut p) = (mag, phase);
            c.pipeline.tukey_window = tukey;
            crate::stages::phase::tukey_window(&mut m, &mut p, &meta, &c);
            let want = crate::stages::phase::working_echo(p, Some(m), true, &meta);
            assert_eq!(store.get(&Item::Phase(e)), Some(&VolumeData::Real(want.phase)), "echo {e}, Tukey {tukey}");
            assert_eq!(store.get(&Item::Magnitude(e)), Some(&VolumeData::Real(want.magnitude.unwrap())), "echo {e}");
            // What the steps before it left, on the acquired grid.
            let acquired_grid = Some(crate::stages::output_space::acquired((N, N, N), &affine));
            let between = if tukey > 0.0 { Item::WindowedPhase(e) } else { Item::ScaledPhase(e) };
            assert_eq!(store.geometry(&between).cloned(), acquired_grid, "{between:?}");
        }
        assert!(!store.has(&Item::ScaledMagnitude(1)), "the magnitude given is read where it is");
    }
}

/// A run that is not resampled skips the resampling step: the step before it writes the
/// working echoes, exactly as when the configuration cannot resample, under the same key.
#[test]
fn a_run_that_is_not_resampled_skips_the_resampling() {
    let held = |store: &MemoryStore| (1..=TES.len()).map(|e| (store.get(&Item::Phase(e)).cloned(), store.get(&Item::Magnitude(e)).cloned()))
        .collect::<Vec<_>>();
    let mut c = config();
    c.pipeline.do_qsm = false;
    c.pipeline.do_r2starmap = true;
    let (facts, mut cannot) = phantom();
    run_once(&c, &facts, &mut cannot);
    c.pipeline.axial_resampling = true;
    let (facts, mut can) = phantom();
    let plan = crate::plan::plan(&c, &facts);
    assert!(plan.has(Stage::ResampleAxial));
    let record = run(&plan, &c, &facts, &mut can, &mut ()).unwrap();
    assert_eq!(record.step("resample_axial").map(|s| s.state), Some(StepState::Skipped));
    assert_eq!(record.resampled, Some(false));
    assert_eq!(held(&can), held(&cannot));
    assert_eq!(can.get(&Item::Magnitude(2)), can.get(&Item::AcquiredMagnitude(2)), "kept as given");
    assert!(!can.has(&Item::ScaledPhase(1)));
    assert_eq!(can.recorded_keys().get("scale_phase"), cannot.recorded_keys().get("scale_phase"));
}

/// An empty brain mask stops the run at the mask step, computed or supplied (#251); a section
/// that adds nothing to a union does not.
#[test]
fn an_empty_brain_mask_is_refused_at_the_mask_step() {
    let failed = |c: &PipelineConfig, facts: &ScanFacts, store: &mut MemoryStore| {
        let mut events = Events::default();
        let err = run(&crate::plan::plan(c, facts), c, facts, store, &mut events).unwrap_err();
        assert_eq!(events.computed.last().map(String::as_str), Some("mask"), "{err}");
        assert!(store.get(&Item::BrainMask).is_none());
        err.to_string()
    };
    let (facts, mut store) = phantom();
    let mut c = config();
    // Erosion after combining removes the whole sphere.
    c.masking.refinements = vec![MaskOp::Erode { iterations: N }];
    let err = failed(&c, &facts, &mut store);
    assert!(err.contains("brain mask is empty") && err.contains("--mask-refine"), "{err}");

    // A section that finds nothing, in a union with one that finds the sphere.
    c.masking.refinements.clear();
    let mut nothing = c.masking.sections[0].clone();
    nothing.refinements = vec![MaskOp::Erode { iterations: N }];
    c.masking.sections.push(nothing);
    assert!(run_once(&c, &facts, &mut store).computed.iter().any(|s| s == "mask"));

    let (mut facts, mut store) = phantom();
    facts.custom_mask = true;
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    store.supply_mask(Supplied::Found {
        origin: "sub-1_mask.nii".into(), data: vec![0.0; N * N * N], dims: (N, N, N), affine }, "sub-1_mask.nii");
    let mut c = config();
    c.masking.custom_mask_tool = Some("bet".into());
    let err = failed(&c, &facts, &mut store);
    assert!(err.contains("sub-1_mask.nii is empty"), "{err}");
}

/// Two-pass reconstructs twice and references both maps; each pass's map is defined where its
/// background removal left a field.
#[test]
fn two_pass_runs_both_passes_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.masking.two_pass = true;
    config.masking.two_pass_sections = Some(vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }]);
    let events = run_once(&config, &facts, &mut store);
    assert_eq!(events.computed[4..], ["unwrap", "bgremove", "invert", "mask-reliable", "bgremove-reliable",
                                      "invert-reliable", "twopass", "reference-singlepass", "reference"]);
    for item in [Item::CombinedRawChi, Item::SinglePassChi, Item::Chi, Item::RawChi(Pass::Reliable)] {
        assert!(store.get(&item).is_some(), "{item:?}");
    }
    // The combination is keyed by which mask its reliable pass is defined on.
    let plan = crate::plan::plan(&config, &facts);
    let mut quiet = ();
    let r = Run::new(&plan, &config, &facts, crate::stages::magnitude::tests::meta_4x4x4(), &mut store, &mut quiet);
    assert_eq!(r.support(Pass::Reliable), Item::LocalFieldMask(Pass::Reliable));
    assert_eq!(r.store.identity(&r.support(Pass::Reliable)), json!("LocalFieldMask(Reliable)"));
}

/// `local` (the default) keeps only holes with a deep phase-quality core, which this tiny
/// phantom has none of: the merge then returns the single-pass map. Switching to `original`
/// recomputes only the reliable side and the merge.
#[test]
fn a_local_two_pass_without_source_holes_is_the_single_pass_map_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.masking.two_pass = true;
    config.masking.two_pass_sections = Some(vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }]);
    assert_eq!(config.masking.two_pass_mode, crate::enums::TwoPassMode::Local);
    let events = run_once(&config, &facts, &mut store);
    assert!(events.computed.iter().any(|s| s == "bgremove-reliable"), "{:?}", events.computed);
    let main = store.real(&Item::RawChi(Pass::Main)).unwrap();
    assert_eq!(store.real(&Item::CombinedRawChi).unwrap(), main);
    let brain = store.mask(&Item::BrainMask).unwrap();
    let reliable = store.mask(&Item::ReliableMask).unwrap();
    assert!(reliable.iter().zip(&brain).all(|(&r, &b)| r == 0 || b != 0), "the reliable mask is in the brain mask");

    config.masking.two_pass_mode = crate::enums::TwoPassMode::Original;
    let events = run_once(&config, &facts, &mut store);
    assert!(!events.computed.iter().any(|s| s == "bgremove" || s == "invert" || s == "mask"), "{:?}", events.computed);
    for step in ["mask-reliable", "bgremove-reliable", "twopass"] {
        assert!(events.computed.iter().any(|s| s == step), "{step}: {:?}", events.computed);
    }
}

/// With HEIDI on, a two-pass run inverts each pass without it (their keys are unchanged) and
/// fills the combined map once: only the combination and the final referencing are recomputed.
#[test]
fn two_pass_heidi_fills_the_combined_map_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.inversion.tkd.threshold = 0.1;
    config.masking.two_pass = true;
    config.masking.two_pass_sections = Some(vec![MaskSection {
        input: MaskingInput::Magnitude,
        generator: MaskOp::Threshold { method: MaskThresholdMethod::Fixed, value: Some(40.0) },
        refinements: vec![],
    }]);
    run_once(&config, &facts, &mut store);
    let plain = store.get(&Item::CombinedRawChi).unwrap().clone();
    config.inversion.heidi.enabled = true;
    (config.inversion.heidi.continuation_steps, config.inversion.heidi.inner_iterations) = (1, 2);
    let events = run_once(&config, &facts, &mut store);
    assert_eq!(events.computed, ["twopass", "reference"], "{:?}", events.computed);
    assert_ne!(store.get(&Item::CombinedRawChi).unwrap(), &plain, "the combined map was not filled");
}

/// SWI's filtered phase is kept from the SWI step only when asked for, with its units, and
/// asking for it (or no longer) computes the step again; off, SWI stays cached as it was.
#[test]
fn the_swi_filtered_phase_is_kept_when_asked_for() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_qsm = false;
    config.pipeline.do_swi = true;
    config.swi.mip_window = 4;
    let events = run_once(&config, &facts, &mut store);
    assert!(events.computed.iter().any(|c| c == "swi"), "{:?}", events.computed);
    assert!(!store.has(&Item::SwiPhase) && !events.outputs.contains(&Item::SwiPhase));

    config.swi.filtered_phase = true;
    let events = run_once(&config, &facts, &mut store);
    assert!(events.computed.iter().any(|c| c == "swi"), "{:?}", events.computed);
    assert!(events.outputs.contains(&Item::SwiPhase));
    let Some(VolumeData::Real(phase)) = store.get(&Item::SwiPhase) else { panic!("no filtered phase") };
    assert!(phase.iter().any(|&v| v != 0.0) && phase.contains(&0.0), "0 outside CLEAR-SWI's mask only");
    let units = if facts.n_echoes > 1 { "Hz" } else { "rad" };
    assert_eq!(store.annotations(&Item::SwiPhase).unwrap()["Units"], units);
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());

    config.swi.filtered_phase = false;
    assert!(run_once(&config, &facts, &mut store).computed.iter().any(|c| c == "swi"));
}

/// SWI and its minIP, T2*/R2*, and SMWI from the final map, all in memory: each projection on
/// its own shorter grid, T2* the inverse of R2* inside the mask.
#[test]
fn swi_smwi_and_relaxometry_run_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_swi = true;
    config.pipeline.do_smwi = true;
    config.pipeline.do_t2starmap = true;
    config.pipeline.do_r2starmap = true;
    config.swi.mip_window = 4;
    config.smwi.mip_window = 4;
    let events = run_once(&config, &facts, &mut store);
    for step in ["swi", "t2star_r2star", "smwi"] {
        assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
    }
    for item in [Item::SwiMip, Item::SmwiMip(Contrast::Paramagnetic), Item::SmwiMip(Contrast::Diamagnetic)] {
        assert_eq!(store.geometry(&item).map(|g| g.dims), Some((N, N, N - 3)), "{item:?}");
        assert!(!events.outputs.contains(&item), "a projection is not shown on the run's grid");
    }
    for item in [Item::Swi, Item::Smwi(Contrast::Paramagnetic), Item::Smwi(Contrast::Diamagnetic)] {
        assert!(events.outputs.contains(&item) && store.geometry(&item).is_none(), "{item:?}");
    }
    let (Some(VolumeData::Real(r2star)), Some(VolumeData::Real(t2star)), Some(VolumeData::Mask(mask))) =
        (store.get(&Item::R2star), store.get(&Item::T2star), store.get(&Item::BrainMask)) else { panic!("no maps") };
    // The phantom's magnitude decays with T2* = 40 ms.
    let inside: Vec<f64> = r2star.iter().zip(mask).filter(|(_, &m)| m != 0).map(|(&r, _)| r).collect();
    assert!(inside.iter().all(|r| (r - 25.0).abs() < 1.0), "R2* {:?}", &inside[..4]);
    assert!(r2star.iter().zip(t2star).all(|(&r, &t)| if r > 0.0 { (t * r - 1.0).abs() < 1e-12 } else { t == 0.0 }));
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());

    // SMWI reads the referenced map, so a new inversion redoes it; SWI and R2* read the echoes.
    config.inversion.tkd.threshold *= 2.0;
    assert_eq!(run_once(&config, &facts, &mut store).computed, ["invert", "reference", "smwi"]);
}

/// R2 from the MESE, R2′ measured as R2* − R2 (and recorded as measured), and χ-separation
/// over the final map's support, in memory.
#[test]
fn r2_r2prime_and_chi_separation_run_in_memory() {
    let (mut facts, mut store) = phantom();
    with_mese(&mut facts, &mut store);
    let mut config = config();
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_r2map = true;
    config.pipeline.do_r2primemap = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
    let events = run_once(&config, &facts, &mut store);
    for step in ["t2star_r2star", "r2_r2prime", "chi_separation"] {
        assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
    }
    let real = |item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!("no {item:?}") };
    let (r2, mask) = (real(Item::R2), store.get(&Item::BrainMask).cloned().unwrap());
    let VolumeData::Mask(mask) = mask else { panic!() };
    let inside: Vec<f64> = r2.iter().zip(&mask).filter(|(_, &m)| m != 0).map(|(&r, _)| r).collect();
    assert!(inside.iter().all(|r| (r - 10.0).abs() < 1.0), "R2 {:?}", &inside[..4]);
    assert_eq!(store.annotations(&Item::R2prime).and_then(|a| a.get("R2primeSource")), Some(&json!("measured")));
    assert!(store.get(&Item::R2primeCoverage).is_some());
    let support = match store.get(&Item::ChiSupport) { Some(VolumeData::Mask(m)) => m.clone(), _ => panic!() };
    for item in [Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic), Item::SeparatedTotalChi] {
        let map = real(item);
        assert!(map.iter().zip(&support).all(|(&v, &s)| s != 0 || v == 0.0), "{item:?} outside the support");
        assert!(map.iter().all(|v| v.is_finite()));
    }
    assert!(real(Item::SeparatedChi(Contrast::Diamagnetic)).iter().all(|&v| v >= 0.0), "|χ−|");
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());
}

/// χ-sep iLSQR takes a supplied R2′ as it is outside the χ support, as the toolbox's script
/// passes it: its values there reach the fit (QSM.rs's `chi_sep_ilsqr` reads them just past
/// the mask), and the maps it writes are still 0 outside the support.
#[test]
fn r2prime_reaches_chi_sep_ilsqr_unmasked() {
    // R2′ of 5 Hz on `inside` (none: where the magnitude is bright), `outside` elsewhere; the
    // maps written and the support.
    let separated = |inside: Option<&[u8]>, outside: f64| {
        let (mut facts, mut store) = phantom();
        let mut affine = [0.0; 16];
        for i in 0..4 { affine[5 * i] = 1.0; }
        let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
        let r2p: Vec<f64> = m.iter().enumerate()
            .map(|(i, &v)| if inside.map_or(v > 50.0, |s| s[i] != 0) { 5.0 } else { outside }).collect();
        store.insert(Item::SuppliedR2prime, VolumeData::Real(r2p));
        let grid = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
        store.supply(Item::SuppliedR2prime, grid, "r2prime");
        facts.custom_r2prime = true;
        let mut c = config();
        c.separation.custom_r2prime_tool = Some("mine".into());
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = SeparationAlgorithm::ChiSepIlsqr;
        run_once(&c, &facts, &mut store);
        let support = match store.get(&Item::ChiSupport) { Some(VolumeData::Mask(m)) => m.clone(), _ => panic!() };
        assert!(support.contains(&0) && support.iter().any(|&s| s != 0));
        let maps: Vec<Vec<f64>> = [Item::SeparatedChi(Contrast::Paramagnetic), Item::SeparatedChi(Contrast::Diamagnetic),
                                   Item::SeparatedTotalChi].into_iter()
            .map(|item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!("no {item:?}") })
            .collect();
        for map in &maps {
            assert!(map.iter().zip(&support).all(|(&v, &s)| s != 0 || v == 0.0), "written outside the support");
        }
        (maps, support)
    };
    let (_, support) = separated(None, 0.0);
    let (zero, _) = separated(Some(&support), 0.0);
    let (nonzero, _) = separated(Some(&support), 30.0);
    assert_ne!(zero[0], nonzero[0], "R2′ outside the support did not reach χ-sep iLSQR");
    // NaN outside is no measurement: as 0.
    assert_eq!(separated(Some(&support), f64::NAN).0, zero);
}

/// A per-echo QSM runs the same steps on each echo's field, publishes each echo's referenced
/// map with its echo time, and the QSM is their mean where every echo's is defined; DECOMPOSE
/// takes the echoes' maps. Turning it on or off recomputes the reconstruction, not the mask.
#[test]
fn a_per_echo_qsm_runs_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    run_once(&config, &facts, &mut store);
    config.qsm.per_echo = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = crate::enums::SeparationAlgorithm::Decompose;
    config.separation.decompose.n_inner = 1;
    config.separation.decompose.lsqcurvefit.max_iterations = 2;
    let events = run_once(&config, &facts, &mut store);
    assert_eq!(events.computed, ["unwrap", "bgremove", "invert", "reference", "chi_separation"]);
    let real = |store: &MemoryStore, item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!("no {item:?}") };
    let support = match store.get(&Item::ChiSupport) { Some(VolumeData::Mask(m)) => m.clone(), _ => panic!() };
    let echoes: Vec<Vec<f64>> = (1..=TES.len()).map(|e| real(&store, Item::EchoChi(e))).collect();
    for (e, te) in TES.iter().enumerate() {
        assert_eq!(store.annotations(&Item::EchoChi(e + 1)).and_then(|a| a.get("EchoTime")), Some(&json!(te)));
        assert!(store.get(&Item::EchoField(e + 1)).is_some() && store.get(&Item::EchoRawChi(e + 1)).is_some());
    }
    assert!(!events.outputs.contains(&Item::FieldPpm), "a per-echo QSM makes no echo-combined field");
    // Referenced over one support, the mean of the echoes' maps is the QSM.
    let chi = real(&store, Item::Chi);
    let n_support = support.iter().filter(|&&s| s != 0).count();
    assert!(n_support > 100, "{n_support}");
    for v in 0..chi.len() {
        if support[v] == 0 {
            assert_eq!(chi[v], 0.0);
            continue;
        }
        let mean = echoes.iter().map(|m| m[v]).sum::<f64>() / echoes.len() as f64;
        assert!((chi[v] - mean).abs() < 1e-9, "voxel {v}: {} vs {mean}", chi[v]);
    }
    assert!(real(&store, Item::SeparatedTotalChi).iter().all(|v| v.is_finite()));
    // Nothing changed: nothing to compute. Off again: the reconstruction is redone.
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());
    config.qsm.per_echo = false;
    let off = run_once(&config, &facts, &mut store);
    assert_eq!(off.computed, ["unwrap", "bgremove", "invert", "reference", "chi_separation"]);
}

/// Without a MESE, R2′ is predicted by R2PRIMEnet (whose weights the plan lists), and a
/// network separates the sources. Minutes in a debug build, so run with `--ignored`; skipped
/// where the weights have not been downloaded.
#[test]
#[ignore]
fn r2primenet_and_a_separation_network_run_in_memory() {
    let present = |id: &str| qsm_core::models::find_model(id)
        .is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
    if !cfg!(feature = "onnx") || !present("r2primenet") || !present("chi-sepnet") {
        eprintln!("skipped: R2PRIMEnet or χ-sepnet weights are not available here");
        return;
    }
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_r2starmap = true;
    config.pipeline.do_r2primemap = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepNet;
    // The network alone, given the R2' the R2 step predicts (the toolbox route predicts its own).
    config.separation.chi_sepnet.route = crate::ChiSepNetRoute::Native;
    config.separation.r2primenet.patch = [16, 16, 16];
    let plan = crate::plan::plan(&config, &facts);
    assert_eq!(plan.models, ["r2primenet", "chi-sepnet"]);
    run(&plan, &config, &facts, &mut store, &mut ()).unwrap();
    assert_eq!(store.annotations(&Item::R2prime).and_then(|a| a.get("R2primeSource")), Some(&json!("predicted")));
    assert!(store.get(&Item::SeparatedTotalChi).is_some());
}

/// χ-sepnet's toolbox route, with an R2′ map asked for: the map's sidecar says the route did
/// not use it and the χ maps' say what it did. Run with `--ignored`; skipped where the weights
/// have not been downloaded.
#[test]
#[ignore]
fn the_toolbox_route_records_the_r2prime_it_used() {
    let present = |id: &str| qsm_core::models::find_model(id)
        .is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
    if !cfg!(feature = "onnx") || !["r2primenet", "chi-sepnet", "qsmnetme"].iter().all(|m| present(m)) {
        eprintln!("skipped: χ-sepnet's route's weights are not available here");
        return;
    }
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_r2primemap = true;
    config.pipeline.do_chi_separation = true;
    config.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepNet;
    config.separation.r2primenet.patch = [16, 16, 16];
    let plan = crate::plan::plan(&config, &facts);
    run(&plan, &config, &facts, &mut store, &mut ()).unwrap();
    let of = |store: &MemoryStore, item: &Item, key: &str| store.annotations(item).and_then(|a| a.get(key)).cloned();
    assert_eq!(of(&store, &Item::R2prime, "R2primeSource"), Some(json!("predicted")));
    assert_eq!(of(&store, &Item::R2prime, "R2primeUsedByChiSeparation"), Some(json!(false)));
    assert_eq!(of(&store, &Item::SeparatedTotalChi, "R2primeSource"), Some(json!("predicted")));
    assert!(store.get(&Item::SeparatedTotalChi).is_some());
}

/// A supplied segmentation, a region reference measured on it, and the statistics of every map
/// over it, all in memory.
#[test]
fn segmentation_and_analysis_run_in_memory() {
    let (mut facts, mut store) = phantom();
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    // Two structures: the sphere's left and right halves.
    let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
    let dseg: Vec<f64> = m.iter().enumerate()
        .map(|(i, &v)| if v > 50.0 { if i % N < N / 2 { 10.0 } else { 49.0 } } else { 0.0 }).collect();
    store.insert_on_grid(Item::SuppliedSegmentation, dseg.clone(),
                         crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None });
    facts.custom_dseg = true;
    let mut config = config();
    config.pipeline.do_segmentation = true;
    config.pipeline.do_analysis = true;
    config.pipeline.do_r2starmap = true;
    config.segmentation.custom_dseg_tool = Some("mine".into());
    config.qsm.reference = crate::enums::QsmReference::Region;
    config.qsm.reference_region = Some("thalamus".into());
    let events = run_once(&config, &facts, &mut store);
    for step in ["segmentation", "analysis"] {
        assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
    }
    assert_eq!(store.get(&Item::Segmentation), Some(&VolumeData::Real(dseg)));
    let lookup = store.report(&Item::SegmentationLabels).unwrap();
    assert!(lookup.as_str().unwrap().starts_with("index\tname\n"));
    assert!(store.report(&Item::SegmentationVolumes).is_none(), "a supplied segmentation has no posteriors");
    let table = store.report(&Item::RegionStats).unwrap();
    let rows: Vec<&str> = table.as_str().unwrap().lines().skip(1).collect();
    for map in ["Chimap", "R2starmap"] {
        assert_eq!(rows.iter().filter(|r| r.starts_with(&format!("{map}\t"))).count(), 2, "{map}: {rows:?}");
    }
    assert!(rows.iter().filter(|r| r.starts_with("Chimap\t")).all(|r| r.contains("\tthalamus\t")), "{rows:?}");
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());
}

/// Motion correction in memory: the corrected echoes and the mask every echo covers, which
/// the steps after it read. Turned off again, what it left goes, and the run is redone (the
/// load step's key carries the setting, as for a change of grid).
#[test]
fn motion_correction_runs_in_memory() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.motion_correction.enabled = true;
    let events = run_once(&config, &facts, &mut store);
    assert_eq!(events.computed[..5], ["load", "scale_phase", "magnitude", "mask", "motion"]);
    assert!(store.get(&Item::CoverageMask).is_some() && store.get(&Item::CorrectedPhase(3)).is_some());
    let report = store.report(&Item::MotionReport).unwrap();
    assert_eq!(report["echoes"].as_array().map(Vec::len), Some(3));
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());

    config.motion_correction.enabled = false;
    let off = run_once(&config, &facts, &mut store);
    assert!(store.get(&Item::CoverageMask).is_none(), "the coverage mask is not this run's");
    assert_eq!(off.computed, ["load", "scale_phase", "magnitude", "mask", "unwrap", "bgremove", "invert", "reference"]);
}

/// QSMART's vessel magnitude is the mean over every echo: a run missing one echo's magnitude
/// is refused, naming it, rather than averaging the others.
#[test]
fn qsmart_refuses_a_missing_echo_magnitude() {
    let (facts, mut store) = phantom();
    let mut config = config();
    config.inversion.algorithm = QsmAlgorithm::Qsmart;
    let plan = crate::plan::plan(&config, &facts);
    run(&plan, &config, &facts, &mut store, &mut ()).unwrap();
    // Echo 2's magnitude gone after the field map was made, and the vessel magnitude to be
    // made again.
    store.take(&Item::AcquiredMagnitude(2));
    store.take(&Item::Magnitude(2));
    config.masking.inhomogeneity_correction = !config.masking.inhomogeneity_correction;
    let plan = crate::plan::plan(&config, &facts);
    let e = run(&plan, &config, &facts, &mut store, &mut ()).unwrap_err().to_string();
    assert!(e.contains("no magnitude for echo 2 of") && e.contains("--qsmart-vessel-magnitude combined"), "{e}");
}

/// SynthSeg in memory. Minutes in a debug build, so run with `--ignored`; skipped where the
/// weights have not been downloaded.
#[test]
#[ignore]
fn synthseg_runs_in_memory() {
    let present = qsm_core::models::find_model("synthseg")
        .is_some_and(|m| m.files.iter().all(qsm_core::models::weight_file_present));
    if !cfg!(feature = "onnx") || !present {
        eprintln!("skipped: SynthSeg's weights are not available here");
        return;
    }
    let (facts, mut store) = phantom();
    let mut config = config();
    config.pipeline.do_qsm = false;
    config.pipeline.do_segmentation = true;
    let events = run_once(&config, &facts, &mut store);
    assert!(events.computed.iter().any(|c| c == "segmentation"), "{:?}", events.computed);
    let Some(VolumeData::Real(labels)) = store.get(&Item::Segmentation) else { panic!("no segmentation") };
    assert_eq!(labels.len(), N * N * N);
    let volumes = store.report(&Item::SegmentationVolumes).expect("SynthSeg's volumes");
    assert_eq!(volumes.as_array().unwrap().len(), config.segmentation.version.to_core().labels().ids.len());
}

/// An oblique run resampled to axial, reconstructed there and returned to the grid it was
/// acquired on: the masks still binary, the minIP rebuilt on that grid, and a re-run that
/// recomputes a step reads the working-grid versions again.
#[test]
fn a_resampled_run_returns_to_the_acquired_grid_in_memory() {
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (facts, mut store) = phantom_on(affine);
    let mut config = config();
    config.pipeline.axial_resampling = true;
    config.pipeline.obliquity_threshold = 5.0;
    config.pipeline.do_swi = true;
    config.swi.mip_window = 4;
    let events = run_once(&config, &facts, &mut store);
    assert_eq!(events.computed.last().map(String::as_str), Some("output_space"), "{:?}", events.computed);
    for item in [Item::Chi, Item::ChiSupport, Item::BrainMask, Item::CombinedMagnitude, Item::Swi] {
        let g = store.geometry(&item).unwrap_or_else(|| panic!("{item:?} left on the working grid"));
        assert_eq!((g.dims, g.affine), ((N, N, N), affine), "{item:?}");
    }
    let Some(VolumeData::Real(mask)) = store.get(&Item::BrainMask) else { panic!() };
    assert!(mask.iter().all(|&v| v == 0.0 || v == 1.0), "a mask travels by nearest neighbour");
    let mip = store.geometry(&Item::SwiMip).unwrap();
    assert_eq!(mip.dims, (N, N, N - 3), "rebuilt from the returned SWI");
    assert!(run_once(&config, &facts, &mut store).computed.is_empty());

    // A new inversion reads the working-grid field again, and returns its map once more.
    config.inversion.tkd.threshold *= 2.0;
    let again = run_once(&config, &facts, &mut store);
    assert_eq!(again.computed, ["invert", "reference", "output_space"]);
    assert!(store.geometry(&Item::Chi).is_some_and(|g| g.affine == affine));
    assert!(store.geometry(&Item::LocalField(Pass::Main)).is_none(), "intermediates stay on the working grid");
}

/// A map supplied on a grid of its own is refused once the load step has decided the run's
/// grid, before anything is computed from the scan, naming the file and both grids (#384). A
/// dry run refuses it the same way.
#[test]
fn a_supplied_map_on_another_grid_is_refused_before_the_first_step() {
    let mut identity = [0.0; 16];
    for i in 0..4 { identity[5 * i] = 1.0; }
    let small = crate::store::Geometry { dims: (4, 4, 4), voxel_size: (1.0, 1.0, 1.0), affine: identity, orientation: None };
    type Setup = fn(&mut PipelineConfig, &mut ScanFacts);
    let cases: [(Item, Setup); 4] = [
        (Item::SuppliedChi, |c, f| {
            c.pipeline.do_chi_separation = true;
            c.separation.custom_qsm_tool = Some("mine".into());
            f.custom_qsm = true;
        }),
        (Item::SuppliedR2, |c, f| {
            c.pipeline.do_r2map = true;
            c.separation.custom_r2_tool = Some("mine".into());
            f.custom_r2 = true;
        }),
        (Item::SuppliedR2prime, |c, f| {
            c.pipeline.do_r2primemap = true;
            c.separation.custom_r2prime_tool = Some("mine".into());
            f.custom_r2prime = true;
        }),
        // Read by the statistics directly, with no segmentation step to fall back to.
        (Item::SuppliedSegmentation, |c, f| {
            c.pipeline.do_analysis = true;
            c.segmentation.custom_dseg_tool = Some("mine".into());
            f.custom_dseg = true;
        }),
    ];
    for (item, setup) in cases {
        let (mut facts, mut store) = phantom();
        let mut c = config();
        setup(&mut c, &mut facts);
        store.insert_on_grid(item, vec![1.0; 64], small.clone());
        let plan = crate::plan::plan(&c, &facts);
        assert!(plan.steps.iter().any(|s| s.reads.contains(&item)), "{item:?}: no step reads it");
        let dry = dry_run(&plan, &c, &facts, &store).unwrap_err().to_string();
        let mut events = Events::default();
        let err = run(&plan, &c, &facts, &mut store, &mut events).unwrap_err().to_string();
        assert_eq!(events.computed, ["load"], "{item:?}");
        assert!(err.contains(&format!("{item:?}")) && err.contains("4x4x4") && err.contains("16x16x16"), "{err}");
        assert_eq!(dry, err);
    }

    // A segmentation the segmentation step reads is left to it, which segments the scan instead.
    let (mut facts, mut store) = phantom();
    let mut c = config();
    c.pipeline.do_segmentation = true;
    c.segmentation.custom_dseg_tool = Some("mine".into());
    facts.custom_dseg = true;
    store.insert_on_grid(Item::SuppliedSegmentation, vec![1.0; 64], small);
    assert!(dry_run(&crate::plan::plan(&c, &facts), &c, &facts, &store).is_ok());
}

/// In a run resampled to a cardinal-aligned grid, maps supplied on the scan's acquired grid
/// are brought onto the working grid, as the echoes are, and used (#384).
#[test]
fn supplied_maps_on_the_acquired_grid_of_a_resampled_run_are_resampled_and_used() {
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (mut facts, mut store) = phantom_on(affine);
    let acquired = crate::store::Geometry { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None };
    let Some(VolumeData::Real(m)) = store.get(&Item::AcquiredMagnitude(1)).cloned() else { panic!() };
    let inside = |v: f64| m.iter().map(|&x| if x > 50.0 { v } else { 0.0 }).collect::<Vec<_>>();
    store.insert_on_grid(Item::SuppliedR2, inside(12.0), acquired.clone());
    store.insert_on_grid(Item::SuppliedSegmentation, inside(10.0), acquired);
    (facts.custom_r2, facts.custom_dseg) = (true, true);
    let mut config = config();
    config.pipeline.axial_resampling = true;
    config.pipeline.obliquity_threshold = 5.0;
    config.pipeline.do_r2map = true;
    config.pipeline.do_analysis = true;
    config.separation.custom_r2_tool = Some("mine".into());
    config.segmentation.custom_dseg_tool = Some("mine".into());
    let events = run_once(&config, &facts, &mut store);
    for step in ["r2_r2prime", "analysis", "output_space"] {
        assert!(events.computed.iter().any(|c| c == step), "{step}: {:?}", events.computed);
    }
    assert!(store.get(&Item::R2).is_some());
    let table = store.report(&Item::RegionStats).unwrap();
    assert!(table.as_str().unwrap().lines().any(|r| r.starts_with("R2map\t")), "{table}");
}

/// Uncombined coils and real/imaginary parts, in memory: the coils combined by MCPC-3D-S,
/// the parts converted (GE's phase sign undone), and either one reconstructed from there.
#[test]
fn coils_and_real_imaginary_parts_run_in_memory() {
    let (facts, store) = phantom();
    let echo = |item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!() };
    let (phases, mags): (Vec<_>, Vec<_>) = (1..=TES.len()).map(|e| (echo(Item::AcquiredPhase(e)), echo(Item::AcquiredMagnitude(e)))).unzip();
    let mut affine = [0.0; 16];
    for i in 0..4 { affine[5 * i] = 1.0; }
    let header = |manufacturer: &str| {
        let sidecar: crate::scan::QsmSidecar = serde_json::from_value(json!({
            "EchoTime": 0.005, "MagneticFieldStrength": 3.0, "Manufacturer": manufacturer })).unwrap();
        Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None, sidecar: Some(sidecar) }
    };

    // Two coils, each seeing the scan with its own smooth sensitivity and phase offset.
    let mut coils = MemoryStore::new(header("Siemens"), json!("coils"));
    for (c, offset) in [(1, 0.4), (2, -0.9)] {
        for e in 1..=TES.len() {
            let (p, m) = (&phases[e - 1], &mags[e - 1]);
            let weight = |i: usize| 0.5 + (i % N) as f64 / N as f64 * if c == 1 { 1.0 } else { -1.0 } * 0.4;
            let phase: Vec<f64> = p.iter().map(|&v| (v + offset + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).collect();
            coils.insert(Item::CoilPhase(c, e), VolumeData::Real(phase));
            coils.insert(Item::CoilMagnitude(c, e), VolumeData::Real(m.iter().enumerate().map(|(i, &v)| v * weight(i)).collect()));
        }
    }
    let mut f = facts.clone();
    f.n_coils = 2;
    let config = config();
    let events = run_once(&config, &f, &mut coils);
    assert!(events.computed.contains(&"scale_phase".to_string()) && events.computed.contains(&"reference".to_string()));
    for e in 1..=TES.len() {
        assert!(coils.get(&Item::CombinedEchoPhase(e)).is_some() && coils.get(&Item::Phase(e)).is_some());
    }
    assert!(coils.get(&Item::CoilCombinationMask).is_some());
    assert!(run_once(&config, &f, &mut coils).computed.is_empty());

    // Real and imaginary parts from a GE scanner, whose phase has the opposite sign.
    let mut parts = MemoryStore::new(header("GE MEDICAL SYSTEMS"), json!("parts"));
    for e in 1..=TES.len() {
        let (p, m) = (&phases[e - 1], &mags[e - 1]);
        parts.insert(Item::AcquiredReal(e), VolumeData::Real(p.iter().zip(m).map(|(&p, &m)| m * p.cos()).collect()));
        parts.insert(Item::AcquiredImaginary(e), VolumeData::Real(p.iter().zip(m).map(|(&p, &m)| -m * p.sin()).collect()));
    }
    let mut f = facts.clone();
    f.real_imaginary = true;
    run_once(&config, &f, &mut parts);
    let (Some(VolumeData::Real(converted)), Some(VolumeData::Real(magnitude))) =
        (parts.get(&Item::Phase(2)), parts.get(&Item::Magnitude(2))) else { panic!("not converted") };
    // The phase comes back as given: wrapped, so compared on the circle.
    assert!(converted.iter().zip(&phases[1]).all(|(&a, &b)| (a - b).sin().abs() < 1e-9 && (a - b).cos() > 0.0));
    assert!(magnitude.iter().zip(&mags[1]).all(|(a, b)| (a - b).abs() < 1e-9));
    assert!(parts.get(&Item::Chi).is_some());
}

/// Combined coils and converted real/imaginary parts of an oblique scan, windowed and
/// resampled: the echoes the single step gave (combined or converted, windowed on the acquired
/// grid, then resampled), and the combined echoes published as they are on the working grid.
#[test]
fn combined_and_converted_echoes_are_windowed_then_resampled_as_before() {
    let (cos, sin) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let affine = [1.0, 0.0, 0.0, 0.0, 0.0, cos, -sin, 0.0, 0.0, sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0];
    let (facts, store) = phantom_on(affine);
    let echo = |item: Item| match store.get(&item) { Some(VolumeData::Real(v)) => v.clone(), _ => panic!() };
    let (phases, mags): (Vec<_>, Vec<_>) = (1..=TES.len()).map(|e| (echo(Item::AcquiredPhase(e)), echo(Item::AcquiredMagnitude(e)))).unzip();
    let header = Header { dims: (N, N, N), voxel_size: (1.0, 1.0, 1.0), affine, orientation: None, sidecar: None };
    let mut c = config();
    (c.pipeline.do_qsm, c.pipeline.do_r2starmap) = (false, true);
    (c.pipeline.axial_resampling, c.pipeline.obliquity_threshold, c.pipeline.tukey_window) = (true, 5.0, 0.3);
    // Left on the working grid, so the published combined echoes are compared there.
    c.pipeline.output_space = crate::OutputSpace::Working;
    let resampled = |store: &MemoryStore| store.grid().unwrap().clone();

    let mut coils = MemoryStore::new(header.clone(), json!("coils"));
    let (mut coil_phases, mut coil_mags) = (Vec::new(), Vec::new());
    for (coil, offset) in [(1, 0.4), (2, -0.9)] {
        let (mut cp, mut cm) = (Vec::new(), Vec::new());
        for e in 1..=TES.len() {
            let phase: Vec<f64> = phases[e - 1].iter()
                .map(|&v| (v + offset + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).collect();
            let mag: Vec<f64> = mags[e - 1].iter().enumerate().map(|(i, &v)| v * (0.5 + (i % N) as f64 / (2 * N * coil) as f64)).collect();
            coils.insert(Item::CoilPhase(coil, e), VolumeData::Real(phase.clone()));
            coils.insert(Item::CoilMagnitude(coil, e), VolumeData::Real(mag.clone()));
            cp.push(phase);
            cm.push(mag);
        }
        coil_phases.push(cp);
        coil_mags.push(cm);
    }
    let f = ScanFacts { n_coils: 2, ..facts.clone() };
    run_once(&c, &f, &mut coils);
    let meta = resampled(&coils);
    assert!(meta.source_geometry.is_some());
    {
        let mut all: Vec<&mut Vec<f64>> = coil_phases.iter_mut().flatten().collect();
        qsm_core::pipeline::scale_phases_to_pi(&mut all);
    }
    let combined = crate::stages::phase::combine_coils(&coil_phases, &coil_mags, &meta, &c).unwrap();
    for e in 1..=TES.len() {
        let (mut m, mut p) = (combined.magnitudes[e - 1].clone(), combined.phases[e - 1].clone());
        crate::stages::phase::tukey_window(&mut m, &mut p, &meta, &c);
        let want = crate::stages::phase::working_echo(p, Some(m), true, &meta);
        let (phase, magnitude) = (VolumeData::Real(want.phase), VolumeData::Real(want.magnitude.unwrap()));
        assert_eq!(coils.get(&Item::Phase(e)), Some(&phase), "echo {e}");
        assert_eq!(coils.get(&Item::Magnitude(e)), Some(&magnitude), "echo {e}");
        assert_eq!(coils.get(&Item::CombinedEchoPhase(e)), Some(&phase), "echo {e}");
        assert_eq!(coils.get(&Item::CombinedEchoMagnitude(e)), Some(&magnitude), "echo {e}");
    }
    // The mask the offsets were estimated in stays on the acquired grid.
    assert_eq!(coils.geometry(&Item::CoilCombinationMask).map(|g| g.dims), Some((N, N, N)));

    let mut parts = MemoryStore::new(header, json!("parts"));
    let (mut real, mut imaginary) = (Vec::new(), Vec::new());
    for e in 1..=TES.len() {
        let (p, m) = (&phases[e - 1], &mags[e - 1]);
        real.push(p.iter().zip(m).map(|(&p, &m)| m * p.cos()).collect::<Vec<f64>>());
        imaginary.push(p.iter().zip(m).map(|(&p, &m)| m * p.sin()).collect::<Vec<f64>>());
        parts.insert(Item::AcquiredReal(e), VolumeData::Real(real[e - 1].clone()));
        parts.insert(Item::AcquiredImaginary(e), VolumeData::Real(imaginary[e - 1].clone()));
    }
    let f = ScanFacts { real_imaginary: true, ..facts };
    run_once(&c, &f, &mut parts);
    let meta = resampled(&parts);
    let (converted, _) = crate::stages::phase::real_imaginary(&real, &imaginary, &meta, false);
    for e in 1..=TES.len() {
        let (mut m, mut p) = (converted.magnitudes[e - 1].clone(), converted.phases[e - 1].clone());
        crate::stages::phase::tukey_window(&mut m, &mut p, &meta, &c);
        let want = crate::stages::phase::working_echo(p, Some(m), true, &meta);
        assert_eq!(parts.get(&Item::Phase(e)), Some(&VolumeData::Real(want.phase)), "echo {e}");
        assert_eq!(parts.get(&Item::Magnitude(e)), Some(&VolumeData::Real(want.magnitude.unwrap())), "echo {e}");
    }
}
