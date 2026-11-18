## Command

qsmxt run <bids_dir> --pipeline uk-biobank

## Config hash

3bc32691826c35a4

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
inhomog                  will_run   b537e5cae9ec9938
magnitude                will_run   41493901271c0f06
mask                     will_run   489f108399b10fb1
motion                   skipped    -
unwrap                   will_run   682f3a4e061d1147
bgremove                 will_run   e9bee72a29bd2135
invert                   will_run   ab3524c6b8584451
reference                will_run   965ae69dfcceed4f
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the UK Biobank QSM pipeline. The brain mask was made from the gradient-echo images as described below, in place of the UK Biobank pipeline's mask from its T1-weighted images. Inhomogeneity correction (Eckstein et al., 2019) was applied to each echo's magnitude. A brain mask was generated using Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude), followed by dilation, hole-filling, and erosion. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase unwrapping was performed separately for each echo using the Laplacian method (Schofield & Zhu, 2003) with an FFT Poisson solve. The B0 field map was estimated as a weighted average of the unwrapped echo phases. Before and after background field removal, the mask was trimmed of voxels with unreliable phase as in the UK Biobank pipeline (Wang et al., 2022): phase-reliability maps of the first and last echoes (the magnitude of the unit phasor averaged over a 2 mm ball) were thresholded at 0.6 and 0.5 before background removal and at 0.7 and 0.6 after it, then in each slice components smaller than 300 (after: 200) voxels were removed and holes smaller than 50 (after: 30) voxels filled, and enclosed holes were filled. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015) with zero-padding = 64 mm per side and precision = single. No susceptibility referencing was applied.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Eckstein, K., Trattnig, S., Robinson, S.D. (2019). "A Simple Homogeneity Correction for Neuroimaging at 7T." *Proc. ISMRM 27th Annual Meeting*.
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Otsu, N. (1979). "A Threshold Selection Method from Gray-Level Histograms." *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076
- Schofield, M.A., Zhu, Y. (2003). "Fast phase unwrapping algorithm for interferometric applications." *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Wang, C., et al. (2022). "Phenotypic and genetic associations of quantitative magnetic susceptibility in UK Biobank brain imaging." *Nature Neuroscience*, 25(6):818-831. https://doi.org/10.1038/s41593-022-01074-w
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
