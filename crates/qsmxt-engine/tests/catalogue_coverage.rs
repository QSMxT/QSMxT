//! Catalogue ↔ engine enums: every method a QSMxT setting can choose has an entry in QSM.rs's
//! method catalogue (`qsm_core::catalogue`), found by its stage and the id QSMxT writes for it, so
//! the methods text can take its name and citations from there. A method the catalogue lacks is
//! listed in `EXCEPTIONS` with why.

use qsm_core::catalogue::{find, Stage};
use qsmxt_engine::{
    B0Estimation, BfAlgorithm, InhomogeneityMethod, MaskOp, MaskThresholdMethod, QsmAlgorithm,
    SeparationAlgorithm, SynthSegVersion, UnwrappingAlgorithm,
};

/// Engine choices with no catalogue entry, each with why. None today.
const EXCEPTIONS: &[(Stage, &str, &str)] = &[];

/// A mask operation's id: its `op` tag in the configuration. The match is exhaustive, so a new
/// operation does not compile until it is given one.
fn mask_op_id(op: &MaskOp) -> &'static str {
    match op {
        MaskOp::Threshold { .. } => "threshold",
        MaskOp::Bet { .. } => "bet",
        MaskOp::Erode { .. } => "erode",
        MaskOp::Dilate { .. } => "dilate",
        MaskOp::Close { .. } => "close",
        MaskOp::MinIntensity { .. } => "min-intensity",
        MaskOp::Majority => "majority",
        MaskOp::FillHoles { .. } => "fill-holes",
        MaskOp::LargestComponent => "largest-component",
        MaskOp::Imfill => "imfill",
        MaskOp::GaussianSmooth { .. } => "gaussian-smooth",
        MaskOp::SignalErode { .. } => "signal-erode",
        MaskOp::HdBet { .. } => "hd-bet",
        MaskOp::Rs2Net { .. } => "rs2-net",
        MaskOp::MindGrab { .. } => "mindgrab",
    }
}

fn mask_ops() -> Vec<MaskOp> {
    vec![
        MaskOp::Threshold { method: MaskThresholdMethod::Otsu, value: None },
        MaskOp::bet(0.5),
        MaskOp::Erode { iterations: 1 },
        MaskOp::Dilate { iterations: 1 },
        MaskOp::Close { radius: 1 },
        MaskOp::MinIntensity { value: 0.0 },
        MaskOp::Majority,
        MaskOp::FillHoles { max_size: 1 },
        MaskOp::LargestComponent,
        MaskOp::Imfill,
        MaskOp::GaussianSmooth { sigma_mm: 1.0 },
        MaskOp::signal_erode_default(),
        MaskOp::hd_bet_default(),
        MaskOp::rs2_net_default(),
        MaskOp::mindgrab_default(),
    ]
}

fn mask_threshold_id(m: MaskThresholdMethod) -> &'static str {
    match m {
        MaskThresholdMethod::Otsu => "otsu",
        MaskThresholdMethod::Fixed => "fixed",
        MaskThresholdMethod::Percentile => "percentile",
        MaskThresholdMethod::StiGui => "sti-gui",
    }
}

/// Every engine choice of a method, as (stage, id).
fn engine_methods() -> Vec<(Stage, String)> {
    let mut all: Vec<(Stage, String)> = Vec::new();
    let mut add = |stage, ids: Vec<&str>| all.extend(ids.into_iter().map(|id| (stage, id.to_string())));
    add(Stage::Masking, mask_ops().iter().map(mask_op_id).collect());
    add(Stage::MaskThreshold, [MaskThresholdMethod::Otsu, MaskThresholdMethod::Fixed,
        MaskThresholdMethod::Percentile, MaskThresholdMethod::StiGui].into_iter().map(mask_threshold_id).collect());
    add(Stage::InhomogeneityCorrection, InhomogeneityMethod::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::Unwrapping, UnwrappingAlgorithm::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::EchoCombination, B0Estimation::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::BackgroundRemoval, BfAlgorithm::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::Inversion, QsmAlgorithm::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::Separation, SeparationAlgorithm::ALL.iter().map(|m| m.as_str()).collect());
    add(Stage::Segmentation, SynthSegVersion::ALL.iter().map(|v| v.weights_id()).collect());
    all
}

#[test]
fn mask_op_ids_are_their_config_tags() {
    for op in mask_ops() {
        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["op"], mask_op_id(&op), "{op:?}");
    }
}

#[test]
fn every_engine_method_is_in_the_catalogue() {
    let missing: Vec<String> = engine_methods().into_iter()
        .filter(|(stage, id)| !EXCEPTIONS.iter().any(|(s, i, _)| s == stage && i == id))
        .filter(|(stage, id)| find(*stage, id).is_none())
        .map(|(stage, id)| format!("{stage:?} {id}"))
        .collect();
    assert!(missing.is_empty(), "engine methods with no catalogue entry: {missing:?}");
}

#[test]
fn every_exception_is_still_missing() {
    for (stage, id, why) in EXCEPTIONS {
        assert!(find(*stage, id).is_none(), "{stage:?} {id} is in the catalogue now ({why}): drop the exception");
    }
}
