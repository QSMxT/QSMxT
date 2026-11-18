## Command

qsmxt run <bids_dir> --pipeline chi-sepnet

## Config hash

452d8b44027ecaae

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
magnitude                will_run   d82b40a650fa4990
mask                     will_run   be81155c6b6e0d6f
motion                   skipped    -
t2star_r2star            will_run   71883b65a3d0ba9d
unwrap                   will_run   5729e68f384acc64
bgremove                 will_run   dd1cecf888d2fbe4
invert                   will_run   44bb0e4c1c060d8d
reference                will_run   ab537061af419c5e
chi_separation           will_run   e6402adff2dc6f39
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the χ-separation toolbox 1.1.3 pipeline. A brain mask was generated using BET brain extraction (Smith, 2002) of the RSS-combined magnitude image, in its stored voxel order as MEDI's BET.m runs it. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). Phase offset removal was performed using MCPC-3D-S (Eckstein et al., 2018). Phase unwrapping was performed using ROMEO (Dymerska et al., 2021) with phase linearity disabled. The B0 field map was estimated as a weighted average of the unwrapped echo phases. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using iLSQR (Li et al., 2015). Susceptibility values were referenced to the mean susceptibility within the reconstruction mask (the brain mask less any rim eroded by background field removal). R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015). Paramagnetic and diamagnetic susceptibility maps were computed using χ-sepnet (deep-learning source separation) (Kim et al., 2025) within the reconstruction mask, with the inputs resampled to 1 mm with B0 along z and the susceptibility channel computed by QSMnetME, as in the χ-separation toolbox, and the maps resampled back. Negative χ+ and |χ−| values were set to zero. χ-separation used R2' measured as R2* - R2 where a multi-echo spin-echo acquisition was available, and otherwise its own R2', estimated from R2* with R2PRIMEnet (Kim et al., 2025) in the network's training frame.

## Note

Outputs the run turned on because other settings need them:

- The R2* map turned on: χ-sepnet (deep-learning source separation) needs it (it reads R2*).

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Smith, S.M. (2002). "Fast robust automated brain extraction." *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062
- Eckstein, K., et al. (2018). "Computationally Efficient Combination of Multi-channel Phase Data From Multi-echo Acquisitions (ASPIRE)." *Magnetic Resonance in Medicine*, 79:2996-3006. https://doi.org/10.1002/mrm.26963
- Dymerska, B., et al. (2021). "Phase unwrapping with a rapid opensource minimum spanning tree algorithm (ROMEO)." *Magnetic Resonance in Medicine*, 85(4):2294-2308. https://doi.org/10.1002/mrm.28563
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Li, W., et al. (2015). "A method for estimating and removing streaking artifacts in quantitative susceptibility mapping." *NeuroImage*, 108:111-122. https://doi.org/10.1016/j.neuroimage.2014.12.043
- Pei, M., et al. (2015). "Algorithm for fast monoexponential fitting based on Auto-Regression on Linear Operations (ARLO) of data." *Magnetic Resonance in Medicine*, 73(2):843-850. https://doi.org/10.1002/mrm.25137
- Kim, M., et al. (2025). "χ-sepnet: Deep neural network for magnetic susceptibility source separation." *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136
- Kim, M., Ji, S., Lee, J., et al. (2025). "chi-sepnet: Deep neural network for magnetic susceptibility source separation." *Human Brain Mapping*, 46(4):e70136. https://doi.org/10.1002/hbm.70136
