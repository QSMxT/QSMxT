## Command

qsmxt run <bids_dir> --pipeline decompose

## Config hash

848754b505db4d7a

## Steps (dry run on a 3-echo 3 T scan: step, state, cache key)

load                     will_run   b637ff66a1d891d8
scale_phase              will_run   565f39e380eb4906
magnitude                will_run   d82b40a650fa4990
mask                     will_run   79b20a1f57f10bfe
motion                   skipped    -
unwrap                   will_run   45774352f6f06114
bgremove                 will_run   d860293ca9f49328
invert                   will_run   df9e21696a6ded2f
reference                will_run   a619656f16c59b2f
chi_separation           will_run   72c53368251f426b
output_space             skipped    -

## Methods

# Methods

QSM processing was performed using qsmxt (Stewart, 2026). Processing followed the DECOMPOSE pipeline. A brain mask was generated using BET brain extraction (Smith, 2002) of the first-echo magnitude image. Phase was converted to ppm with the scanner's centre frequency (ImagingFrequency). For multi-echo data, QSM was reconstructed separately for each echo: each echo's unwrapped phase was divided by its echo time to give that echo's field map, and background field removal, dipole inversion and referencing were applied to each echo's field map on its own. Phase unwrapping was performed separately for each echo, on the raw phase over the whole volume, using the Laplacian method (Schofield & Zhu, 2003) with an FFT Poisson solve on the volume zero-padded by 12 voxels. After background field removal, the mask was reduced to its largest connected component with its holes filled, and the local field was set to zero outside it. Background field removal was performed using V-SHARP (Wu et al., 2012). The V-SHARP background was estimated on an isotropic grid: the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution. Dipole inversion was performed using STAR-QSM (Streaking Artifact Reduction) (Wei et al., 2015) with zero-padding of 0 × 0 × 0 voxels per side and the field converted to phase at the sum of the echo times. Susceptibility values were referenced to the mean susceptibility within the reconstruction mask (the brain mask less any rim eroded by background field removal). For multi-echo data, the susceptibility map reported is the mean across echoes of the echoes' maps, over the voxels where every echo's map was defined; each echo's map is reported too. Paramagnetic and diamagnetic susceptibility maps were computed using DECOMPOSE-QSM (Chen et al., 2021) within the reconstruction mask, with the paper's fit (the compartment amplitudes constrained to sum to one and R2*₀ fitted by linear least squares, χ± by trust-region-reflective least squares, each voxel's magnitude scaled to its signal at TE = 0, extrapolated from the first two echoes). Each echo's susceptibility map was used for the signal phase at that echo.

## References

- Stewart, A. (2026). QSMxT. https://github.com/QSMxT/QSMxT
- Smith, S.M. (2002). "Fast robust automated brain extraction." *Human Brain Mapping*, 17(3):143-155. https://doi.org/10.1002/hbm.10062
- Schofield, M.A., Zhu, Y. (2003). "Fast phase unwrapping algorithm for interferometric applications." *Optics Letters*, 28(14):1194-1196. https://doi.org/10.1364/OL.28.001194
- Wu, B., et al. (2012). "Whole brain susceptibility mapping using compressed sensing." *Magnetic Resonance in Medicine*, 67(1):137-147. https://doi.org/10.1002/mrm.23000
- Wei, H., Dibb, R., Zhou, Y., Sun, Y., Xu, J., Wang, N., Liu, C. (2015). "Streaking artifact reduction for quantitative susceptibility mapping of sources with large dynamic range." *NMR in Biomedicine*, 28(10):1294-1303. https://doi.org/10.1002/nbm.3383
- Chen, J., et al. (2021). "Decompose quantitative susceptibility mapping (QSM) to sub-voxel diamagnetic and paramagnetic components based on gradient-echo MRI data." *NeuroImage*, 242:118477. https://doi.org/10.1016/j.neuroimage.2021.118477
