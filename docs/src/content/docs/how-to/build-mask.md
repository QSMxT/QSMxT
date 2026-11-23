---
title: Build a custom mask
description: "Masking presets, custom mask sections, combining and refining, two-pass masking."
---

## Masking

Choose a masking recipe with `--mask-preset`, and override the image it
operates on with `--masking-input`.

| Value | Method |
| --- | --- |
| `robust-threshold` | Otsu thresholding of the phase-quality map, refined with dilation, hole-filling and erosion (default) |
| `bet` | Brain Extraction Tool on the magnitude image, then two erosions |
| `hd-bet` | HD-BET deep-learning brain extraction on the magnitude image, followed by signal-gated erosion (the QSM-CI harmonization masking). Needs a deep-learning build; the weights (~120 MB, CC-BY-NC-4.0) are downloaded on first use |
| `bet-and-phase` | BET on the first-echo magnitude intersected with the `robust-threshold` mask of the phase-quality map, then hole-filled and eroded (the masking recommended by the [ISMRM EMTP study group consensus](https://doi.org/10.1002/mrm.30006)) |
| `rs2-net` | RS2-Net deep-learning **rodent** brain extraction (Lin et al., 2024) on the magnitude image, with no refinements: it follows the inner skull directly. Needs a deep-learning build; the weights are downloaded on first use |
| `mindgrab` | MindGrab deep-learning brain extraction (Fani et al., 2025) on the magnitude image, with no refinements. Works on any contrast; the image is resliced to a 256³ 1 mm grid for the network using its NIfTI affine, and the mask is returned on the original grid. Needs a deep-learning build; the weights are downloaded on first use |

Each preset is exactly the custom mask sections below (see the next
paragraphs for the grammar), so a preset can be used as the starting point for
your own recipe:

| Preset | Equivalent flags |
| --- | --- |
| `robust-threshold` | `--mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1` |
| `bet` | `--mask magnitude,bet:0.50,erode:2` |
| `hd-bet` | `--mask magnitude,hd-bet,signal-erode` |
| `bet-and-phase` | `--mask magnitude-first,bet:0.50 --mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1 --mask-combine and --mask-refine fill-holes:0 --mask-refine erode:1` |
| `rs2-net` | `--mask magnitude,rs2-net` |
| `mindgrab` | `--mask magnitude,mindgrab` |

**Masking input** (`--masking-input`): `magnitude-first`, `magnitude`,
`magnitude-last`, `phase-quality`. For example,
`--mask-preset robust-threshold --masking-input magnitude` thresholds the
combined magnitude image instead of the phase-quality map.

For full control, compose custom mask sections with `--mask` (repeatable),
e.g. `--mask magnitude,bet:0.5,erode:2`. Each section names its input
followed by a generator and refinement operations.

Some scanners write a constant phase where they recorded none (Philips, for
example, outside its own signal mask). Such a phase looks flat and reliable, so
every recipe's mask leaves those voxels out, and the log and the methods say so.
`qsmxt mask preset --phase` does the same for a standalone mask.

- **Generators:** `threshold[:otsu|fixed:<v>|percentile:<p>|sti-gui]` (`sti-gui` is
  STI Suite QSM_GUI's thresholding: the level its `thresh_tool` starts at, Otsu's
  threshold of the middle slice scaled to that slice's maximum), `bet[:<f>][:scale=<s>]`,
  `hd-bet[:full|window|<X>x<Y>x<Z>|low-memory][:tta][:step=<s>]`, `rs2-net[:tta][:step=<s>]` and `mindgrab[:tta]`. HD-BET runs on the
  whole volume by default (`full`; about 6–8 GB measured). `window` runs 192×192×96
  sliding-window patches instead (about 3.6–4.5 GB measured, about twice as long),
  `low-memory` 128×128×64 patches (about 1.5–1.9 GB measured), and `step=` sets the
  window stride. `tta` adds mirroring test-time augmentation, which is roughly
  8× slower. For `mindgrab`, `tta` adds a left–right flipped pass, which doubles
  the run time.
- **Refinements:** `erode[:<n>]`, `dilate[:<n>]`, `close[:<r>]`, `fill-holes[:<max>]`
  (`0`, or omitted, is automatic: holes up to 5% of the volume),
  `gaussian[:<sigma_mm>]`, `min-intensity:<v>` (remove the mask's voxels whose
  intensity in the section's input image is below `<v>`), `majority` (a 3×3×3
  majority filter, MATLAB's `bwmorph3` `'majority'`: a voxel is kept when at least
  14 of its 27 neighbours, itself included, are in the mask), `largest-component`
  (only the largest 26-connected component, as MATLAB's `bwlabeln`), `imfill`
  (every hole filled whatever its size, as MATLAB's `imfill` `'holes'`), and
  `signal-erode[:<threshold>[:<depth_cap>[:<global_erosions>[:<bias_sigma>[:<min_component>]]]]]`.
  `signal-erode` removes only boundary voxels whose bias-corrected magnitude is
  below `threshold` × the in-mask median (default 0.80). It works inward through
  sinus and skull-base signal dropout, never more than `depth_cap` voxels deep
  (default 5), after `global_erosions` plain erosions (default 1). Dark interior
  structures are never removed. `bias_sigma` (default 12) is the Gaussian scale
  in voxels of the receive-coil bias divided out first, and `min_component`
  (default 1000) keeps every connected component that size or larger rather than
  only the largest. In the TUI each of these is its own row under the step, so
  you can nudge them with ←/→ and watch the generated command update.

  `global_erosions` is not the same as putting an `erode` step in front of
  `signal-erode`: the gate and the coil-bias estimate are computed from the
  mask as it arrives, and the depth cap is measured from *that* surface, so
  these erosions spend the depth budget and leave the gate unchanged. An `erode`
  step beforehand shrinks the mask the gate is derived from and resets the depth
  budget. The default of 1 is the QSM-CI harmonization setting.

For example, `--mask magnitude,hd-bet:low-memory,signal-erode` is the `hd-bet`
preset with the low-memory patch size, and
`--mask magnitude-first,bet:0.20,min-intensity:100,majority,close:2` is QSMART's
mask (`brainmask.m`), which `--pipeline qsmart` sets up with N4 correction, and
`--mask magnitude-last,threshold:sti-gui,largest-component,imfill` is STI Suite
QSM_GUI's, which `--pipeline sti-suite-ilsqr` sets up with `--bf-polish-mask`
(after V-SHARP, the mask's largest component with its holes filled, as the GUI's
`PolishMask`) and an unreferenced χ.

The [UK Biobank pipeline](/QSMxT/toolbox-pipelines/uk-biobank/) masks with HD-BET on the
magnitude; its page gives the mask's settings and why.

BET's surface settings (`--bet-smoothness`, `--bet-gradient-threshold`,
`--bet-iterations`, `--bet-subdivisions`, `--bet-voxel-scale`) and its
orientation apply to every BET step. `--bet-orientation fsl` (the default) runs
BET on the image as FSL's `bet` reads a NIfTI file, left-right reversed when the
image is stored neurologically. `--bet-orientation as-stored` runs it on the
voxels in the order they are stored, as MEDI's `BET.m` hands its bet2 mex the
array; the MEDI toolbox and χ-separation pipelines use it. BET's surface is not
symmetric, so the two masks differ slightly at the brain's edge.

### Combining sections

With more than one `--mask` section, `--mask-combine` decides how they fold
together: `or` (the default) keeps a voxel any section keeps, and `and` keeps
only voxels every section keeps. `--mask-refine` (repeatable) then applies
refinement operations to the combined mask — which is where hole-filling
belongs, since filling a section's holes before an intersection is not the same
as filling the intersection's.

The `bet-and-phase` preset is exactly this:

```bash
qsmxt run bids/ \
  --mask magnitude-first,bet:0.50 \
  --mask phase-quality,threshold:otsu,dilate:1,fill-holes:0,erode:1 \
  --mask-combine and \
  --mask-refine fill-holes:0 \
  --mask-refine erode:1
```

BET bounds the head, the phase-quality threshold drops voxels whose phase
cannot be unwrapped reliably, and the holes their intersection leaves inside
the brain are filled afterwards.

The two-pass reliable mask (`--two-pass-mask`, repeatable) has its own combine
mode, `--two-pass-mask-combine or|and`, which is the main mask's `--mask-combine`
unless set. Its combined mask then takes the same `--mask-refine` steps as the
main mask, and is intersected with the main mask.

### Two-pass modes

`--two-pass-mode` sets how the second pass is made and merged:

| Mode | Holes | Background removal of the second pass | Merge |
|---|---|---|---|
| `local` (default) | After holes of up to 27 voxels are filled, only holes with a core of at least 20 voxels of phase quality below 5 (after an opening that cuts thin vessels off) are kept, with their own rim: strong sources such as bleeds and calcifications, not veins or low-quality patches in the pallidum | The configured method on the mask with those holes; voxels next to the holes that it leaves without a value are filled by LBV | The second map within 6 mm of a hole, blended into the first out to 12 mm; the first map in the holes and everywhere else |
| `original` | Every voxel outside the reliable mask (Stewart et al., 2022) | The configured method on the reliable mask | The second map wherever it is defined, the first elsewhere |

`local` keeps the published method's gain next to strong sources without lowering the
deep grey matter: with `original`, the many small and vessel-shaped holes shrink
V-SHARP's kernel throughout the brain, which lowers χ in the pallidum and other
nuclei. Where a scan has no hole with a deep core, `local` returns the single-pass
map. QSM.rs (`pipeline::two_pass`) records the measurements behind the default and
how each inversion treats the holes.

To try this on files without a pipeline run, use
[`qsmxt mask preset bet-and-phase`, or `qsmxt mask and` / `or`](/QSMxT/reference/tools/#masking--qsmxt-mask)
on masks you already have.
