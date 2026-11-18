## Command

qsmxt run <bids_dir> --pipeline sti-suite-ilsqr

## Config hash

7950360caf1a4c62

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
magnitude                will_run   d82b40a650fa4990
mask                     will_run   3b99652d9088e6da
motion                   skipped    -
unwrap                   will_run   b61ce283bf05d071
bgremove                 will_run   6fd8ee4747dee677
invert                   will_run   1572db122c3e6ce5
reference                will_run   965ae69dfcceed4f
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the STI Suite 3.0 pipeline. A brain mask was generated using thresholding of the last-echo magnitude image at STI Suite's default level (Otsu's threshold of the middle slice; Otsu, 1979), followed by the largest connected component kept and every hole filled. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed separately for each echo, on the raw phase over the whole volume, using the Laplacian method (Schofield & Zhu, 2003) with an FFT Poisson solve on the volume zero-padded by 12 voxels. The B0 field map was estimated as a weighted average of the unwrapped echo phases. After background field removal, the mask was reduced to its largest connected component with its holes filled, and the local field was set to zero outside it. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015) with at most 40 iterations and precision = single. No susceptibility referencing was applied.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Otsu, N. (1979). "A Threshold Selection Method from Gray-Level Histograms." *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Schofield, M.A., Zhu, Y. (2003). "Fast phase unwrapping algorithm for interferometric applications." *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
