## Command

qsmxt run <bids_dir> --do-swi --do-analysis --do-smwi --do-t2starmap --do-r2primemap --do-chisep --export-dicom --motion-correction --two-pass

## Config hash

39c974848729099e

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   74006b51a3fd9d40
scale_phase              will_run   565f39e380eb4906
resample_axial           skipped    -
inhomog                  will_run   b537e5cae9ec9938
magnitude                will_run   41493901271c0f06
mask                     will_run   489f108399b10fb1
motion                   will_run   16443289e4761e88
swi                      will_run   eb6a1243524ea20e
segmentation             will_run   f2c3e0b888b62e31
t2star_r2star            will_run   71883b65a3d0ba9d
r2_r2prime               will_run   3083acab23a6f2b6
unwrap                   will_run   65e65dbff212cbcd
bgremove                 will_run   dd1cecf888d2fbe4
invert                   will_run   933dd90aeff50919
mask-reliable            will_run   c2e64e66f1652962
bgremove-reliable        will_run   984ad4227d215cef
invert-reliable          will_run   933dd90aeff50919
twopass                  will_run   a5b9256626e84531
reference-singlepass     will_run   ab537061af419c5e
reference                will_run   ab537061af419c5e
smwi                     will_run   127231076028c778
chi_separation           will_run   43ec991241300a84
analysis                 will_run   edfd61c950702653
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Inhomogeneity correction (Eckstein et al., 2019) was applied to each echo's magnitude. A brain mask was generated using Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude), followed by dilation, hole-filling, and erosion. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Before field mapping, in runs with more than one echo and magnitude images, every echo was rigidly co-registered to the first echo by maximising the normalised cross-correlation of the magnitude images over a multi-resolution pyramid, and the phase was resampled with it through the complex signal; the brain mask was restricted to voxels covered by every echo. Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed using ROMEO (Dymerska et al., 2021). The B0 field map was estimated as a weighted average of the unwrapped echo phases. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015). Two-pass artefact reduction (Stewart et al., 2022) was applied with holes only at strong susceptibility sources, taken from a second mask generated using Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude): small holes were filled, thin vessel-like holes were separated off by a morphological opening, and only holes with a core of very low phase quality inside the first reconstruction's mask were kept, each with its own rim. Background field removal and dipole inversion were repeated against this mask. Voxels next to the holes that background field removal left without a value were filled by LBV background field removal (Zhou et al., 2014) on the same mask. The second susceptibility map was used near the holes and blended linearly into the first with distance from them; the first map was used in the holes and everywhere else. Susceptibility values were referenced to the mean susceptibility within the reconstruction mask (the brain mask less any rim eroded by background field removal). The brain was parcellated from the GRE magnitude using SynthSeg v1 (Billot et al., 2023), yielding 31 FreeSurfer-labelled structures. Each quantitative map was then summarised within every segmented structure (mean, standard deviation, median, minimum, maximum and 5th/95th percentiles over the structure's voxels), excluding voxels the map does not cover. Susceptibility map-weighted images were computed (Gho et al., 2014) by weighting the magnitude with a mask derived from the susceptibility map, separately for paramagnetic and diamagnetic sources, each followed by a minimum-intensity projection. Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021). Minimum-intensity projections of these images were also computed. T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015). R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended Phase Graph dictionary (Weigel, 2015). R2' was computed as R2* - R2 where a multi-echo spin-echo acquisition was available, and otherwise, at 3 T only, estimated from R2* with R2PRIMEnet (Kim et al., 2025). Negative R2' values were set to zero. Paramagnetic and diamagnetic susceptibility maps were computed using R2*-QSM susceptibility source separation (Dimov et al., 2022) within the reconstruction mask. Final maps were additionally exported as DICOM series.

## Note

Differences from the methods' reference pipelines:

- Mask used Otsu's threshold; this differs from the iLSQR reference pipeline, which uses thresh_tool's starting level.
- Mask used phase quality; this differs from the iLSQR reference pipeline, which uses last echo.
- Inhomogeneity correction used makehomogeneous; this differs from the iLSQR reference pipeline, which uses none.
- Mask used every component; this differs from the iLSQR reference pipeline, which uses largest component only.
- Mask used holes not filled; this differs from the iLSQR reference pipeline, which uses every hole filled.
- Mask used 1 voxel; this differs from the iLSQR reference pipeline, which uses no erosion.
- Phase unwrapping used ROMEO; this differs from the iLSQR reference pipeline, which uses Laplacian.
- B0 estimation used Weighted average; this differs from the R2*-QSM reference pipeline, which uses Complex fit.
- Background removal used V-SHARP; this differs from the R2*-QSM reference pipeline, which uses PDF.
- Polish mask was off; this differs from the iLSQR reference pipeline, which has it on.
- Maximum iterations (iLSQR) was 100; this differs from the iLSQR reference pipeline, which uses 40.
- Precision (iLSQR) used Double; this differs from the iLSQR reference pipeline, which uses Single.
- QSM inversion used iLSQR; this differs from the R2*-QSM reference pipeline, which uses MEDI.
- QSM reference used Mean; this differs from the iLSQR reference pipeline, which uses None.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Eckstein, K., Trattnig, S., Robinson, S.D. (2019). "A Simple Homogeneity Correction for Neuroimaging at 7T." *Proc. ISMRM 27th Annual Meeting*.
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Otsu, N. (1979). "A Threshold Selection Method from Gray-Level Histograms." *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Stewart, A.W., Robinson, S.D., O'Brien, K., et al. (2022). "QSMxT: Robust masking and artifact reduction for quantitative susceptibility mapping." *Magnetic Resonance in Medicine*, 87(3):1289-1300. https://doi.org/10.1002/mrm.29048
- Zhou, D., et al. (2014). "Background field removal by solving the Laplacian boundary value problem." *NMR in Biomedicine*, 27(3):312-319. https://doi.org/10.1002/nbm.3064
- Billot, B., Greve, D.N., Puonti, O., et al. (2023). "SynthSeg: Segmentation of brain MRI scans of any contrast and resolution without retraining." *Medical Image Analysis*, 86:102789. https://doi.org/10.1016/j.media.2023.102789
- Gho, S.-M., Liu, C., Li, W., et al. (2014). "Susceptibility map-weighted imaging (SMWI) for neuroimaging." *Magnetic Resonance in Medicine*, 72(2):337-346. https://doi.org/10.1002/mrm.24920
- Eckstein, K., Bachrata, B., Hangel, G., Widhalm, G., Enzinger, C., Barth, M., Trattnig, S., Robinson, S.D. (2021). "Improved susceptibility weighted imaging at ultra-high field using bipolar multi-echo acquisition and optimized image processing: CLEAR-SWI." *NeuroImage*, 237:118175. https://doi.org/10.1016/j.neuroimage.2021.118175
- Pei, M., et al. (2015). "Algorithm for fast monoexponential fitting based on Auto-Regression on Linear Operations (ARLO) of data." *Magnetic Resonance in Medicine*, 73(2):843-850. https://doi.org/10.1002/mrm.25137
- Weigel, M. (2015). "Extended phase graphs: dephasing, RF pulses, and echoes - pure and simple." *Journal of Magnetic Resonance Imaging*, 41(2):266-295. https://doi.org/10.1002/jmri.24619
- Kim, M., Ji, S., Lee, J., et al. (2025). "chi-sepnet: Deep neural network for magnetic susceptibility source separation." *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136
- Dimov, A. V., et al. (2022). "Susceptibility source separation from gradient echo data using magnitude decay modeling." *Journal of Neuroimaging*, 32(4):852-859. https://doi.org/10.1111/jon.13014
