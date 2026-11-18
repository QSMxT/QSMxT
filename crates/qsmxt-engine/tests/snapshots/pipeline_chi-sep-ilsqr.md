## Command

qsmxt run <bids_dir> --pipeline chi-sep-ilsqr

## Config hash

8cd964d12a56f83b

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
magnitude                will_run   d82b40a650fa4990
mask                     will_run   be81155c6b6e0d6f
motion                   skipped    -
t2star_r2star            will_run   71883b65a3d0ba9d
r2_r2prime               will_run   cb5c308f771c4c69
unwrap                   will_run   5729e68f384acc64
bgremove                 will_run   dd1cecf888d2fbe4
invert                   will_run   44bb0e4c1c060d8d
reference                will_run   965ae69dfcceed4f
chi_separation           will_run   b7bcacdda63045b9
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the χ-separation toolbox 1.1.3 pipeline. A brain mask was generated using BET brain extraction (Smith, 2002) of the RSS-combined magnitude image, in its stored voxel order as MEDI's BET.m runs it. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed using ROMEO (Dymerska et al., 2021) with phase linearity disabled. The B0 field map was estimated as a weighted average of the unwrapped echo phases. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015). No susceptibility referencing was applied. R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015). R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended Phase Graph dictionary (Weigel, 2015). R2' was computed as R2* - R2. Negative R2' values were set to zero. Paramagnetic and diamagnetic susceptibility maps were computed using χ-separation (iLSQR) (Shin et al., 2021) within the reconstruction mask (the brain mask less any rim eroded by background field removal). The two sources were initialised from the susceptibility map and solved by LSQR, streaking removal and a non-negative refinement. The susceptibility map separated was reconstructed from the local field by iLSQR (Li et al., 2015).

## Note

Outputs the run turned on because other settings need them:

- The R2′ map turned on: χ-separation (iLSQR) needs it (it separates χ with R2′).
- The R2* map turned on: the R2′ map needs it (R2′ is computed from R2*).
- The R2 map turned on: the R2′ map needs it (R2′ is measured as R2* − R2 where it can be).

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Smith, S.M. (2002). "Fast robust automated brain extraction." *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Pei, M., et al. (2015). "Algorithm for fast monoexponential fitting based on Auto-Regression on Linear Operations (ARLO) of data." *Magnetic Resonance in Medicine*, 73(2):843-850. https://doi.org/10.1002/mrm.25137
- Weigel, M. (2015). "Extended phase graphs: dephasing, RF pulses, and echoes - pure and simple." *Journal of Magnetic Resonance Imaging*, 41(2):266-295. https://doi.org/10.1002/jmri.24619
- Shin, H.-G., et al. (2021). "χ-separation: Magnetic susceptibility source separation toward iron and myelin mapping in the brain." *NeuroImage*, 240:118371. https://doi.org/10.1016/j.neuroimage.2021.118371
