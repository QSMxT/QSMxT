## Command

qsmxt run <bids_dir> --pipeline qsmart

## Config hash

492b5bcabcc1f708

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
inhomog                  will_run   df8800df7a34de01
magnitude                will_run   9c8af2ebff243f82
mask                     will_run   ce4a76ab15a11e7b
motion                   skipped    -
unwrap                   will_run   d357943336f2deb4
qsmart                   will_run   5a72eb92d982dd8f
reference                will_run   965ae69dfcceed4f
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the QSMART pipeline. N4 inhomogeneity correction (Tustison et al., 2010) was applied to each echo's magnitude. A brain mask was generated using BET brain extraction (Smith, 2002; f=0.20) of the inhomogeneity-corrected first-echo magnitude image, followed by removal of voxels with an intensity below 100, a 3×3×3 majority filter, and morphological closing (radius 2 voxels). Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase unwrapping was performed separately for each echo, on the raw phase over the whole volume, using the Laplacian method (Schofield & Zhu, 2003) with an FFT Poisson solve on the volume mirrored along each axis. The B0 field map was estimated by a magnitude-weighted fit of phase vs echo time through the origin, as in QSMART (Yaghmaie et al., 2021), and voxels whose blurred fit residual exceeded 40 were excluded from the QSMART reconstruction. QSM was computed using QSMART (Yaghmaie et al., 2021), with spatially dependent filtering for background field removal, iLSQR dipole inversion (Li et al., 2015), and Frangi vesselness-based tissue/vasculature separation, with the stages' offset adjusted on the dipole grid of QSMART's adjust_offset.m. Vessels were detected on the mean magnitude over echoes, bias-corrected with N4 (Tustison et al., 2010). No susceptibility referencing was applied.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Tustison, N.J., Avants, B.B., Cook, P.A., Zheng, Y., Egan, A., Yushkevich, P.A., Gee, J.C. (2010). "N4ITK: Improved N3 Bias Correction." *IEEE Transactions on Medical Imaging*, 29(6):1310-1320. https://doi.org/10.1109/TMI.2010.2046908
- Smith, S.M. (2002). "Fast robust automated brain extraction." *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062
- Schofield, M.A., Zhu, Y. (2003). "Fast phase unwrapping algorithm for interferometric applications." *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194
- Yaghmaie, N., Syeda, W., et al. (2021). "QSMART: Quantitative Susceptibility Mapping Artifact Reduction Technique." *NeuroImage*, 231:117701. https://doi.org/10.1016/j.neuroimage.2020.117701
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
