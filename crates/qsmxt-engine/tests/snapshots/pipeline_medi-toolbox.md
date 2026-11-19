## Command

qsmxt run <bids_dir> --pipeline medi-toolbox

## Config hash

c60ab831e5e482f

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
magnitude                will_run   d82b40a650fa4990
mask                     will_run   be81155c6b6e0d6f
motion                   skipped    -
unwrap                   will_run   3df22523c40d3af2
bgremove                 will_run   f9355b4b840602c3
invert                   will_run   d7b0b854e7bf9b6f
reference                will_run   965ae69dfcceed4f
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the MEDI toolbox 2024.11.26 pipeline. A brain mask was generated using BET brain extraction (Smith, 2002) of the RSS-combined magnitude image, in its stored voxel order as MEDI's BET.m runs it. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). The B0 field map was estimated by a magnitude-weighted nonlinear fit of the complex echo signal (Fit_ppm_complex, MEDI toolbox; Liu et al., 2013); the fitted phase per echo spacing was unwrapped using MEDI's magnitude-guided region growing (unwrapPhase, MEDI toolbox) and divided by the echo spacing. Background field removal was performed using PDF (Liu et al., 2011a). Dipole inversion was performed using MEDI (Morphology Enabled Dipole Inversion) (Liu et al., 2011b), with the local field high-passed by a 5 mm spherical mean value (SMV) filter within the mask shrunk by that radius. MEDI's zero reference to the ventricular CSF (MEDI+0; Liu et al., 2018) was applied to runs of at least three echoes. No susceptibility referencing was applied.

## Note

Departures from the methods' recommendations:

- CG iterations (PDF) was 30; PDF recommends 300.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Smith, S.M. (2002). "Fast robust automated brain extraction." *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062
- Liu, T., et al. (2013). "Nonlinear formulation of the magnetic field to source relationship for robust quantitative susceptibility mapping." *Magnetic Resonance in Medicine*, 69(2):467-476. https://doi.org/10.1002/mrm.24272
- Liu, T., et al. (2011a). "A novel background field removal method for MRI using projection onto dipole fields (PDF)." *NMR in Biomedicine*, 24(9):1129-1136. https://doi.org/10.1002/nbm.1670
- Liu, T., et al. (2011b). "Morphology enabled dipole inversion (MEDI) from a single-angle acquisition: Comparison with COSMOS in human brain imaging." *Magnetic Resonance in Medicine*, 66(3):777-783. https://doi.org/10.1002/mrm.22816
- Liu, Z., et al. (2018). "MEDI+0: Morphology enabled dipole inversion with automatic uniform cerebrospinal fluid zero reference for quantitative susceptibility mapping." *Magnetic Resonance in Medicine*, 79(5):2795-2803. https://doi.org/10.1002/mrm.26946
