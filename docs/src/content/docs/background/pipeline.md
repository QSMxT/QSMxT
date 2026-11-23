---
title: How the pipeline works
description: "The stages every run goes through, and which methods skip which stages."
---

## The stages

Each run flows through the pipeline below. The method at every stage is
configurable — see [Algorithms](/QSMxT/reference/methods/).

1. **Masking** — generate a brain/region mask (`--mask-preset`, or custom
   `--mask` sections).
2. **Phase offset removal** — remove receiver phase offsets on multi-echo data.
3. **Phase unwrapping** — `romeo`, `laplacian`, `prelude` or `medi-region-growing`.
4. **Echo combination / B0 mapping** — combine echoes into a field map
   (`--b0-estimation`, `--b0-weight-type`).
5. **Background field removal** — `vsharp`, `pdf`, `lbv`, and more.
6. **Dipole inversion** — `ilsqr` (default), `rts`, `tv`, `tgv`, `medi`, …, and HEIDI streaking reduction after them.
7. **Referencing** — reference the susceptibility values (e.g. to the mean).

Not every inversion runs every stage: total-field methods (TFI, QSMART, AutoQSM,
NeXtQSM, multi-echo TGV) skip background field removal,
and the wrapped-phase methods (iQSM, iQSM+, single-echo TGV) skip unwrapping and
echo combination too. See [Algorithms](/QSMxT/reference/methods/#dipole-inversion).
