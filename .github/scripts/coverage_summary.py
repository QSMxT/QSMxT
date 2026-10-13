#!/usr/bin/env python3
"""Line coverage from a cargo-llvm-cov LCOV report, with test code left out.

    python3 .github/scripts/coverage_summary.py target/lcov.info

Prints `<percent> <covered> <coverable>` (percent to two decimals) on stdout and a per-file
table of the least-covered files on stderr.

Why this exists: cargo-tarpaulin, which this replaced, leaves test code out of the
denominator (its default `--ignore-tests`). LLVM source-based coverage cannot: an inline
`#[cfg(test)] mod tests { ... }` lives in the same file as the code it tests, and
`#[coverage(off)]` is nightly-only. Counting tests would inflate the figure, since a test
body runs almost by definition. So every item carrying `#[cfg(test)]` is found in the
source and its lines are dropped from the LCOV line records: inline modules, helper
functions, statics, `use` lines, and out-of-line `#[cfg(test)] mod foo;` files (dropped
whole). What remains is plain LLVM line coverage of non-test code.
"""

import os
import re
import sys

CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]\s*$")
OUT_OF_LINE_MOD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;")


def _strip_code(text):
    """Blank out comments and string/char literals, keeping newlines, so braces and
    semicolons left in the result are real syntax."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]) and not (i and (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = len(re.match(r"r(#*)\"", text[i:]).group(1))
            end = text.find('"' + "#" * hashes, i + 2 + hashes)
            j = n if end < 0 else end + 1 + hashes
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j += 1
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "'":
            # A char literal ('x', '\n', '\u{..}') or a lifetime ('a). Only the former is blanked.
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\.|[^\\'])'", text[i:], re.S)
            if m:
                out.append(" " * len(m.group(0)))
                i += len(m.group(0))
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def test_lines(path):
    """1-based line numbers of `#[cfg(test)]` items in `path`, and any out-of-line test
    module files they declare."""
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return set(), []
    raw_lines = text.split("\n")
    code = _strip_code(text).split("\n")
    lines, extra_files = set(), []
    for idx, line in enumerate(raw_lines):
        if not CFG_TEST.match(code[idx] if idx < len(code) else ""):
            continue
        # The item starts after any further attributes or doc comments.
        start = idx + 1
        while start < len(code) and (code[start].strip().startswith("#[") or not code[start].strip()):
            start += 1
        if start >= len(code):
            continue
        m = OUT_OF_LINE_MOD.match(code[start])
        if m:
            base = os.path.dirname(path)
            stem = os.path.splitext(os.path.basename(path))[0]
            if stem not in ("lib", "main", "mod"):
                base = os.path.join(base, stem)
            extra_files += [os.path.join(base, m.group(1) + ".rs"),
                            os.path.join(base, m.group(1), "mod.rs")]
            lines.update(range(idx + 1, start + 2))
            continue
        # Walk until a `;` at depth 0 before any brace, or the brace that closes the first one.
        depth, end, seen_brace = 0, None, False
        for j in range(start, len(code)):
            for ch in code[j]:
                if ch == "{":
                    depth += 1
                    seen_brace = True
                elif ch == "}":
                    depth -= 1
                    if seen_brace and depth == 0:
                        end = j
                        break
                elif ch == ";" and depth == 0 and not seen_brace:
                    end = j
                    break
            if end is not None:
                break
        if end is None:
            end = len(code) - 1
        lines.update(range(idx + 1, end + 2))
    return lines, extra_files


def main(lcov_path):
    records = {}  # file -> {line: hits}
    current = None
    with open(lcov_path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("SF:"):
                current = records.setdefault(line[3:], {})
            elif line.startswith("DA:") and current is not None:
                ln, hits = line[3:].split(",")[:2]
                current[int(ln)] = max(current.get(int(ln), 0), int(hits))
            elif line == "end_of_record":
                current = None

    excluded_whole = set()
    per_file_tests = {}
    for path in records:
        lines, extra = test_lines(path)
        per_file_tests[path] = lines
        excluded_whole.update(os.path.normpath(p) for p in extra)

    covered = coverable = 0
    rows = []
    for path, das in records.items():
        if os.path.normpath(path) in excluded_whole:
            continue
        skip = per_file_tests[path]
        kept = {ln: h for ln, h in das.items() if ln not in skip}
        c = sum(1 for h in kept.values() if h > 0)
        covered += c
        coverable += len(kept)
        if kept:
            rows.append((c / len(kept) * 100, c, len(kept), path))

    pct = covered / coverable * 100 if coverable else 0.0
    cwd = os.getcwd() + os.sep
    print("Least-covered files (non-test lines):", file=sys.stderr)
    for p, c, t, path in sorted(rows)[:15]:
        print(f"  {p:6.2f}%  {c:>5}/{t:<5} {path.replace(cwd, '')}", file=sys.stderr)
    print(f"{pct:.2f}% coverage, {covered}/{coverable} lines covered (test code excluded)", file=sys.stderr)
    print(f"{pct:.2f} {covered} {coverable}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
