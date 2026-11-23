---
title: "Geometry: oblique data, output space, padding"
description: "What QSMxT resamples, pads and crops, and why."
---

## Oblique acquisitions

The dipole kernel is built in the voxel grid, so an acquisition whose slices
are tilted relative to B0 has to be handled explicitly. QSMxT applies the first
of these that fits:

1. **A deep-learning method forces resampling to axial.** These learned the
   dipole relationship from axially-acquired data and take no direction
   argument, so nothing else can help them.
2. **A `B0_dir` in the JSON sidecar wins.** It describes the acquisition better
   than the affine can.
3. **`--axial-resampling` resamples to axial** when the obliquity exceeds
   `--obliquity-threshold` (degrees, default 10). Off by default. A negative
   threshold is refused; a config file that gives `obliquity_threshold`
   without `axial_resampling` is read with a deprecation warning: negative as
   off, otherwise on at that threshold.
4. **Otherwise the kernel is rotated** — B0 is taken from the affine and used
   as-is. This is the default, and normally the one you want.

Rotating the kernel is preferred because it is exact and free: the dipole
kernel takes the field direction as a parameter, so pointing it the right way
costs nothing, keeps the acquired grid, and interpolates nothing.

The cost of the alternative is not small. A 256×288×48 acquisition at 32.5°
obliquity resamples to 272×339×77, roughly twice the voxels, and took 459 s
against 246 s for the same reconstruction on the acquired grid. Resampled
outputs are returned to the acquired grid afterwards unless you ask otherwise;
see [Output space](#output-space).

### Which methods need axial data

| | behaviour on oblique data |
|---|---|
| all classical dipole inversions, HEIDI, PDF, χ-sep iLSQR and MEDI | correct as acquired — B0 is an explicit parameter |
| QSMnetME, χ-sepnet (its default route) | correct as acquired — rotate their inputs to B0 along z themselves, as their authors' toolbox does |
| SHARP, V-SHARP, RESHARP, iSMV, LBV, HARPERELLA, iHARPERELLA, BFRnet | unaffected — never uses B0 |
| the other deep-learning inversions, SUSEP-Net, R2PRIMEnet, iQFM, χ-sepnet with `--chi-sepnet-route native` | **resampled first** — assumes B0 is +z |

Background removal by the spherical mean value property is genuinely
direction-independent rather than merely untested, which is why PDF is the only
background remover in the first row.

`qsmxt validate` reports what a dataset will do:

```
B0 direction: (0.05, 0.39, 0.92) (from the affine)
Obliquity:    32.5° (B0 22.9° off the slice normal)
```

Obliquity is `nibabel`'s definition (the norm of the per-axis angles). It is a combined measure rather than a tilt: a single
23° oblique acquisition scores ≈32° because two voxel axes move. The tilt in
brackets is the physical angle between B0 and the slice normal, which is what
the kernel cares about.

:::caution
Wrapped phase cannot be resampled on its own. Halfway between `+3.0` and `−3.0`
rad a linear interpolator returns `0.0`, where the answer is near `±π`, so
every wrap becomes a band of wrong values. The pipeline always resamples
magnitude and phase together; if you use `qsmxt resample` by hand, pass
`--phase` with `--magnitude` rather than resampling phase as a plain volume.
:::

R2PRIMEnet is in the third row because it is a χ-sepnet-family network, trained
on the same axial data and taking no B0 direction. It only runs when R2′ is
being estimated rather than measured, so whether a run is resampled on its
account depends on whether that run has an R2 to subtract: a MESE (or
`--use-custom-r2`) with `--r2prime-strategy auto` measures R2′ instead, and
nothing is resampled. iQFM is there because it is the local-field head of the
same network as iQSM.

A matching MESE acquisition no longer changes any of this. The MESE is resampled
onto whatever grid the run reconstructs on, so an oblique run with a MESE
resamples like any other, and a deep-learning method on one is no longer
refused. The MESE is also no longer required to share the GRE's grid at all: a
2D spin-echo stack with thicker slices or a smaller matrix is reconciled by its
affine, where it used to be dropped and R2′ silently never computed. Where the
MESE's field of view does not cover all of the brain, R2 and a measured R2′ are
reported only inside it, and the log says how much of the mask that was.

## Output space

A run that was resampled to axial reconstructs on the resampled grid, and by
default its derivatives are returned to the grid the data arrived on. This
matters more than it sounds: a FLIRT matrix is defined in a space derived from
the image's dimensions and voxel sizes, so applying one to a volume on a
different grid gives a wrong registration rather than an error.

- `--output-space acquired` (default) writes derivatives on the acquired grid.
  Costs a second interpolation, so the result is slightly smoother than a
  reconstruction that was never resampled.
- `--output-space working` leaves them on the resampled grid. Their names then
  carry `space-axial` (`sub-01_space-axial_Chimap.nii`) and each sidecar a
  `SpatialReference` (the run's `part-mag_space-axial_T2starw`), so they cannot
  be mistaken for acquired-grid outputs. A run that was never resampled keeps
  its plain names under either setting.

Masks travel by nearest neighbour and phase travels with its magnitude through
the complex domain, for the same reason phase went out that way.

## Grid padding and cropping

All three trade reconstruction cost against the grid the FFT stages run on.
`--fft-padding` and `--crop-to-mask` are off by default; iLSQR always pads.

### `--fft-padding`

FFT cost is `O(N log N)` in the whole grid, and the transform is much faster on
sizes built from small prime factors. Resampling has no reason to land on one:
the grid above comes out 272×339×77, which is 2⁴·17, 3·113 and 7·11.
Growing it to 280×343×80 took one FFT from 131 ms to 71 ms for 8% more voxels.

Padding discards nothing, but it is **not** a no-op. The dipole kernel is
sampled at `k = n / (N·Δx)`, so changing `N` changes where k-space is sampled
and the deconvolution with it. Measured end to end on that acquisition, against
the same run unpadded:

| inversion | median difference | p99 | as % of dynamic range |
|---|---|---|---|
| TKD (direct) | 4e-7 ppm | 9.4e-3 ppm | 0.000% / 5.7% |

A direct inversion is unchanged for the typical voxel and moves only where the
deconvolution is ill-conditioned. An iterative one solves on a different grid,
so it can move slightly everywhere. Neither is evidence that padding is
worse — a finer k-space sampling with the boundary further from the object has
reason to be better — but nothing here establishes that it is better, so it does
not change anyone's numbers unless asked for.

Note that an acquisition usually arrives on a friendly grid already: 256×288×48
is 2⁸, 2⁵·3² and 2⁴·3. Padding earns its keep on grids that resampling created.

### `--crop-to-mask`

Reconstructing only inside a box around the brain cut background removal from
11.5 s to 7.2 s and inversion from 6.0 s to 3.0 s on the same acquisition. But
the FFT stages were 5% of wall time there — HD-BET masking was 289 s of 338 s —
so the end-to-end saving was inside noise. Use it for grids with large empty
regions, not as a general speedup.

### `--ilsqr-pad-mm`

iLSQR crops the field to the mask's bounding box and zero-pads it before
solving. Each axis is padded to the next multiple of 16 above the crop plus
`2 × pad / voxel size`, and both LSQR solves run on that grid. The default is
12 mm per side.

The padding changes χ slightly, because the dipole convolution is circular on
the padded grid. Larger padding costs time and memory roughly in proportion to
the padded grid. Too little padding lets a thin slab or small field of view wrap
onto itself.

### `--ilsqr-precision`

`double` (the default) runs iLSQR's two LSQR solves in double precision.
`single` runs them in single precision, as the original implementation does,
and follows its output to within its own differences between computers. It is
slower and uses more memory. On whole-head data both settings agree with the
original to within 0.02 % NRMSE; the solves stop after a fixed number of
iterations, which amplifies rounding, so on small noisy volumes only `single`
follows it closely.
