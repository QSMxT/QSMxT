---
title: Susceptibility source separation
description: "Where R2′ comes from, and the source-separation methods and their settings."
---

## Where R2′ comes from

R2′ (`--do-r2primemap`, and every R2′-based [source separation](#susceptibility-source-separation))
comes from one of three places, chosen by `--r2prime-strategy`:

| `--r2prime-strategy` | R2′ is |
| --- | --- |
| `auto` (default) | **measured** as R2\* − R2 when the run has an R2 (a matched MESE, or `--use-custom-r2`), otherwise **predicted** from R2\* by R2PRIMEnet |
| `mese` | only ever measured; a run without an R2 gets no R2′, and asking for R2′ or an R2′-based separation on it is an error |
| `r2primenet` | always predicted by R2PRIMEnet, even when a MESE is present |

`--use-custom-r2prime <TOOL>` bypasses all three and reads an R2′ map from
`derivatives/<TOOL>/`. So with the default `auto`, an R2′-based separation does not
need a MESE: without one it runs on R2PRIMEnet's prediction, which is trained at
3 T only (see below), and the R2′ sidecar's `R2primeSource` says which it was. χ-sepnet's
default route makes that prediction itself ([below](#susceptibility-source-separation)).

Both measured and predicted R2′ maps have negative values set to zero.

## R2′ coverage and field strength

A MESE is often a slab rather than a whole head, because whole-brain MESE is
expensive. R2′ is only defined where R2 exists, so the voxels the MESE did not
reach are left at zero rather than being filled with `R2* − 0 = R2*`, which
would report the whole of R2* as reversible and overstate R2′ by roughly the
tissue R2 (10–20 s⁻¹ at 3 T). Every χ-separation method built on that R2′
inherits the gap, so expect the derived maps to be empty there too. The pipeline
writes where its R2′ is a measurement as `_desc-r2prime_mask.nii`. What should
happen to the uncovered region instead is still open; see
[QSMxT#237](https://github.com/QSMxT/QSMxT/issues/237).

`qsmxt r2prime` applies the same rule standalone. It infers coverage from
`R2 > 0`, which is exact for an R2 fitted on the same grid. An R2 map resampled
from another grid is the case that needs help: interpolation across the slab
boundary leaves small non-zero values just outside the acquired slices, and
nothing in the map distinguishes them from tissue. Pass `--r2-coverage` with a
mask of where the acquisition actually reached, and `--output-coverage` to write
out which voxels ended up measured.
`qsmxt r2prime --r2primenet --field-strength <T>` is the standalone R2PRIMEnet
prediction from an R2\* map, under the same field-strength rule as
`--r2prime-strategy r2primenet`.

R2PRIMEnet, which predicts R2′ from R2* when there is no R2 to subtract, is
**trained at 3 T only**. R2* scales with field strength and the network takes
none as an input, so on 7 T data it returns a map of the right shape and the
right units with the wrong numbers, which nothing downstream can detect. So away
from 3 T (more than 0.3 T off):

| `--r2prime-strategy` | without `--r2primenet-ignore-field-mismatch` | with it |
| --- | --- | --- |
| `auto` (no R2 to subtract) | no R2′: an error if R2′ or an R2′-based separation was asked for | R2PRIMEnet runs, with a warning |
| `r2primenet` | the run stops with an error | R2PRIMEnet runs, with a warning |

The R2′ map's JSON sidecar says where it came from: `R2primeSource` is
`measured` (R2* − R2), `predicted` (R2PRIMEnet, with `R2primenetCalibrated` false
away from 3 T) or `supplied` (`--use-custom-r2prime`), with `R2primeMethod`
naming how.

Treat any χ-separation built on a non-3 T R2PRIMEnet prediction as
uncalibrated; better, acquire a MESE or supply an R2′ with
`--use-custom-r2prime`. `--r2primenet-patch D H W` sets the sliding-window patch
(default 192 192 128; each a multiple of 16). In the TUI both are under the R2′
source row. The authors publish a 7 T variant that maps 7 T R2* to
3 T-equivalent R2′; it is not implemented yet.

## Susceptibility source separation

Enable with `--do-chisep` to split susceptibility into paramagnetic and
diamagnetic components, and choose the method with `--chisep`.

<!-- generated:separation-methods start (from QSM.rs's method catalogue and the settings registry: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Value | Method | Description | Citation |
| --- | --- | --- | --- |
| `r2star-qsm` | R2\*-QSM (default) | R2\*-QSM closed-form — QSM + R2\*, GRE-only. | [Dimov et al., 2022a](https://doi.org/10.1111/jon.13014); [Dimov et al., 2022b](https://doi.org/10.3390/tomography8030127) |
| `decompose` | DECOMPOSE-QSM | DECOMPOSE-QSM signal-domain fit — QSM + multi-echo magnitude, GRE-only. | [Chen et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118477) |
| `chi-sep-ilsqr` | χ-separation (iLSQR) | χ-separation iLSQR — local field + R2' + magnitude + QSM. | [Shin et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118371) |
| `chi-sep-medi` | χ-separation (MEDI) | χ-separation MEDI — local field + R2' + magnitude. | [Shin et al., 2021](https://doi.org/10.1016/j.neuroimage.2021.118371) |
| `wavesep` | WaveSep | WaveSep wavelet-L1 — QSM + R2'. | [Fang et al., 2023](https://doi.org/10.1007/978-3-031-44858-4_6) |
| `hc-chisep` | HC-ChiSep | Hollow-cylinder χ-separation — QSM + R2' + multi-echo magnitude. | Stewart et al., 2026; [Wharton & Bowtell, 2012](https://doi.org/10.1073/pnas.1211075109) |
| `susep-net` | SUSEP-Net | SUSEP-Net deep-learning separation — QSM + R2' + local field (weights auto-downloaded). | [Li et al., 2025b](https://doi.org/10.48550/arXiv.2506.13293) |
| `chi-sepnet` | χ-sepnet | χ-sepnet deep-learning separation — QSM + R2' + local field (weights auto-downloaded). | [Kim et al., 2025](https://doi.org/10.1002/hbm.70136) |

<!-- generated:separation-methods end -->

Only `r2star-qsm` and `decompose` run on a GRE acquisition alone. The rest are
based on R2′, which comes from a multi-echo spin-echo acquisition, a
bring-your-own R2′ map or, with the default `--r2prime-strategy auto` and neither
of those, R2PRIMEnet's prediction from R2\* (3 T only); see
[Where R2′ comes from](#where-r2-comes-from) and [Input data](/QSMxT/reference/inputs/).

`chi-sep-ilsqr` and `chi-sep-medi` are the χ-separation toolbox's `chi_sep_iLSQR` and
`chi_sep_MEDI`, run as its script runs them.

- `chi-sep-ilsqr` solves the two sources by LSQR, streaking removal and a non-negative
  refinement, initialised from the QSM; it does not use the magnitude. It is initialised with a
  QSM that the χ-separation step reconstructs from the local field by iLSQR, as the toolbox's
  script does, rather than with the run's own QSM: 100 iterations and 12 mm padding per side
  by default (`--chi-sep-ilsqr-qsm-max-iter`, `--chi-sep-ilsqr-qsm-tol`,
  `--chi-sep-ilsqr-qsm-pad-mm`, and `--chi-sep-ilsqr-qsm-precision` as for
  [`--ilsqr-precision`](/QSMxT/background/geometry/#--ilsqr-precision)). With `--use-custom-qsm` it takes the supplied QSM instead.
  It runs on the field zero-padded by `--chi-sep-ilsqr-pad` voxels at both ends of each axis
  (default `16 16 16`).
- `chi-sep-medi` is initialised with the same reconstructed iLSQR QSM, unreferenced, as the
  toolbox's script does, rather than with the run's own QSM (`--chi-sep-medi-qsm-max-iter`,
  `--chi-sep-medi-qsm-tol`, `--chi-sep-medi-qsm-pad-mm` and `--chi-sep-medi-qsm-precision`, with
  the same defaults); with `--use-custom-qsm` it takes the supplied QSM instead. It weights its data terms by a noise map
  made from every echo's magnitude (MEDI toolbox `Fit_ppm_complex`), and keeps χ uniform in a
  CSF mask found from R2\* (MEDI's `extract_CSF`), weighted by `--chi-sep-medi-lambda-csf`.
  `--chi-sep-medi-csf-r2star` picks that R2\*: `arlo` (MEDI's ARLO, the default; at least three
  echoes) or `nnls` (the toolbox's own `r2star_nnls` fit, at least two echoes). The toolbox's
  script uses ARLO only for echo times given as a column, and documents them as a row, so it
  always runs its NNLS fit; `--pipeline chi-sep-medi` uses `nnls`. It needs the magnitude of
  every echo. The standalone `qsmxt separate chi-sep-medi --csf-mask` takes a CSF mask file
  instead, or `none` for no CSF term, as the toolbox's GUI runs without a CSF mask
  (`--chi-sep-medi-csf-r2star` picks the R2\* otherwise).

Their relaxometric constants D_r default to the toolbox's 137 × B0 / 2.9 T Hz/ppm (141.7 at
3 T); `--chi-sep-ilsqr-dr-pos` / `-dr-neg` and `--chi-sep-medi-dr-pos` / `-dr-neg` set them
(`auto` for the default). `wavesep` uses 137 Hz/ppm at every field strength, from the original
χ-separation paper (Shin et al. 2021), which was measured by regressing R2' against a
single-orientation QSM. The χ-sepnet paper (Kim et al. 2025) repeated that regression against
COSMOS and got 114 Hz/ppm; that value is baked into χ-sepnet and SUSEP-Net as a training
normalisation and is not a parameter there. If your χ_total is COSMOS-derived, 114 is the
better match for the analytic methods too.

`decompose` fits each voxel's multi-echo signal in three alternating stages. By default
(`--decompose-fit reference-code`) it runs the fit of the authors' public code, each stage solved by
MATLAB `lsqcurvefit`'s trust-region-reflective method with its options:
`--decompose-max-iterations` (default 400), `--decompose-max-function-evaluations` (default
100 × the fit's parameters), `--decompose-function-tolerance`, `--decompose-step-tolerance`
and `--decompose-optimality-tolerance` (default 1e-6 each), `--decompose-finite-difference-step`
(default √eps) and `--decompose-typical-x` (default 1). `--decompose-fit paper`, which the
DECOMPOSE pipeline uses, runs the paper's fit instead (Chen et al. 2021, §2.2): the three
amplitudes constrained to sum to one and R2*₀ fitted by linear least squares, χ± by
trust-region-reflective least squares, and each voxel's magnitude scaled to its signal at TE = 0,
extrapolated from the first two echoes (the paper does not say how it scales the data; this is
QSM.rs's reading). It takes the phase of each echo on its
principal branch, as the authors' code does; `--decompose-unwrap-phase true` unwraps it along
the echoes. `--decompose-chi-bound` bounds |χ| in the fit: a number in ppm (default 0.5, the
authors'), or `echo-spacing`, which computes π / ((2/3)·γ·B0·ΔTE) from the scan, ΔTE being the
largest of TE1 and the echo spacings: the largest |χ| whose phase changes by less than π between
echoes (0.31 ppm at 7 T with 8 ms spacing). At 7 T (6.5 T and above) the recommendations suggest
both, since the method was designed for 3 T and the phase wraps between echoes at 7 T. In a
settings file the bound is `chi_bound = 0.5` or `chi_bound = "echo-spacing"` under
`[separation.decompose]`. Its authors give it a QSM for each echo. With
[`--qsm-per-echo true`](/QSMxT/reference/methods/#per-echo-qsm), which choosing DECOMPOSE recommends, `qsmxt run` gives it
each echo's map; otherwise it uses its one QSM at every echo. The methods text says which.
`qsmxt separate decompose` takes one per echo (`--qsm-per-echo`).

χ-sepnet runs its authors' route by default (`--chi-sepnet-route toolbox`): the local field
and R2′ are resampled to 1 mm with B0 along z, its susceptibility channel is computed there by
QSMnetME, and the maps are resampled back. Without a measured R2′ it predicts R2′ from R2* with
R2PRIMEnet inside that route (3 T only, as `--r2primenet-ignore-field-mismatch` describes). That
needs R2* only, so no `r2_r2prime` step runs. `--do-r2primemap` on such a run also publishes an R2′
map made by R2PRIMEnet on the run's own grid; χ-sepnet does not read it, and its sidecar says so
(`R2primeUsedByChiSeparation: false`). The χ+/χ− sidecars record the R2′ the route used. The
networks run on the whole 1 mm frame at once; `--chi-sepnet-frame-patch D H W` runs them as a
sliding window instead (each a multiple of 16). `--chi-sepnet-route native` runs the network alone
on the acquired grid with the run's QSM, as a sliding window of 192×192×128-voxel patches
(`--chi-sepnet-patch D H W`, each a multiple of 16). On the toolbox route, and with χ-separation
MEDI and iLSQR, the toolbox's [Tukey windowing](/QSMxT/reference/methods/#tukey-windowing) of the echoes is shown by
manufacturer as the original authors' setting (`--tukey-window 0.4`, none for Philips). SUSEP-Net runs
the whole volume in one pass; `--susep-net-patch D H W` runs it as a sliding
window of that size instead (each a multiple of 8), so its memory follows the
patch rather than the volume.

χ-sepnet and SUSEP-Net set negative χ+ and |χ−| values to zero.

Outputs are written as `desc-paramagnetic_Chimap`, `desc-diamagnetic_Chimap` and
`desc-total_Chimap`.

Separation is fitted over the voxels the susceptibility map is defined on — its
support, written as `desc-qsm_mask` (see [Referencing](/QSMxT/background/pipeline/#referencing)) — not over
the whole brain mask: the rim that background-field removal eroded has no χ to
separate. The methods that also fit the local field are limited to where that
field is defined. R2' and R2\* are restricted to the same voxels before fitting,
except for `chi-sep-ilsqr`, `chi-sep-medi` and χ-sepnet's toolbox route, which take
them as the χ-separation toolbox's script passes them: R2′ as measured or supplied
and R2\* over the brain mask (`chi_sep_iLSQR` reads R2′ just outside its mask, and
the toolbox route resamples it before masking). All three outputs are 0 outside
the support. With a bring-your-own QSM, the support
is the brain mask less the voxels where that map is 0.
