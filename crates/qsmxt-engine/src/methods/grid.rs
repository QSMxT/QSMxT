//! Methods prose for the reconstruction grid.

use super::*;

/// The reconstruction-grid switches, all off by default, so each is said only when on.
pub(super) fn describe_reconstruction_grid(config: &PipelineConfig, ran: Ran, doc: &mut Draft) {
    let p = &config.pipeline;
    if p.ignore_slice_gap {
        doc.say(&[Stage::Load], "Acquisitions whose slices were separated by a gap were reconstructed as if the slices were contiguous.".to_string());
    }
    let space = match p.output_space {
        OutputSpace::Acquired => "",
        OutputSpace::Working => ", and the derivatives were written on the resampled grid",
    };
    // Resampled only because an algorithm needs axial data: the switches do not say so.
    if !p.axial_resampling && ran.count(Topic::Grid, Choice::ResampledForAlgorithm) > 0 {
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "Acquisitions whose B0 direction was more than {}° from the slice normal were resampled onto an \
             axial grid, with B0 along the slice normal, before reconstruction, as the algorithms used assume \
             it{space}.", crate::scan::AXIAL_ONLY_TOLERANCE_DEG));
    }
    // None of the runs was resampled: there is nothing to say.
    if p.axial_resampling && !ran.only(Topic::Grid, &[Choice::Acquired]) {
        let which = if differs(p.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD) {
            format!("Acquisitions more than {}° oblique", p.obliquity_threshold)
        } else {
            "Oblique acquisitions".to_string()
        };
        doc.say(&[Stage::Load, Stage::ResampleAxial, Stage::OutputSpace], format!(
            "{which} were resampled onto an axial grid, with B0 along the slice normal, before reconstruction{space}."));
    }
    if p.crop_to_mask {
        let margin = if differs(p.crop_margin_mm, PipelineToggles::default().crop_margin_mm) {
            format!(" with a {} mm margin", p.crop_margin_mm)
        } else {
            String::new()
        };
        doc.say(&reconstruction(), format!("Reconstruction was performed within a box around the brain mask{margin}."));
    }
    if p.fft_padding {
        doc.say(&reconstruction(), "Before the FFT-based stages, the grid was zero-padded to an FFT-efficient size.".to_string());
    }
}

/// "64" for a uniform pad, "64×64×32" otherwise.
pub(super) fn describe_pad(p: [usize; 3]) -> String {
    if p[0] == p[1] && p[1] == p[2] { p[0].to_string() } else { format!("{}×{}×{}", p[0], p[1], p[2]) }
}
