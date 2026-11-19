/// The statistics need something to average over and something to average, so asking for them
/// forces both — and a supplied input satisfies its own side without forcing a recomputation.
#[test]
fn analysis_forces_segmentation_and_qsm() {
    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_analysis = true;
    crate::config::enforce_analysis_dependencies(&mut c);
    assert!(c.pipeline.do_segmentation);
    assert!(c.pipeline.do_qsm);

    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_analysis = true;
    c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
    c.separation.custom_qsm_tool = Some("*".to_string());
    crate::config::enforce_analysis_dependencies(&mut c);
    assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");
    assert!(!c.pipeline.do_qsm, "a supplied Chimap needs no reconstruction");

    // Any map satisfies the map side: the table summarises whatever the run made, so asking
    // for R2* and the statistics must not quietly switch reconstruction back on.
    for other in ["t2star", "r2star", "r2", "r2prime", "chisep"] {
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_analysis = true;
        match other {
            "t2star" => c.pipeline.do_t2starmap = true,
            "r2star" => c.pipeline.do_r2starmap = true,
            "r2" => c.pipeline.do_r2map = true,
            "r2prime" => c.pipeline.do_r2primemap = true,
            _ => c.pipeline.do_chi_separation = true,
        }
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm, "{other} is a map to summarise — QSM must stay off");
        assert!(c.pipeline.do_segmentation, "{other} still needs something to average over");
    }

    // Segmentation on its own forces nothing: it reads the magnitude, not χ.
    let mut c = crate::config::PipelineConfig::default();
    c.pipeline.do_qsm = false;
    c.pipeline.do_segmentation = true;
    crate::config::enforce_analysis_dependencies(&mut c);
    assert!(!c.pipeline.do_qsm);
}
