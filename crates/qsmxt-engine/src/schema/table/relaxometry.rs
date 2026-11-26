//! The Relaxometry section's settings.

use super::*;

pub(super) fn defs() -> Vec<Def> {
    vec![
        d("pipeline.do_t2starmap", Relaxometry, "Compute T2* map", "Compute T2* relaxation map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-t2starmap")
        .cli("Compute T2* relaxation map from multi-echo magnitude data"),
        d("pipeline.do_r2starmap", Relaxometry, "Compute R2* map", "Compute R2* decay rate map (requires 3+ echoes with magnitude)", Bool).basic().flag("--do-r2starmap")
        .cli("Compute R2* decay rate map from multi-echo magnitude data"),
        d("pipeline.do_r2map", Relaxometry, "Compute R2 map", "R2 map (Hz) from a multi-echo spin-echo (MESE) acquisition via EPG. Required when chi-separation needs it", Bool).basic().flag("--do-r2map")
        .gui("R2 map from a multi-echo spin-echo (MESE) acquisition via EPG. Required when χ-separation needs it")
        .cli("Compute R2 map from a multi-echo spin-echo (MESE) acquisition (EPG)"),
        d("separation.custom_r2_tool", Relaxometry, "Custom R2 tool", "Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE (blank = compute, * = any tool)", Text).basic().optional().flag("--use-custom-r2").parent("pipeline.do_r2map")
        .if_omitted("the R2 map is computed from MESE")
        .supplied_file(Item::SuppliedR2, "An R2 map to use instead of computing one from a MESE acquisition")
        .when(|x| x.reads_r2).full("Custom R2")
        .gui("Use an R2 map from <bids>/derivatives/<tool>/ instead of computing it from MESE: * for any tool, or a tool name; empty to compute it")
        .cli("Use a bring-your-own R2 map from <bids>/derivatives/<TOOL>/ for chi-separation")
        .value_name("TOOL"),
        d("pipeline.do_r2primemap", Relaxometry, "Compute R2' map", "R2' = R2* − R2 (Hz); needs GRE magnitude + a MESE acquisition, or R2PRIMEnet. Required when chi-separation needs it", Bool).basic().flag("--do-r2primemap")
        .gui("R2' = R2* − R2; needs GRE magnitude and a MESE acquisition, or R2PRIMEnet. Required when χ-separation needs it")
        .cli("Compute R2' map (= R2* − R2; needs GRE magnitude + a MESE acquisition)"),
        d("separation.custom_r2prime_tool", Relaxometry, "Custom R2' tool", "Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it (blank = compute, * = any tool)", Text).basic().optional().flag("--use-custom-r2prime").parent("pipeline.do_r2primemap")
        .if_omitted("the R2' map is computed")
        .supplied_file(Item::SuppliedR2prime, "An R2' map to use instead of computing one")
        .when(|x| x.reads_r2prime).full("Custom R2'")
        .gui("Use an R2' map from <bids>/derivatives/<tool>/ instead of computing it: * for any tool, or a tool name; empty to compute it")
        .cli("Use a bring-your-own R2' map from <bids>/derivatives/<TOOL>/ for chi-separation")
        .value_name("TOOL"),
        d("separation.r2prime_strategy", Relaxometry, "R2' source", "", Enum(vec![
            o("auto", "auto", "Automatic", "Measure R2' = R2* - R2 from a MESE acquisition when one is present, else predict it from R2* with R2PRIMEnet"),
            o("mese", "mese", "MESE", "Only ever measure R2' from a MESE acquisition - produces no R2' without spin-echo data"),
            o("r2primenet", "r2primenet", "R2PRIMEnet", "Always predict R2' from R2* with R2PRIMEnet, even when a MESE acquisition is available (an estimate, not a measurement)"),
        ])).basic().flag("--r2prime-strategy").parent("pipeline.do_r2primemap")
        .when(|x| x.computes_r2prime)
        .gui("How R2' is obtained when no R2' map is supplied")
        .cli("How to obtain R2' when no custom map is supplied. `auto` (default) measures it as R2* - R2 from a MESE acquisition when one is present and predicts it from R2* with R2PRIMEnet when it is not; `mese` only ever measures it, producing nothing without spin-echo data; `r2primenet` always predicts it. A measurement and an estimate are not the same thing — the generated methods text says which route was configured")
        .value_help("auto", "Measure from MESE where available, else predict with R2PRIMEnet").value_help("mese", "Only ever measure it (R2* - R2)").value_help("r2primenet", "Always predict it from R2*"),
        d("separation.r2primenet.ignore_field_mismatch", Relaxometry, "R2PRIMEnet off 3 T", "Run R2PRIMEnet on non-3 T data (trained at 3 T only; the R2' is uncalibrated). Off: auto skips R2', r2primenet errors", Bool).flag("--r2primenet-ignore-field-mismatch").parent("separation.r2prime_strategy")
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese).full("R2PRIMEnet away from 3 T")
        .cli("Run R2PRIMEnet even when the acquisition is not at 3 T, the only field strength its weights were trained at; the predicted R2' is then uncalibrated. Without it, an explicit `--r2prime-strategy r2primenet` refuses a non-3 T run and `auto` skips R2' with a warning"),
        // QSMxT R2primenetConfig::validate: each side a positive multiple of QSM.rs's
        // relaxometry::POOL_MULTIPLE (16; QSM.rs utils/r2primenet.rs asserts it). The texts' "16" is pinned to it by a test.
        d("separation.r2primenet.patch", Relaxometry, "R2PRIMEnet patch", "Sliding-window patch D H W in voxels, each a multiple of 16 (default 192 192 128)", IntVec3).min(qsm_core::relaxometry::POOL_MULTIPLE as f64).units("voxels").flag("--r2primenet-patch").parent("separation.r2prime_strategy")
        // χ-sepnet's toolbox route runs R2PRIMEnet on its own frame, not at this patch.
        .when(|x| x.computes_r2prime && x.c.separation.r2prime_strategy != R2PrimeStrategy::Mese
            && (x.c.pipeline.do_r2primemap || !crate::config::chisepnet_route_can_estimate_r2prime(x.c)))
        .gui("Sliding-window patch D H W, each a multiple of 16")
        .cli("R2PRIMEnet sliding-window patch in voxels, D H W, each a multiple of 16 (default: 192 192 128, the authors')")
        .value_names(&["D", "H", "W"]),
    ]
}
