## Command

qsmxt run <bids_dir>

## Config hash

bf3430d046024d47

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
inhomog                  will_run   b537e5cae9ec9938
magnitude                will_run   41493901271c0f06
mask                     will_run   489f108399b10fb1
motion                   skipped    -
unwrap                   will_run   65e65dbff212cbcd
bgremove                 will_run   dd1cecf888d2fbe4
invert                   will_run   44bb0e4c1c060d8d
reference                will_run   ab537061af419c5e
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Inhomogeneity correction (Eckstein et al., 2019) was applied to each echo's magnitude. A brain mask was generated using Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude), followed by dilation, hole-filling, and erosion. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed using ROMEO (Dymerska et al., 2021). The B0 field map was estimated as a weighted average of the unwrapped echo phases. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015). Susceptibility values were referenced to the mean susceptibility within the reconstruction mask (the brain mask less any rim eroded by background field removal).

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Eckstein, K., Trattnig, S., Robinson, S.D. (2019). "A Simple Homogeneity Correction for Neuroimaging at 7T." *Proc. ISMRM 27th Annual Meeting*.
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Otsu, N. (1979). "A Threshold Selection Method from Gray-Level Histograms." *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
