# Embedded fonts

`sans.ttf` and `sans-bold.ttf` are subsets of **Liberation Sans** (Regular and Bold),
carrying only the characters the per-structure figures draw: printable ASCII plus
`± · – — − ² ³ ¹ χ`.

Note that Liberation Sans has **no superscript minus** (U+207B), so `s⁻¹` cannot be
spelled; the figures write `1/s`. `Canvas::text` asserts in debug builds that every
character it draws has a glyph, so a gap like that fails a test rather than rendering
as blank space. Subsetting takes them from ~410 KB each to ~9 KB.

Liberation Sans is licensed under the **SIL Open Font License, Version 1.1**.
Upstream: https://github.com/liberationfonts/liberation-fonts

They are embedded rather than loaded from the system because a figure has to look the
same everywhere it is produced, including in a container with no fonts installed.

Regenerate with:

    pyftsubset LiberationSans-Regular.ttf --output-file=sans.ttf \
      --unicodes="U+0020-007E,U+00B1,U+00B2,U+00B3,U+00B7,U+00B9,U+2013,U+2014,U+2212,U+03C7" \
      --layout-features='' --no-hinting --desubroutinize
