---
title: Choose methods
description: "Pick the method for each stage of the pipeline."
---

## Choosing algorithms

```sh
qsmxt run study/bids \
  --mask-preset robust-threshold \
  --unwrapping-algorithm romeo \
  --bf-algorithm vsharp \
  --qsm-algorithm ilsqr
```

| Option | Choices |
| --- | --- |
| `--qsm-algorithm` | classical (`rts`, `tv`, `tkd`, `tgv`, `medi`, `ilsqr`, …) and deep-learning (`xqsm`, `qsmnet`, `iqsm`, …) |
| `--unwrapping-algorithm` | `romeo`, `laplacian` (QSMART's reference), `prelude`, `medi-region-growing` |
| `--bf-algorithm` | `vsharp`, `pdf`, `lbv`, `ismv`, `sharp`, `resharp`, `harperella`, `iharperella`, `bfrnet`, `iqfm` |
| `--mask-preset` | `robust-threshold`, `bet`, `hd-bet`, `bet-and-phase`, `rs2-net`, `mindgrab` |
| `--masking-input` | `magnitude-first`, `magnitude`, `magnitude-last`, `phase-quality` |

Every value of every option is listed in the [CLI reference](/QSMxT/reference/cli/)
and described in [Algorithms](/QSMxT/reference/methods/).

`--masking-input` overrides the image the mask is computed from and combines
with `--mask-preset` — e.g. `--mask-preset bet --masking-input magnitude-first`
runs BET on the first-echo magnitude. For fully custom masking, see `--mask`
in [Algorithms](/QSMxT/how-to/build-mask/#masking).
