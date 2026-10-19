use super::*;
use serde_json::json;

const AFFINE: [f64; 16] = [1., 0., 0., -2., 0., 1., 0., -2., 0., 0., 1., -2., 0., 0., 0., 1.];

fn grid(dims: (usize, usize, usize)) -> Geometry {
    Geometry { dims, voxel_size: (1.0, 1.0, 1.0), affine: AFFINE }
}

fn g() -> Option<Geometry> {
    Some(grid((4, 4, 4)))
}

fn sidecar(te: f64) -> serde_json::Value {
    json!({ "EchoTime": te, "MagneticFieldStrength": 3.0, "Manufacturer": "Siemens" })
}

fn echo(te: Option<f64>, magnitude: bool) -> EchoInput {
    EchoInput {
        phase: g(),
        magnitude: magnitude.then(|| grid((4, 4, 4))),
        sidecar: te.map(sidecar),
        ..Default::default()
    }
}

fn parts(te: f64, manufacturer: Option<&str>) -> EchoInput {
    let mut sc = json!({ "EchoTime": te, "MagneticFieldStrength": 3.0 });
    if let Some(m) = manufacturer {
        sc["Manufacturer"] = m.into();
    }
    EchoInput { real: g(), imaginary: g(), sidecar: Some(sc), ..Default::default() }
}

fn coils(te: f64, n: usize) -> EchoInput {
    EchoInput {
        coils: (0..n).map(|_| CoilInput { phase: g(), magnitude: g() }).collect(),
        sidecar: Some(sidecar(te)),
        ..Default::default()
    }
}

fn mese(tes: &[f64]) -> MeseInput {
    MeseInput {
        name: Some("sub-1_MESE".into()),
        echoes: tes.iter().map(|&te| MeseEchoInput {
            magnitude: Some(grid((4, 4, 2))), sidecar: Some(json!({ "EchoTime": te })),
        }).collect(),
    }
}

fn shown(report: &ScanReport, name: FactName) -> &Fact {
    report.shown.iter().find(|f| f.name == name).unwrap()
}

fn source(report: &ScanReport, name: FactName) -> Source {
    shown(report, name).source
}

fn keys(report: &ScanReport) -> Vec<&str> {
    report.files.iter().map(|f| f.key.as_str()).collect()
}

#[test]
fn facts_come_from_the_headers_and_sidecars() {
    let inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let facts = report.facts.clone().unwrap();
    assert_eq!((facts.dims, facts.n_echoes, facts.has_magnitude), ((4, 4, 4), 2, true));
    assert_eq!(facts.echo_times, [0.004, 0.008]);
    assert_eq!((facts.field_strength, facts.b0_dir), (3.0, None));
    assert_eq!((facts.n_coils, facts.real_imaginary, facts.mese_echo_times.clone()), (1, false, None));
    assert_eq!(source(&report, FactName::Dims), Source::Header);
    assert_eq!(source(&report, FactName::EchoTimes), Source::Sidecar);
    assert_eq!(source(&report, FactName::FieldStrength), Source::Sidecar);
    assert_eq!(source(&report, FactName::B0Dir), Source::Missing);
    assert_eq!(source(&report, FactName::Manufacturer), Source::Sidecar);
    assert_eq!(shown(&report, FactName::Coils).value, json!(1));
    assert_eq!(shown(&report, FactName::RealImaginary).value, json!(false));
    assert_eq!(shown(&report, FactName::MeseEchoes).value, json!(0));
    assert_eq!(keys(&report), ["AcquiredPhase(1)", "AcquiredMagnitude(1)", "AcquiredPhase(2)", "AcquiredMagnitude(2)"]);
    // The sidecars a run reads keep what the files say.
    assert_eq!(report.sidecars.len(), 2);
    assert_eq!(report.sidecars[1].echo_time, 0.008);
    assert_eq!(report.sidecars[0].manufacturer.as_deref(), Some("Siemens"));
    // The same facts as from the sidecars alone.
    let sidecars: Vec<_> = inputs.echoes.iter().map(|e| QsmSidecar::from_json(e.sidecar.as_ref().unwrap()).unwrap()).collect();
    assert_eq!(report.facts, Some(ScanFacts::from_sidecars((4, 4, 4), &sidecars, true).unwrap()));
}

#[test]
fn typed_values_win_and_fill_what_the_files_lack() {
    let mut inputs = ScanInputs { echoes: vec![echo(None, false), echo(None, false)], ..Default::default() };
    let report = inputs.facts();
    assert!(report.facts.is_none());
    let missing: Vec<_> = report.problems.iter().map(|p| (p.fact, p.echo)).collect();
    assert_eq!(missing, [(Some(FactName::EchoTimes), Some(1)), (Some(FactName::EchoTimes), Some(2)),
        (Some(FactName::FieldStrength), None)]);
    assert!(report.problems.iter().all(|p| p.severity == Severity::Error));

    inputs.overrides = FactOverrides {
        echo_times: Some(vec![0.005, 0.01]), field_strength: Some(7.0), b0_dir: Some((0.0, 0.1, 0.99)),
        manufacturer: Some("Philips".into()), mese_echo_times: None,
    };
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let facts = report.facts.clone().unwrap();
    assert_eq!((facts.echo_times.clone(), facts.field_strength, facts.b0_dir), (vec![0.005, 0.01], 7.0, Some((0.0, 0.1, 0.99))));
    assert!(!facts.has_magnitude);
    for name in [FactName::EchoTimes, FactName::FieldStrength, FactName::B0Dir, FactName::Manufacturer] {
        assert_eq!(source(&report, name), Source::Typed);
    }
    assert_eq!(report.sidecars[1].echo_time, 0.01);
    assert_eq!(report.sidecars[0].b0_dir, Some(vec![0.0, 0.1, 0.99]));
    assert_eq!(report.sidecars[1].manufacturer.as_deref(), Some("Philips"));

    // A typed value wins over a sidecar's.
    let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true)], ..Default::default() };
    inputs.overrides.field_strength = Some(2.89);
    let report = inputs.facts();
    assert_eq!(report.facts.unwrap().field_strength, 2.89);
    assert_eq!(report.sidecars[0].magnetic_field_strength, 2.89);

    // Typed echo times must be one per echo.
    inputs.overrides.echo_times = Some(vec![0.004, 0.008]);
    let report = inputs.facts();
    assert!(report.facts.is_none());
    assert_eq!(report.problems[0].fact, Some(FactName::EchoTimes));
}

#[test]
fn images_must_share_a_grid_and_magnitude_is_all_or_none() {
    let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
    inputs.echoes[1].magnitude = Some(grid((4, 4, 5)));
    let report = inputs.facts();
    let p = &report.problems[0];
    assert_eq!((p.severity, p.echo, p.part), (Severity::Error, Some(2), Some(Part::Magnitude)));
    assert!(p.message.contains("4×4×5") && p.message.contains("4×4×4"), "{}", p.message);
    assert!(report.has_errors());

    // Same size, moved: a warning.
    let mut moved = grid((4, 4, 4));
    moved.affine[3] += 5.0;
    inputs.echoes[1].magnitude = Some(moved);
    let report = inputs.facts();
    assert_eq!(report.problems.len(), 1);
    assert_eq!(report.problems[0].severity, Severity::Warning);
    assert!(report.facts.is_some() && !report.has_errors());

    // Magnitude for some echoes only.
    inputs.echoes[1].magnitude = None;
    let report = inputs.facts();
    assert_eq!(report.problems.len(), 1);
    assert_eq!((report.problems[0].fact, report.problems[0].echo), (Some(FactName::Magnitude), Some(2)));

    // An echo without its phase image.
    inputs.echoes[1].phase = None;
    assert!(inputs.facts().problems.iter().any(|p| p.part == Some(Part::Phase) && p.echo == Some(2)));
    assert!(ScanInputs::default().facts().has_errors());
}

#[test]
fn b0_direction_is_read_as_qsmxt_reads_it() {
    let mut inputs = ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], ..Default::default() };
    inputs.echoes[0].sidecar.as_mut().unwrap()["B0_dir"] = json!([0.0, 0.1, 0.99]);
    inputs.echoes[1].sidecar.as_mut().unwrap()["B0_dir"] = json!([1.0]);
    let report = inputs.facts();
    assert_eq!(report.facts.as_ref().unwrap().b0_dir, Some((0.0, 0.1, 0.99)));
    assert_eq!(source(&report, FactName::B0Dir), Source::Sidecar);
    assert_eq!(report.problems.len(), 1);
    assert_eq!(report.problems[0].severity, Severity::Warning);
}

#[test]
fn real_and_imaginary_parts_carry_the_manufacturer_to_the_run() {
    // Only echo 2's sidecar names the manufacturer; the run reads echo 1's.
    let inputs = ScanInputs { echoes: vec![parts(0.004, None), parts(0.008, Some("GE MEDICAL SYSTEMS"))], ..Default::default() };
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let facts = report.facts.clone().unwrap();
    assert!(facts.real_imaginary && facts.has_magnitude && facts.n_coils == 1);
    assert_eq!(keys(&report), ["AcquiredReal(1)", "AcquiredImaginary(1)", "AcquiredReal(2)", "AcquiredImaginary(2)"]);
    assert_eq!(shown(&report, FactName::RealImaginary).value, json!(true));
    let manufacturer = shown(&report, FactName::Manufacturer);
    assert_eq!((manufacturer.value.clone(), manufacturer.source), (json!("GE MEDICAL SYSTEMS"), Source::Sidecar));
    assert!(manufacturer.note.as_deref().unwrap().contains("negated"));
    let header = inputs.header(&report).unwrap();
    assert_eq!(header.sidecar.unwrap().manufacturer.as_deref(), Some("GE MEDICAL SYSTEMS"));

    // Unknown: a warning, and typing it fixes that.
    let mut inputs = ScanInputs { echoes: vec![parts(0.004, None), parts(0.008, None)], ..Default::default() };
    let report = inputs.facts();
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert_eq!((report.problems[0].severity, report.problems[0].fact), (Severity::Warning, Some(FactName::Manufacturer)));
    inputs.overrides.manufacturer = Some("GE".into());
    let report = inputs.facts();
    assert!(report.problems.is_empty());
    assert_eq!(report.sidecars[0].manufacturer.as_deref(), Some("GE"));

    // A missing part, a magnitude image beside the parts, and an echo given another way.
    inputs.echoes[0].imaginary = None;
    inputs.echoes[1].magnitude = g();
    inputs.echoes.push(echo(Some(0.012), false));
    let report = inputs.facts();
    let found: Vec<_> = report.problems.iter().map(|p| (p.severity, p.echo, p.part)).collect();
    assert!(found.contains(&(Severity::Error, Some(1), Some(Part::Imaginary))), "{found:?}");
    assert!(found.contains(&(Severity::Warning, Some(2), Some(Part::Magnitude))), "{found:?}");
    assert!(found.contains(&(Severity::Error, Some(3), None)), "{found:?}");
    assert!(!keys(&report).contains(&"AcquiredMagnitude(2)"));
}

#[test]
fn uncombined_coils_are_counted_and_checked() {
    let mut inputs = ScanInputs { echoes: vec![coils(0.004, 3), coils(0.008, 3)], ..Default::default() };
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let facts = report.facts.clone().unwrap();
    assert_eq!((facts.n_coils, facts.has_magnitude, facts.real_imaginary), (3, true, false));
    assert_eq!(shown(&report, FactName::Coils).value, json!(3));
    assert_eq!(report.files.len(), 12);
    assert_eq!(keys(&report)[..4], ["CoilPhase(1, 1)", "CoilMagnitude(1, 1)", "CoilPhase(2, 1)", "CoilMagnitude(2, 1)"]);
    assert_eq!(report.files[11].coil, Some(3));
    for f in &report.files {
        assert_eq!(Item::from_key(&f.key).and_then(|i| i.echo()), Some(f.echo));
    }

    // A coil short of a magnitude image, a moved image and a coil too few.
    inputs.echoes[0].coils[1].magnitude = None;
    inputs.echoes[1].coils[2].phase = Some(grid((4, 4, 3)));
    inputs.echoes[1].coils.pop();
    let report = inputs.facts();
    let found: Vec<_> = report.problems.iter().map(|p| (p.fact, p.echo, p.coil, p.part)).collect();
    assert!(found.contains(&(None, Some(1), Some(2), Some(Part::Magnitude))), "{found:?}");
    assert!(found.contains(&(Some(FactName::Coils), Some(2), None, None)), "{found:?}");
    assert!(report.has_errors());

    // One echo cannot be coil-combined (the rule the plan checks too).
    let report = ScanInputs { echoes: vec![coils(0.004, 2)], ..Default::default() }.facts();
    assert!(report.problems.iter().any(|p| p.fact == Some(FactName::Coils) && p.message.contains("at least 2 echoes")));

    // One coil is combined data.
    let report = ScanInputs { echoes: vec![coils(0.004, 1), coils(0.008, 1)], ..Default::default() }.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.facts.as_ref().unwrap().n_coils, 1);
    assert_eq!(keys(&report), ["AcquiredPhase(1)", "AcquiredMagnitude(1)", "AcquiredPhase(2)", "AcquiredMagnitude(2)"]);
    assert!(shown(&report, FactName::Coils).note.is_some());
}

#[test]
fn a_mese_gives_its_echo_times_and_grid() {
    let mut inputs = ScanInputs {
        echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)],
        mese: Some(mese(&[0.01, 0.02, 0.03])),
        ..Default::default()
    };
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.facts.as_ref().unwrap().mese_echo_times, Some(vec![0.01, 0.02, 0.03]));
    assert_eq!(shown(&report, FactName::MeseEchoes).value, json!(3));
    assert_eq!(source(&report, FactName::MeseEchoTimes), Source::Sidecar);
    let mese_files: Vec<_> = report.files.iter().filter(|f| f.own_grid).map(|f| (f.key.as_str(), f.echo)).collect();
    assert_eq!(mese_files, [("MeseMagnitude(1)", 1), ("MeseMagnitude(2)", 2), ("MeseMagnitude(3)", 3)]);
    let m = inputs.mese(&report).unwrap();
    assert_eq!((m.name.as_str(), m.dims, m.echo_times.clone()), ("sub-1_MESE", (4, 4, 2), vec![0.01, 0.02, 0.03]));

    // Typed echo times win; a missing one, or an echo on another grid, leaves no MESE to use.
    inputs.overrides.mese_echo_times = Some(vec![0.012, 0.024, 0.036]);
    let report = inputs.facts();
    assert_eq!(report.facts.as_ref().unwrap().mese_echo_times, Some(vec![0.012, 0.024, 0.036]));
    assert_eq!(source(&report, FactName::MeseEchoTimes), Source::Typed);
    inputs.overrides.mese_echo_times = None;
    inputs.mese.as_mut().unwrap().echoes[1].sidecar = None;
    let report = inputs.facts();
    let p = &report.problems[0];
    assert_eq!((p.fact, p.echo, p.part), (Some(FactName::MeseEchoTimes), Some(2), Some(Part::MeseSidecar)));
    assert_eq!(report.facts.as_ref().unwrap().mese_echo_times, None);
    assert!(inputs.mese(&report).is_none());
    inputs.mese.as_mut().unwrap().echoes[1].sidecar = Some(json!({ "EchoTime": 0.02 }));
    inputs.mese.as_mut().unwrap().echoes[2].magnitude = Some(grid((4, 4, 3)));
    let report = inputs.facts();
    assert_eq!(report.problems[0].part, Some(Part::MeseMagnitude));
    assert_eq!(report.facts.as_ref().unwrap().mese_echo_times, None);
}

/// The store keys the report gives each file are the ones the plan's load step writes, for every
/// way a scan can be given.
#[test]
fn files_are_kept_where_the_run_reads_them() {
    let scans = [
        ScanInputs { echoes: vec![echo(Some(0.004), true), echo(Some(0.008), true)], mese: Some(mese(&[0.01, 0.02, 0.03])), ..Default::default() },
        ScanInputs { echoes: vec![echo(Some(0.004), false), echo(Some(0.008), false)], ..Default::default() },
        ScanInputs { echoes: vec![parts(0.004, Some("GE")), parts(0.008, Some("GE"))], ..Default::default() },
        ScanInputs { echoes: vec![coils(0.004, 2), coils(0.008, 2)], ..Default::default() },
    ];
    for inputs in scans {
        let report = inputs.facts();
        let plan = crate::plan::plan(&crate::PipelineConfig::default(), report.facts.as_ref().unwrap());
        let load = plan.steps.iter().find(|s| s.stage == crate::plan::Stage::Load).unwrap();
        let mut written: Vec<String> = load.outputs.iter().map(Item::key).collect();
        let mut files: Vec<String> = keys(&report).into_iter().map(str::to_string).collect();
        written.sort();
        files.sort();
        assert_eq!(files, written, "{:?}", inputs.encoding());
    }
}

/// A store built from the report holds the header, the sidecar and the MESE the run reads.
#[test]
fn a_store_is_built_from_the_report() {
    use crate::store::{StageStore, Supplied};
    let mut inputs = ScanInputs {
        echoes: vec![parts(0.004, None), parts(0.008, Some("GE"))],
        mese: Some(mese(&[0.01, 0.02, 0.03])),
        ..Default::default()
    };
    // The headers as the store's volumes give them.
    inputs.read_headers(|item| Ok::<_, ()>(match item {
        Item::MeseMagnitude(_) => Some(grid((4, 4, 2))),
        _ => Some(grid((5, 5, 5))),
    })).unwrap();
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.facts.as_ref().unwrap().dims, (5, 5, 5));
    let mut store = MemoryStore::new(inputs.header(&report).unwrap(), json!(1));
    for f in report.files.iter().filter(|f| f.own_grid) {
        store.insert(Item::from_key(&f.key).unwrap(), crate::observe::VolumeData::Real(vec![1.0; 32]));
    }
    inputs.supply(&report, &mut store);
    assert_eq!(store.sidecar().unwrap().unwrap().manufacturer.as_deref(), Some("GE"));
    assert_eq!(store.header().unwrap().dims, (5, 5, 5));
    assert_eq!(store.mese().unwrap().echo_times, [0.01, 0.02, 0.03]);
    assert!(matches!(store.supplied(&Item::MeseMagnitude(3)), Supplied::Found { dims: (4, 4, 2), .. }));
}

#[test]
fn the_report_serialises_for_a_front_end() {
    let inputs: ScanInputs = serde_json::from_value(json!({
        "echoes": [{ "phase": { "dims": [4, 4, 4], "voxel_size": [1, 1, 1], "affine": AFFINE },
                     "sidecar": { "EchoTime": 0.004 } }],
        "overrides": { "field_strength": 3.0 },
    })).unwrap();
    let value = serde_json::to_value(inputs.facts()).unwrap();
    assert_eq!(value["facts"]["field_strength"], 3.0);
    assert_eq!(value["shown"][4]["name"], "field_strength");
    assert_eq!(value["shown"][4]["source"], "typed");
    assert_eq!(value["shown"][4]["label"], "Field strength");
    assert_eq!(value["sidecars"][0]["EchoTime"], 0.004);
    assert_eq!(value["problems"], json!([]));
    assert_eq!(value["files"], json!([{ "key": "AcquiredPhase(1)", "echo": 1, "coil": null, "part": "phase", "own_grid": false }]));

    // The other ways, as JSON.
    let geometry = json!({ "dims": [4, 4, 4], "voxel_size": [1, 1, 1], "affine": AFFINE });
    let inputs: ScanInputs = serde_json::from_value(json!({
        "echoes": [
            { "coils": [{ "phase": geometry, "magnitude": geometry }, { "phase": geometry, "magnitude": geometry }],
              "sidecar": { "EchoTime": 0.004, "MagneticFieldStrength": 3.0 } },
            { "coils": [{ "phase": geometry, "magnitude": geometry }, { "phase": geometry, "magnitude": geometry }],
              "sidecar": { "EchoTime": 0.008, "MagneticFieldStrength": 3.0 } },
        ],
        "mese": { "echoes": [{ "magnitude": geometry }, { "magnitude": geometry }, { "magnitude": geometry }] },
        "overrides": { "mese_echo_times": [0.01, 0.02, 0.03] },
    })).unwrap();
    let report = inputs.facts();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["facts"]["n_coils"], 2);
    assert_eq!(value["facts"]["mese_echo_times"], json!([0.01, 0.02, 0.03]));
    let inputs: ScanInputs = serde_json::from_value(json!({
        "echoes": [{ "real": geometry, "imaginary": geometry, "sidecar": { "EchoTime": 0.004 } }],
        "overrides": { "field_strength": 3.0, "manufacturer": "GE" },
    })).unwrap();
    assert!(inputs.facts().facts.unwrap().real_imaginary);
    // Today's shape serialises as it was.
    let today = serde_json::to_value(ScanInputs { echoes: vec![echo(Some(0.004), true)], ..Default::default() }).unwrap();
    assert_eq!(today["echoes"][0].as_object().unwrap().keys().collect::<Vec<_>>(), ["phase", "magnitude", "sidecar"]);
    assert!(today.get("mese").is_none());
}
