//! The browser profile: QSMxT's defaults, changed only where the browser cannot run them.
//!
//! A browser (WASM) run has a 4 GB heap, and several deep-learning steps need more than that at
//! QSMxT's defaults: whole-volume inversion networks, BFRnet, iQFM, iQSM and iQSM+, HD-BET on the whole volume (9-13 GB),
//! SynthSeg on the whole volume (about 7 GB for a 1 mm whole-head scan), R2PRIMEnet and χ-sepnet
//! at their authors' patch and SUSEP-Net on the whole volume. The profile replaces those defaults with values that fit,
//! and writes them into the config, so the TOML, the generated `qsmxt run` command and the
//! methods text all carry them and QSMxT reproduces a browser run exactly.
//!
//! Everything else stays QSMxT's default, with two exceptions that are not about memory, settings a
//! page cannot carry out: a page holds one scan, so it has no orientations to group (#496), and it
//! writes no DICOM (#504). It clears these when a file brings them in and says so, because it could
//! not have done them and must not export them or describe them as done.

use crate::config::{synthseg_low_memory_patch, MultiOrientationConfig, PipelineConfig};
use crate::masking::{hd_bet_low_memory_patch, MaskOp, MaskSection};

/// Overlap-tiling core of the deep-learning networks in the browser (the inversions, iQSM/iQSM+,
/// BFRnet and iQFM), in voxels. With
/// [`BROWSER_TILE_HALO`] it makes 64³ patches, inside qsm-core's largest WASM patch
/// (`qsm_core::inversion::max_wasm_patch_edge`, 131).
pub const BROWSER_TILE_SIZE: usize = 56;

/// Overlap-tiling context margin per side in the browser, in voxels.
pub const BROWSER_TILE_HALO: usize = 4;

/// R2PRIMEnet's sliding-window patch in the browser: qsm-core's `WASM_PATCH` for it.
pub const BROWSER_R2PRIMENET_PATCH: [usize; 3] = {
    let (d, h, w) = qsm_core::relaxometry::WASM_PATCH;
    [d, h, w]
};

/// χ-sepnet's sliding-window patch in the browser: qsm-core's `WASM_PATCH` for it.
pub const BROWSER_CHI_SEPNET_PATCH: [usize; 3] = {
    let (d, h, w) = qsm_core::separation::chisepnet::WASM_PATCH;
    [d, h, w]
};

/// SUSEP-Net's sliding-window patch in the browser (natively it runs the whole volume):
/// qsm-core's `WASM_PATCH` for it.
pub const BROWSER_SUSEP_NET_PATCH: [usize; 3] = {
    let (d, h, w) = qsm_core::separation::susep_net::WASM_PATCH;
    [d, h, w]
};

impl PipelineConfig {
    /// QSMxT's defaults with the browser profile applied: what a browser run starts from.
    pub fn browser() -> Self {
        let mut c = Self::default();
        let _ = c.apply_browser_profile();
        c
    }

    /// Replace each of QSMxT's defaults the browser cannot run with the browser's value, and
    /// clear the multi-orientation settings and DICOM export. Returns a warning for each setting the user chose
    /// that this took away, for the front end to show: a page imports a file through this.
    /// Every other setting the user changed is left alone. Idempotent.
    ///
    /// - The deep-learning networks are tiled ([`BROWSER_TILE_SIZE`] / [`BROWSER_TILE_HALO`])
    ///   when tiling is not set: the inversions, iQSM/iQSM+, BFRnet and iQFM. The tile options only reach the run (and the command) for the
    ///   networks that tile.
    /// - HD-BET steps on the whole volume run as a sliding window with qsm-core's low-memory
    ///   patch (`hd-bet:low-memory`) instead, in every mask section and refinement list.
    /// - R2PRIMEnet at its authors' patch runs at [`BROWSER_R2PRIMENET_PATCH`].
    /// - SynthSeg on the whole volume runs as a sliding window with qsm-core's low-memory patch.
    /// - χ-sepnet at its authors' patch runs at [`BROWSER_CHI_SEPNET_PATCH`], and SUSEP-Net on the
    ///   whole volume as a sliding window of [`BROWSER_SUSEP_NET_PATCH`].
    /// - `multi_orientation.*` goes back to its defaults: the page runs one scan on its own
    ///   whatever they say, so only the exported command and settings file would carry them.
    /// - `pipeline.export_dicom` is turned off: a page writes no DICOM, so only the exported
    ///   command, the settings file and the methods text would say it did.
    ///
    /// Front ends apply it again after a change that can bring a native default back, such as
    /// a mask preset with an HD-BET step.
    #[must_use = "a warning says which of the user's settings were cleared: show it"]
    pub fn apply_browser_profile(&mut self) -> Vec<String> {
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
        let chi_sepnet = &mut self.separation.chi_sepnet;
        if chi_sepnet.patch == native.separation.chi_sepnet.patch {
            chi_sepnet.patch = BROWSER_CHI_SEPNET_PATCH;
        }
        chi_sepnet.frame_patch.get_or_insert(BROWSER_CHI_SEPNET_PATCH);
        self.separation.susep_net.patch.get_or_insert(BROWSER_SUSEP_NET_PATCH);
        let mut warnings: Vec<String> = clear_multi_orientation(self).into_iter().collect();
        if self.pipeline.export_dicom {
            self.pipeline.export_dicom = false;
            warnings.push(
                "DICOM export turned off (pipeline.export_dicom = true): a browser page writes no \
                 DICOM series. The exported command and settings file leave it out, and the methods \
                 do not mention it."
                    .to_string(),
            );
        }
        warnings
    }
}

/// Put `multi_orientation.*` back to its defaults, and the warning naming what was set, if anything
/// was.
fn clear_multi_orientation(c: &mut PipelineConfig) -> Option<String> {
    let native = MultiOrientationConfig::default();
    if c.multi_orientation == native {
        return None;
    }
    let (set, default) = (serde_json::to_value(&c.multi_orientation).ok()?, serde_json::to_value(&native).ok()?);
    let changed: Vec<String> = set.as_object()?.iter()
        .filter(|(k, v)| default.get(k.as_str()) != Some(*v))
        .map(|(k, v)| format!("multi_orientation.{k} = {v}"))
        .collect();
    c.multi_orientation = native;
    Some(format!(
        "Multi-orientation settings cleared ({}): a browser page holds one scan, so there are no \
         orientations to combine. The scan is processed on its own, and the exported command and \
         settings file leave them out.",
        changed.join(", ")))
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
        a["separation"]["chi_sepnet"]["patch"] = serde_json::json!(BROWSER_CHI_SEPNET_PATCH);
        a["separation"]["chi_sepnet"]["frame_patch"] = serde_json::json!(BROWSER_CHI_SEPNET_PATCH);
        a["separation"]["susep_net"]["patch"] = serde_json::json!(BROWSER_SUSEP_NET_PATCH);
        assert_eq!(a, b);
    }

    #[test]
    fn the_profile_is_idempotent_and_keeps_what_the_user_set() {
        let mut c = PipelineConfig::browser();
        let _ = c.apply_browser_profile();
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::to_value(PipelineConfig::browser()).unwrap());

        let mut c = PipelineConfig::default();
        c.inversion.tile_size = Some(40);
        c.separation.r2primenet.patch = [96, 96, 64];
        c.segmentation.patch = Some(96);
        c.separation.chi_sepnet.patch = [96, 96, 96];
        c.separation.susep_net.patch = Some([64, 64, 64]);
        c.masking.sections = hd_bet_mask_sections();
        if let MaskOp::HdBet { patch, .. } = &mut c.masking.sections[0].generator {
            *patch = Some([160, 160, 80]);
        }
        let _ = c.apply_browser_profile();
        assert_eq!(c.inversion.tile_size, Some(40));
        assert_eq!(c.inversion.tile_halo, None, "the user's tiling keeps its own (default) halo");
        assert_eq!(c.separation.r2primenet.patch, [96, 96, 64]);
        assert_eq!(c.segmentation.patch, Some(96));
        assert_eq!(c.separation.chi_sepnet.patch, [96, 96, 96]);
        assert_eq!(c.separation.susep_net.patch, Some([64, 64, 64]));
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
        let _ = c.apply_browser_profile();
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

    /// Each χ-separation network's browser patch reaches the command when that network is the
    /// method, and only then.
    #[test]
    fn the_command_carries_the_chi_separation_patches() {
        use crate::SeparationAlgorithm::{ChiSepNet, SusepNet};
        for (alg, flag, own, other, [x, y, z]) in [
            // χ-sepnet's default route (the toolbox's) tiles its training frame.
            (ChiSepNet, "--chi-sepnet-frame-patch", "chi-sepnet", "--susep-net-patch", BROWSER_CHI_SEPNET_PATCH),
            (SusepNet, "--susep-net-patch", "susep-net", "--chi-sepnet-patch", BROWSER_SUSEP_NET_PATCH),
        ] {
            let mut c = PipelineConfig::default();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = alg;
            let native = generate_command(&c);
            assert!(native.contains(&format!("--chisep {own}")) && !native.contains(flag), "{native}");
            let _ = c.apply_browser_profile();
            let cmd = generate_command(&c);
            assert!(cmd.contains(&format!("{flag} {x} {y} {z}")), "{cmd}");
            assert!(!cmd.contains(other), "{cmd}");
        }
    }

    /// A page runs one scan, so it can't export a group it never made, and what it took away is
    /// said (#496).
    #[test]
    fn multi_orientation_from_a_file_is_cleared_with_a_warning() {
        let (mut c, _) = PipelineConfig::from_toml_migrating(
            "[multi_orientation]\ngroup_by = \"acq\"\nreference = \"*acq-Neutral*\"\nforce = true\n").unwrap();
        let warnings = c.apply_browser_profile();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        for said in ["multi_orientation.group_by = \"acq\"", "multi_orientation.reference = \"*acq-Neutral*\"", "multi_orientation.force = true"] {
            assert!(warnings[0].contains(said), "{}", warnings[0]);
        }
        assert_eq!(c.multi_orientation, MultiOrientationConfig::default());
        let command = generate_command(&c);
        assert!(!command.contains("orientation"), "{command}");
        assert!(c.apply_browser_profile().is_empty(), "already cleared");
    }

    /// A page writes no DICOM, so it neither exports the setting nor describes it as done, and
    /// says it turned it off (#504).
    #[test]
    fn dicom_export_from_a_file_is_turned_off_with_a_warning() {
        let (mut c, _) = PipelineConfig::from_toml_migrating("[pipeline]\nexport_dicom = true\n").unwrap();
        assert!(generate_command(&c).contains("--export-dicom"));
        let warnings = c.apply_browser_profile();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("pipeline.export_dicom = true"), "{}", warnings[0]);
        assert!(!c.pipeline.export_dicom);
        assert!(!generate_command(&c).contains("dicom"), "{}", generate_command(&c));
        assert!(!crate::methods::generate_methods_for(&c, "QSMbly").contains("DICOM"));
        assert!(c.apply_browser_profile().is_empty(), "already off");
    }

    #[test]
    fn a_config_without_multi_orientation_gets_no_warning() {
        assert!(PipelineConfig::default().apply_browser_profile().is_empty());
        assert!(PipelineConfig::browser().apply_browser_profile().is_empty());
    }
}
