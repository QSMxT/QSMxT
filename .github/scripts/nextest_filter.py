#!/usr/bin/env python3
"""Prints the nextest filterset CI's test run uses with `--run-ignored all`.

Two kinds of test are `#[ignore]`d:

- slow ones, `#[ignore = "slow: ..."]`: too slow to wait for locally, but part of the suite.
  CI runs them.
- the rest: tools rather than tests (BIDS validator fixtures, figure previews, timings), or
  tests that need downloaded model weights. CI does not run them in the main suite.

nextest cannot filter on why a test is ignored, so this reads the attributes from the sources
and excludes every ignored test that is not a slow one, by name.
"""

import pathlib
import re
import sys

IGNORE = re.compile(r'#\[ignore(?:\s*=\s*"([^"]*)")?\s*\]')
FN = re.compile(r'\bfn\s+(\w+)')


def ignored_tests(root):
    """Yields (name, is_slow) for every `#[ignore]`d fn under root."""
    for path in sorted(root.rglob("*.rs")):
        if "target" in path.parts:
            continue
        pending = None
        for line in path.read_text(encoding="utf-8").splitlines():
            m = IGNORE.search(line)
            if m:
                pending = (m.group(1) or "").startswith("slow:")
                continue
            if pending is not None:
                f = FN.search(line)
                if f:
                    yield f.group(1), pending
                    pending = None


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    tests = list(ignored_tests(root))
    slow = sorted(n for n, s in tests if s)
    skip = sorted({n for n, s in tests if not s})
    print(f"slow (run): {', '.join(slow) or 'none'}", file=sys.stderr)
    print(f"ignored (not run): {', '.join(skip) or 'none'}", file=sys.stderr)
    if not skip:
        print("all()")
    else:
        print("not test(/(^|::)(" + "|".join(skip) + ")$/)")


if __name__ == "__main__":
    main()
