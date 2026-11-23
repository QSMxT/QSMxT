---
title: Toolbox pipelines
description: "Start from an original toolbox's whole pipeline with --pipeline or the TUI's Pipeline tab."
---

## Pipelines

The Pipeline tab lists QSMxT's defaults and the original toolboxes' pipelines: the MEDI
toolbox, STI Suite (iLSQR and STAR-QSM), QSMART, the HEIDI package, the χ-separation toolbox
(iLSQR, MEDI and χ-sepnet) and DECOMPOSE. Next to the list, the highlighted pipeline is
compared with your settings: each setting choosing it would change, from your value to the
pipeline's. Where a toolbox pipeline is known to differ from the toolbox's own, a note above
the comparison says how: for bipolar readouts, the MEDI toolbox uses a bipolar echo fit, which
is not supported yet, so its pipeline here always uses the monopolar fit. *About*, at the top of the list, explains what the toolbox pipelines are, with
a link to [QSM-CI's parity checks](https://qsmxt.github.io/QSM-CI/parity.html); `Enter` there
copies the link.

`Enter` opens a preview of those changes. **Apply** makes them; `Esc` or **Cancel** changes
nothing. A toolbox pipeline is its method and the original authors' settings that method
brings with it (the same ones its reference pipeline pop-up offers, see
[Recommendations](/QSMxT/background/recommendations/#in-the-tui)), so choosing the MEDI toolbox sets MEDI with BET, the
complex fit, region-growing unwrapping and PDF. The stages it covers start from QSMxT's
defaults first, so a value you changed there earlier is reset too; every other setting stays as
it is. A reasoned recommendation is not part of a pipeline: it is shown in the preview and
offered as usual afterwards.

The header shows the pipeline your settings follow, on every tab: `Pipeline: MEDI toolbox`.
Change a setting afterwards and it reads `MEDI toolbox · 1 departure`; change the method itself
and it reads `Custom`. Press `P` on any settings page to come back to the Pipeline tab. The
command preview starts with `--pipeline medi-toolbox`, followed only by the flags where your
settings depart from it.

## Pipelines

`--pipeline` starts from a whole pipeline: `qsmxt-defaults`, or an original toolbox's
(`medi-toolbox`, `sti-suite-ilsqr`, `sti-suite-star`, `qsmart`, `heidi`, `chi-sep-ilsqr`,
`chi-sep-medi`, `chi-sepnet`, `decompose`). A toolbox pipeline is its method and the original
authors' settings that method brings with it (see [Recommendations](/QSMxT/background/recommendations/#on-the-command-line)). The
stages it covers (masking, field mapping, background removal, inversion, referencing,
χ-separation) start from QSMxT's defaults; every other flag given then changes any setting.

```sh
# The MEDI toolbox's pipeline: BET, the complex fit, region-growing unwrapping, PDF, MEDI
qsmxt run study/bids --pipeline medi-toolbox

# The same, with a different MEDI regularisation weight
qsmxt run study/bids --pipeline medi-toolbox --medi-lambda 500
```

The run's settings file (`pipeline_config.toml`) records the pipeline as
`reference_pipeline`, and the TUI's command preview writes a configuration on a pipeline as
`--pipeline` plus the flags where it departs from it.
