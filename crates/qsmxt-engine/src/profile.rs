//! The browser profile: QSMxT's defaults, changed only where the browser cannot run them.
//!
//! A browser (WASM) run has a 4 GB heap, and several deep-learning steps need more than that at
//! QSMxT's defaults: whole-volume inversion networks, HD-BET on the whole volume (9-13 GB),
//! SynthSeg on the whole volume (about 7 GB for a 1 mm whole-head scan) and R2PRIMEnet at its
//! authors' patch. The profile replaces those defaults with values that fit,
//! and writes them into the config, so the TOML, the generated `qsmxt run` command and the
//! methods text all carry them and QSMxT reproduces a browser run exactly.
//!
//! Everything else stays QSMxT's default.

use crate::config::{synthseg_low_memory_patch, PipelineConfig};
use crate::masking::{hd_bet_low_memory_patch, MaskOp, MaskSection};

/// Overlap-tiling core of the deep-learning inversions in the browser, in voxels. With
/// [`BROWSER_TILE_HALO`] it makes 64³ patches, inside qsm-core's largest WASM patch
/// (`qsm_core::inversion::max_wasm_patch_edge`, 131).
pub const BROWSER_TILE_SIZE: usize = 56;

/// Overlap-tiling context margin per side in the browser, in voxels.
pub const BROWSER_TILE_HALO: usize = 4;

/// R2PRIMEnet's sliding-window patch in the browser: qsm-core's `WASM_PATCH` for it
/// (`qsm_core::relaxometry::WASM_PATCH`, only compiled with qsm-core's `onnx` feature).
pub const BROWSER_R2PRIMENET_PATCH: [usize; 3] = [128, 128, 64];

impl PipelineConfig {
    /// QSMxT's defaults with the browser profile applied: what a browser run starts from.
    pub fn browser() -> Self {
        let mut c = Self::default();
        c.apply_browser_profile();
        c
    }

    /// Replace each of QSMxT's defaults the browser cannot run with the browser's value,
    /// leaving every setting the user changed alone. Idempotent.
    ///
    /// - The deep-learning inversions are tiled ([`BROWSER_TILE_SIZE`] / [`BROWSER_TILE_HALO`])
    ///   when tiling is not set. The tile options only reach the run (and the command) for the
    ///   networks that tile.
    /// - HD-BET steps on the whole volume run as a sliding window with qsm-core's low-memory
    ///   patch (`hd-bet:low-memory`) instead, in every mask section and refinement list.
    /// - R2PRIMEnet at its authors' patch runs at [`BROWSER_R2PRIMENET_PATCH`].
    /// - SynthSeg on the whole volume runs as a sliding window with qsm-core's low-memory patch.
    ///
    /// Front ends apply it again after a change that can bring a native default back, such as
    /// a mask preset with an HD-BET step.
    pub fn apply_browser_profile(&mut self) {
        let native = Self::default();
        let inv = &mut self.inversion;
        if inv.tile_size.is_none() {
            inv.tile_size = Some(BROWSER_TILE_SIZE);
            if inv.tile_halo.is_none() {
                inv.tile_halo = Some(BROWSER_TILE_HALO);
            }
        }
        let m = &mut self.masking;
        m.sections.iter_mut().for_each(browser_section);
        m.refinements.iter_mut().for_each(browser_op);
        if let Some(s) = m.two_pass_sections.as_mut() {
            s.iter_mut().for_each(browser_section);
        }
        if self.segmentation.patch.is_none() {
            self.segmentation.patch = Some(synthseg_low_memory_patch());
        }
        let r2primenet = &mut self.separation.r2primenet;
        if r2primenet.patch == native.separation.r2primenet.patch {
            r2primenet.patch = BROWSER_R2PRIMENET_PATCH;
        }
    }
}

fn browser_section(s: &mut MaskSection) {
    browser_op(&mut s.generator);
    s.refinements.iter_mut().for_each(browser_op);
}

fn browser_op(op: &mut MaskOp) {
    if let MaskOp::HdBet { patch: patch @ None, .. } = op {
        *patch = Some(hd_bet_low_memory_patch());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_command, QsmAlgorithm};
    use crate::masking::hd_bet_mask_sections;

    #[test]
    fn the_profile_changes_only_what_the_browser_cannot_run() {
        let (native, browser) = (PipelineConfig::default(), PipelineConfig::browser());
        let (mut a, b) = (serde_json::to_value(&native).unwrap(), serde_json::to_value(&browser).unwrap());
        a["inversion"]["tile_size"] = BROWSER_TILE_SIZE.into();
        a["inversion"]["tile_halo"] = BROWSER_TILE_HALO.into();
        a["separation"]["r2primenet"]["patch"] = serde_json::json!(BROWSER_R2PRIMENET_PATCH);
        a["segmentation"]["patch"] = synthseg_low_memory_patch().into();
        assert_eq!(a, b);
    }

    #[test]
    fn the_profile_is_idempotent_and_keeps_what_the_user_set() {
        let mut c = PipelineConfig::browser();
        c.apply_browser_profile();
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::to_value(PipelineConfig::browser()).unwrap());

        let mut c = PipelineConfig::default();
        c.inversion.tile_size = Some(40);
        c.separation.r2primenet.patch = [96, 96, 64];
        c.segmentation.patch = Some(96);
        c.masking.sections = hd_bet_mask_sections();
        if let MaskOp::HdBet { patch, .. } = &mut c.masking.sections[0].generator {
            *patch = Some([160, 160, 80]);
        }
        c.apply_browser_profile();
        assert_eq!(c.inversion.tile_size, Some(40));
        assert_eq!(c.inversion.tile_halo, None, "the user's tiling keeps its own (default) halo");
        assert_eq!(c.separation.r2primenet.patch, [96, 96, 64]);
        assert_eq!(c.segmentation.patch, Some(96));
        assert!(matches!(c.masking.sections[0].generator, MaskOp::HdBet { patch: Some([160, 160, 80]), .. }));
    }

    #[test]
    fn a_default_browser_run_is_the_default_command() {
        // iLSQR does not tile and R2' is not computed: nothing the profile set applies.
        assert_eq!(generate_command(&PipelineConfig::browser()), generate_command(&PipelineConfig::default()));
    }

    #[test]
    fn the_command_carries_the_browser_values() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        c.masking.sections = hd_bet_mask_sections();
        c.pipeline.do_r2primemap = true;
        c.separation.r2prime_strategy = crate::R2PrimeStrategy::R2primenet;
        c.pipeline.do_segmentation = true;
        let native = generate_command(&c);
        c.apply_browser_profile();
        let cmd = generate_command(&c);
        assert!(cmd.contains(&format!("--tile-size {BROWSER_TILE_SIZE}")), "{cmd}");
        assert!(cmd.contains(&format!("--tile-halo {BROWSER_TILE_HALO}")), "{cmd}");
        let [x, y, z] = BROWSER_R2PRIMENET_PATCH;
        assert!(cmd.contains(&format!("--r2primenet-patch {x} {y} {z}")), "{cmd}");
        // The low-memory window has a name of its own.
        assert!(cmd.contains("hd-bet:low-memory"), "{cmd}");
        assert!(cmd.contains(&format!("--synthseg-patch {}", synthseg_low_memory_patch())), "{cmd}");
        for flag in ["--tile-size", "--r2primenet-patch", "hd-bet:low-memory", "--synthseg-patch"] {
            assert!(!native.contains(flag), "{native}");
        }
    }
}
