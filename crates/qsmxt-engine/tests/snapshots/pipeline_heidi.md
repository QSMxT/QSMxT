## Command

qsmxt run <bids_dir> --pipeline heidi

## Config hash

f33a3401a2e8101b

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
inhomog                  will_run   b537e5cae9ec9938
magnitude                will_run   41493901271c0f06
mask                     will_run   489f108399b10fb1
motion                   skipped    -
unwrap                   will_run   65e65dbff212cbcd
bgremove                 will_run   34e054fce95c6864
invert                   will_run   6e9d5e238b2dcdcc
reference                will_run   965ae69dfcceed4f
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the HEIDI package (qsm_heidi 4592631) pipeline. Inhomogeneity correction (Eckstein et al., 2019) was applied to each echo's magnitude. A brain mask was generated using Otsu thresholding (Otsu, 1979) of the ROMEO phase quality map (computed from phase data and the inhomogeneity-corrected RSS-combined magnitude), followed by dilation, hole-filling, and erosion. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed using ROMEO (Dymerska et al., 2021). The B0 field map was estimated as a weighted average of the unwrapped echo phases. Background field removal was performed using SHARP (Schweser et al., 2011). Dipole inversion was performed using minimally regularised LSQR (Schweser et al., 2010). Streaking was reduced with HEIDI (Homogeneity Enabled Incremental Dipole Inversion) (Schweser et al., 2012) on the LSQR map, as published: its well-conditioned k-space coefficients were kept, and the dipole cone was filled by total variation weighted by the field's homogeneity, with denoise reference itk enabled. No susceptibility referencing was applied.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Eckstein, K., Trattnig, S., Robinson, S.D. (2019). "A Simple Homogeneity Correction for Neuroimaging at 7T." *Proc. ISMRM 27th Annual Meeting*.
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Otsu, N. (1979). "A Threshold Selection Method from Gray-Level Histograms." *IEEE Transactions on Systems, Man, and Cybernetics*, 9(1):62-66. https://doi.org/10.1109/TSMC.1979.4310076
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Schweser, F., et al. (2011). "Quantitative imaging of intrinsic magnetic tissue properties using MRI signal phase: An approach to in vivo brain iron metabolism?" *NeuroImage*, 54(4):2789-2807. https://doi.org/10.1016/j.neuroimage.2010.10.070
- Schweser, F., Deistung, A., Lehr, B.W., Reichenbach, J.R. (2010). "Differentiation between diamagnetic and paramagnetic cerebral lesions based on magnetic susceptibility mapping." *Medical Physics*, 37(10):5165-5178. https://doi.org/10.1118/1.3481505
- Schweser, F., Sommer, K., Deistung, A., Reichenbach, J.R. (2012). "Quantitative susceptibility mapping for investigating subtle susceptibility variations in the human brain." *NeuroImage*, 62(3):2083-2100. https://doi.org/10.1016/j.neuroimage.2012.05.067
