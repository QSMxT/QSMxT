use crate::config::{enforce_separation_dependencies, PipelineConfig};
use crate::enums::R2PrimeStrategy;

/// R2' needs R2* on every route — it is the minuend of R2* - R2 and R2PRIMEnet's only input.
/// R2 is needed only where R2' can actually be *measured*; forcing a MESE-derived R2 map for a
/// run that is going to predict R2' instead would be work with nothing to consume it.
#[test]
fn only_the_measuring_routes_force_r2() {
    let cfg = |s| {
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = s;
        enforce_separation_dependencies(&mut c);
        c
    };

    for s in [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese] {
        let c = cfg(s);
        assert!(c.pipeline.do_r2map, "{s} can measure R2', so it needs R2");
        assert!(c.pipeline.do_r2starmap, "{s} needs R2*");
    }

    let c = cfg(R2PrimeStrategy::R2primenet);
    assert!(!c.pipeline.do_r2map, "predicting R2' subtracts nothing, so it needs no R2");
    assert!(c.pipeline.do_r2starmap, "R2* is R2PRIMEnet's input");

    // A supplied R2' map means none of it has to be computed.
    let mut c = PipelineConfig::default();
    c.pipeline.do_r2primemap = true;
    c.separation.custom_r2prime_tool = Some("*".to_string());
    enforce_separation_dependencies(&mut c);
    assert!(!c.pipeline.do_r2map);
    assert!(!c.pipeline.do_r2starmap);
}

/// Chi-separation forces R2', which in turn must not drag in R2 on the predicting route.
#[test]
fn chi_separation_inherits_the_strategy() {
    let mut c = PipelineConfig::default();
    c.pipeline.do_chi_separation = true;
    c.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
    c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet;
    enforce_separation_dependencies(&mut c);
    assert!(c.pipeline.do_r2primemap, "the method consumes R2'");
    assert!(c.pipeline.do_r2starmap);
    assert!(!c.pipeline.do_r2map, "R2PRIMEnet needs no spin-echo R2");
}

#[test]
fn auto_is_the_default() {
    assert_eq!(PipelineConfig::default().separation.r2prime_strategy, R2PrimeStrategy::Auto);
}
