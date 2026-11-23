---
title: Toolbox pipelines
description: "Start from a published toolbox's whole pipeline with --pipeline or the TUI's Pipeline tab."
sidebar:
  label: Overview
  order: 0
---

A toolbox pipeline is a published toolbox's whole QSM pipeline written as QSMxT settings: the
toolbox's method, and the settings its authors' code or paper uses with it, from the brain mask
to the susceptibility reference. Use one to compare with results from that toolbox or to follow
a published analysis. QSMxT's defaults stay its own recommendation; a toolbox pipeline is not.

## Starting one

In the TUI, open the Pipeline tab (`P` from any settings page). Next to the list, the
highlighted pipeline is compared with your settings: each setting choosing it would change, from
your value to the pipeline's. `Enter` opens a preview of those changes; **Apply** makes them, and
`Esc` or **Cancel** changes nothing.

On the command line, give `--pipeline`; flags after it change the pipeline further:

```sh
qsmxt run study/bids --pipeline medi-toolbox
qsmxt run study/bids --pipeline medi-toolbox --medi-lambda 500
```

Choosing a pipeline first resets every reconstruction stage to QSMxT's defaults, then sets the
pipeline's settings. The input, the outputs and how the run executes stay as they are. The run's
`pipeline_config.toml` records the choice as `reference_pipeline`.

## Departures and recommendations

The TUI's header shows the pipeline your settings follow: `Pipeline: MEDI toolbox`. Change a
setting afterwards and it reads `MEDI toolbox · 1 departure`; change the pipeline's method and it
reads `Custom`. The command preview is `--pipeline` followed only by the flags where your settings
depart from it.

A pipeline holds what its authors ran. Where QSM.rs recommends something else for one of its
methods, the recommendation is shown in the preview and offered afterwards like any other (see
[Recommendations](/QSMxT/background/recommendations/)); nothing changes unless you accept it.

## Parity

How closely a pipeline matches the original toolbox is measured, not assumed.
[QSM-CI](https://qsmxt.github.io/QSM-CI/parity.html) runs each toolbox's own pipeline and QSMxT's
on the same data for every QSMxT release, and each pipeline's page shows the latest result. Each
page also lists what the pipeline does differently from the original on purpose, with the
reasons, and any known gaps.

## The pipelines

<!-- generated:pipelines start (from QSM.rs's toolbox pipelines: do not edit; regenerate with QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs) -->

| Pipeline | Taken from | `--pipeline` | Parity |
| --- | --- | --- | --- |
| [MEDI toolbox](/QSMxT/toolbox-pipelines/medi-toolbox/) | MEDI toolbox 2024.11.26 | `medi-toolbox` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-medi-cornell.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-medi-cornell) |
| [STI Suite · iLSQR](/QSMxT/toolbox-pipelines/sti-suite-ilsqr/) | STI Suite 3.0 | `sti-suite-ilsqr` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-sti.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-sti) |
| [STI Suite · STAR-QSM](/QSMxT/toolbox-pipelines/sti-suite-star/) | STI Suite 3.0 | `sti-suite-star` | no whole-pipeline test yet |
| [QSMART](/QSMxT/toolbox-pipelines/qsmart/) | QSMART | `qsmart` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-qsmart.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-qsmart) |
| [HEIDI package](/QSMxT/toolbox-pipelines/heidi/) | HEIDI package (qsm_heidi 4592631) | `heidi` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Fheidi.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-heidi) |
| [χ-separation toolbox · iLSQR](/QSMxT/toolbox-pipelines/chi-sep-ilsqr/) | χ-separation toolbox 1.1.3 | `chi-sep-ilsqr` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-chisep-ilsqr.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-chisep-ilsqr) |
| [χ-separation toolbox · MEDI](/QSMxT/toolbox-pipelines/chi-sep-medi/) | χ-separation toolbox 1.1.3 | `chi-sep-medi` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-chisep-medi.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-chisep-medi) |
| [χ-separation toolbox · χ-sepnet](/QSMxT/toolbox-pipelines/chi-sepnet/) | χ-separation toolbox 1.1.3 | `chi-sepnet` | no whole-pipeline test yet |
| [DECOMPOSE](/QSMxT/toolbox-pipelines/decompose/) | DECOMPOSE | `decompose` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-decompose.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-decompose) |
| [UK Biobank](/QSMxT/toolbox-pipelines/uk-biobank/) | UK Biobank QSM | `uk-biobank` | [![parity](https://img.shields.io/endpoint?url=https%3A%2F%2Fqsmxt.github.io%2FQSM-CI%2Fparity%2Fbadges%2Ftoolbox-pipeline-ukb.json)](https://qsmxt.github.io/QSM-CI/parity.html#pair-toolbox-pipeline-ukb) |

<!-- generated:pipelines end -->
