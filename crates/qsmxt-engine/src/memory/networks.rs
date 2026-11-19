//! The deep-learning networks' peaks: HD-BET, RS2-Net, MindGrab, SynthSeg and the χ-separation
//! networks.

use crate::config::SeparationConfig;
use crate::enums::SeparationAlgorithm;

/// HD-BET's peak for the network itself, in bytes, with a sliding window of `patch` (`None`: one
/// pass over the whole volume): tract's activations for one network input (~850 B per input
/// voxel, measured at 128×128×64 and 96×96×64) and the parsed model. A sliding window's input
/// is its patch. The whole volume's is the head's bounding box at 1 mm, padded to multiples of
/// 32×32×16, which does not depend on the acquired resolution (6.4–8.2 GB peak RSS measured on
/// 1 mm whole-head and 0.8×0.8×3 mm GRE, up to 160×256×256 voxels), so budget a 176×256×256
/// box. The scan's own working volumes come on top.
pub fn hd_bet_network_bytes(patch: Option<[usize; 3]>) -> u64 {
    const FULL_VOLUME_VOXELS: u64 = 256 * 256 * 176;
    let input_voxels = patch.map_or(FULL_VOLUME_VOXELS, |p| p.iter().map(|&d| d as u64).product());
    input_voxels * 850 + 130_000_000
}

/// RS2-Net runs at one fixed window, so its peak does not depend on the volume: 3.2 GB measured
/// in the browser on a mouse scan.
pub(super) const RS2_NET_BYTES: u64 = 3_200_000_000;

/// MindGrab runs on the image conformed to 256³ at 1 mm whatever the acquisition, so its peak
/// does not depend on the volume: 7.3 GB measured on 256×288×48 GRE, with or without TTA (the
/// flipped pass runs after the first).
pub(super) const MINDGRAB_BYTES: u64 = 7_500_000_000;

/// SynthSeg's peak in bytes on a `dims` volume (taken to be about 1 mm, the resolution SynthSeg
/// resamples to), with a sliding window of `patch` (`None`: the whole volume at once). The
/// network takes about 1 KB per voxel it sees at once, and the posteriors (one float per label
/// per voxel, blended and then cleaned up) about 200 B per padded voxel. Measured on a 184×256×128
/// scan with flip averaging: 6.8 GB on the whole volume, 3.25 GB with 128³ windows, 2.0 GB with
/// 96³.
pub fn synthseg_bytes(dims: [usize; 3], patch: Option<usize>) -> u64 {
    // qsm-core pads each axis to a multiple of 32, at least 128.
    let padded: [u64; 3] = std::array::from_fn(|a| (dims[a].div_ceil(32) * 32).max(128) as u64);
    let n_pad: u64 = padded.iter().product();
    let window: u64 = match patch {
        None => n_pad,
        Some(p) => {
            let p = (p.div_ceil(32) * 32) as u64;
            padded.iter().map(|&d| d.min(p)).product()
        }
    };
    window * 1000 + n_pad * 200
}

/// The peak of a χ-separation network on top of the maps it reads, in bytes, on a `dims`
/// volume: tract's activations for one patch, the parsed model, and the network's own padded
/// input channels and overlap accumulators. χ-sepnet runs its `patch`; SUSEP-Net runs its
/// `patch`, or with none the whole volume (padded to multiples of 8) in one pass. Other methods:
/// 0. Measured natively, single-threaded, on a 184×256×128 volume (peak RSS, the maps in and out
/// included): χ-sepnet 4.5 GB at its authors' 192×192×128 patch and 1.7 GB at 128×128×64;
/// SUSEP-Net 12.5 GB on the whole volume and 3.1 GB at 128×128×64; on 136×136×72, SUSEP-Net
/// 2.8 GB at 128×128×64 and 2.0 GB at 96×96×64.
///
/// χ-sepnet's toolbox route runs its networks (QSMnetME, R2PRIMEnet, χ-sepnet: the same graph
/// size, one at a time) on the whole training frame `frame` (or its `frame_patch`), plus the
/// spline resampling's working arrays over the acquired grid and the frame.
pub fn chi_separation_network_bytes(dims: [usize; 3], frame: [usize; 3], config: &SeparationConfig) -> u64 {
    if config.algorithm == SeparationAlgorithm::ChiSepNet && config.chi_sepnet.route == crate::ChiSepNetRoute::Toolbox {
        let frame_n: u64 = frame.iter().map(|&d| d as u64).product();
        let window: u64 = match config.chi_sepnet.frame_patch {
            Some(p) => p.iter().map(|&d| d as u64).product(),
            None => frame_n,
        };
        let native_n: u64 = dims.iter().map(|&d| d as u64).product();
        return window * CHI_SEPNET_PER_VOXEL + CHI_SEPNET_FIXED + 64 * frame_n + 32 * native_n;
    }
    // (activation bytes per patch voxel, parsed model and fixed overhead, pooling multiple)
    let (patch, per_voxel, fixed, multiple) = match config.algorithm {
        SeparationAlgorithm::ChiSepNet => (Some(config.chi_sepnet.patch), CHI_SEPNET_PER_VOXEL, CHI_SEPNET_FIXED,
            qsm_core::separation::chisepnet::POOL_MULTIPLE),
        SeparationAlgorithm::SusepNet => (config.susep_net.patch, SUSEP_NET_PER_VOXEL, SUSEP_NET_FIXED,
            qsm_core::separation::susep_net::POOL_MULTIPLE),
        _ => return 0,
    };
    let round = |d: usize| d.div_ceil(multiple) * multiple;
    // The padded volume: each axis at least the patch (and, whole-volume, a multiple of 8).
    let padded: [u64; 3] = std::array::from_fn(|a| match patch {
        Some(p) => dims[a].max(p[a]) as u64,
        None => round(dims[a]) as u64,
    });
    let window: u64 = match patch {
        Some(p) => p.iter().map(|&d| d as u64).product(),
        None => padded.iter().product(),
    };
    // Three f32 input channels and three f64 accumulators over the padded volume.
    window * per_voxel + fixed + 36 * padded.iter().product::<u64>()
}

const CHI_SEPNET_PER_VOXEL: u64 = 760;
const CHI_SEPNET_FIXED: u64 = 350_000_000;
const SUSEP_NET_PER_VOXEL: u64 = 1_850;
const SUSEP_NET_FIXED: u64 = 800_000_000;
