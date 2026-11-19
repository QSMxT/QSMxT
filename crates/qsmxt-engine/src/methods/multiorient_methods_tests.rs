use super::*;

#[test]
fn nothing_is_said_when_grouping_is_off() {
    let text = generate_methods(&PipelineConfig::default());
    assert!(!text.contains("COSMOS"), "{text}");
    assert!(!text.contains("orientation"), "{text}");
}

#[test]
fn cosmos_is_described_and_cited() {
    let mut config = PipelineConfig::default();
    config.multi_orientation.group_by = "*acq-dir*".into();
    let text = generate_methods(&config);
    assert!(text.contains("Multiple Orientation Sampling (COSMOS; Liu et al., 2009)"), "{text}");
    assert!(text.contains("10.1002/mrm.21828"), "the reference list should carry it: {text}");
    // Default lambda is zero, so it should not be claimed.
    assert!(!text.contains("lambda"), "{text}");
}

#[test]
fn sti_is_described_and_cited() {
    let mut config = PipelineConfig::default();
    config.multi_orientation.group_by = "acq".into();
    config.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
    config.multi_orientation.lambda = 1e-3;
    let text = generate_methods(&config);
    assert!(text.contains("susceptibility tensor imaging (STI; Liu, 2010)"), "{text}");
    assert!(text.contains("10.1002/mrm.22482"), "{text}");
    assert!(text.contains("lambda = 0.001"), "{text}");
}

/// How the orientations reached a common grid is part of the method — it is where the B0
/// directions came from — so the paragraph has to say which of the two it was.
#[test]
fn the_methods_say_how_the_orientations_were_registered() {
    let mut config = PipelineConfig::default();
    config.multi_orientation.group_by = "acq".into();
    let text = generate_methods(&config);
    assert!(text.contains("rigidly co-registered"), "{text}");
    assert!(text.contains("derived from the recovered rotation"), "{text}");

    config.multi_orientation.register = false;
    let text = generate_methods(&config);
    assert!(text.contains("taken to be already co-registered"), "{text}");
    assert!(!text.contains("rigidly co-registered"), "{text}");
}

/// The methods paragraph says what was done: no tool provenance (STI Suite, UK Biobank),
/// no QSMxT version history, no "rather than". Literature citations in the reference list
/// are fine, so only the paragraph itself is checked.
#[test]
fn methods_paragraph_has_no_provenance_or_contrasts() {
    use crate::enums::{B0WeightType, LaplacianSolver, UnwrappingAlgorithm};
    let mut configs = Vec::new();
    for alg in &QsmAlgorithm::ALL {
        for laplacian in [false, true] {
            let mut c = PipelineConfig::default();
            c.inversion.algorithm = *alg;
            c.pipeline.do_segmentation = true;
            c.pipeline.do_chi_separation = true;
            c.pipeline.do_r2primemap = true;
            c.pipeline.do_r2map = true;
            c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay;
            if laplacian {
                c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
                c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
            }
            c.qsm.reference = QsmReference::Region;
            c.qsm.reference_region = Some("ventricles".to_string());
            configs.push(c);
        }
    }
    for c in &configs {
        let out = generate_methods(c);
        let paragraph = out.split("\n## ").next().unwrap().to_lowercase();
        // "set to zero outside": no zeroing outside a mask is described (the non-negative
        // network and R2' outputs are, as negative values set to zero).
        for banned in ["sti suite", "biobank", "ukb", "qsmxt v", "rather than", "without the", "set to zero outside"] {
            assert!(!paragraph.contains(banned), "`{banned}` in methods:\n{paragraph}");
        }
    }
}
