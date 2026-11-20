#!/usr/bin/env python3
"""Reports Rust files and functions over the size budgets.

Budgets:
- a file's non-test code: 800 lines (600 is a warning);
- a test file (under `tests/`, or `tests.rs` / `*_tests.rs`), or a file's inline test modules:
  1,500 lines;
- a function: 120 lines.

What is over budget today is listed in `.github/size-budget.json` with its current size as a
ceiling. A ceiling can only go down: a listed file or function that grows past it, or a new one
over budget, is reported as a breach. Lines are counted as in the file, comments and blank lines
included.

Exits 0 unless `--enforce` is given; CI gives it.

    python3 .github/scripts/size_budget.py              # report
    python3 .github/scripts/size_budget.py --enforce    # exit 1 on a breach
    python3 .github/scripts/size_budget.py --write-baseline   # set ceilings to today's sizes
"""

import json
import pathlib
import re
import sys

FILE_BUDGET = 800
FILE_WARN = 600
TEST_FILE_BUDGET = 1500
FN_BUDGET = 120

ROOTS = ["src", "tests", "crates"]
BASELINE = pathlib.Path(".github/size-budget.json")

FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
CFG_TEST = re.compile(r"#\[cfg\(test\)\]")


def code_mask(text):
    """A copy of `text` with comments and string/char literals blanked (newlines kept), so braces
    and keywords can be found by position."""
    out = list(text)
    i, n = 0, len(text)

    def blank(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
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
            blank(i, j)
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:i + 10]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = len(re.match(r'r(#*)"', text[i:]).group(1))
            end = text.find('"' + "#" * hashes, i + 2 + hashes)
            end = n if end < 0 else end + 1 + hashes
            blank(i, end)
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            blank(i, j + 1)
            i = j + 1
        elif c == "'":
            # A char literal ('x', '\n', '\u{..}'), not a lifetime ('a).
            m = re.match(r"'(\\.[^']*|[^'\\])'", text[i:i + 12])
            if m:
                blank(i, i + m.end())
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def matching_brace(code, open_at):
    depth = 0
    for j in range(open_at, len(code)):
        if code[j] == "{":
            depth += 1
        elif code[j] == "}":
            depth -= 1
            if depth == 0:
                return j
    return len(code) - 1


def line_of(code, pos):
    return code.count("\n", 0, pos) + 1


def test_ranges(code):
    """Line ranges (first, last) of `#[cfg(test)]` items with a body (inline test modules)."""
    ranges = []
    for m in CFG_TEST.finditer(code):
        brace = code.find("{", m.end())
        semi = code.find(";", m.end())
        if brace < 0 or (0 <= semi < brace):
            continue
        ranges.append((line_of(code, m.start()), line_of(code, matching_brace(code, brace))))
    return ranges


def functions(code):
    """(name, first line, length in lines) of every function with a body."""
    out = []
    for m in FN.finditer(code):
        # The body is the first `{` at bracket depth 0 after the signature; a `;` first means a
        # declaration without one (a trait method).
        depth, j = 0, m.end()
        while j < len(code):
            ch = code[j]
            if ch in "(<[":
                depth += 1
            elif ch in ")>]":
                depth -= 1
            elif ch == ";" and depth <= 0:
                j = -1
                break
            elif ch == "{" and depth <= 0:
                break
            j += 1
        if j < 0 or j >= len(code):
            continue
        # `->` in a signature is not a closing bracket.
        first, last = line_of(code, m.start()), line_of(code, matching_brace(code, j))
        out.append((m.group(1), first, last - first + 1))
    return out


def is_test_file(path):
    return "tests" in path.parts or path.name == "tests.rs" or path.name.endswith("_tests.rs")


def measure():
    files, fns = {}, {}
    for root in ROOTS:
        for path in sorted(pathlib.Path(root).rglob("*.rs")):
            if "target" in path.parts or "fixtures" in path.parts:
                continue
            text = path.read_text(encoding="utf-8")
            code = code_mask(text.replace("->", "  "))
            total = text.count("\n") + (0 if text.endswith("\n") else 1)
            ranges = [] if is_test_file(path) else test_ranges(code)
            test_lines = sum(b - a + 1 for a, b in ranges)
            key = path.as_posix()
            if is_test_file(path):
                files[key] = {"test": total}
            else:
                files[key] = {"code": total - test_lines, "test": test_lines}
            for name, first, length in functions(code):
                if length > FN_BUDGET:
                    in_test = is_test_file(path) or any(a <= first <= b for a, b in ranges)
                    fns[f"{key}::{name}@{first}"] = {"lines": length, "test": in_test, "id": f"{key}::{name}"}
    return files, fns


def over(files, fns):
    """What is over budget now: {file: lines} for code, {file: lines} for tests, {fn id: lines}."""
    code = {f: v["code"] for f, v in files.items() if v.get("code", 0) > FILE_BUDGET}
    tests = {f: v["test"] for f, v in files.items() if v.get("test", 0) > TEST_FILE_BUDGET}
    funcs = {}
    for v in fns.values():
        # Functions are keyed by file and name (not line), so a ceiling survives edits above it;
        # the longest of a name's overloads counts.
        funcs[v["id"]] = max(funcs.get(v["id"], 0), v["lines"])
    return code, tests, funcs


def main():
    args = set(sys.argv[1:])
    files, fns = measure()
    code, tests, funcs = over(files, fns)
    if "--write-baseline" in args:
        BASELINE.write_text(json.dumps({"files": code, "test_files": tests, "functions": funcs},
                                       indent=1, sort_keys=True) + "\n")
        print(f"wrote {BASELINE}: {len(code)} files, {len(tests)} test files, {len(funcs)} functions")
        return 0
    base = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    breaches = []

    def section(title, now, ceilings, budget):
        print(f"\n{title} (budget {budget} lines): {len(now)} over")
        for k, v in sorted(now.items(), key=lambda kv: -kv[1]):
            ceiling = ceilings.get(k)
            if ceiling is None:
                note = "NEW: over budget"
                breaches.append(f"{k}: {v} lines, over the {budget}-line budget")
            elif v > ceiling:
                note = f"GREW past its ceiling {ceiling}"
                breaches.append(f"{k}: {v} lines, past its ceiling of {ceiling}")
            elif v < ceiling:
                note = f"ceiling {ceiling}: lower it to {v}"
            else:
                note = "at ceiling"
            print(f"  {v:>6}  {k}  ({note})")
        gone = sorted(set(ceilings) - set(now))
        for k in gone:
            print(f"  within budget now, drop its ceiling: {k}")

    section("Files, non-test code", code, base.get("files", {}), FILE_BUDGET)
    warn = sorted((v["code"], f) for f, v in files.items() if FILE_WARN < v.get("code", 0) <= FILE_BUDGET)
    if warn:
        print(f"\nFiles nearing the budget ({FILE_WARN}+ lines): {len(warn)}")
        for v, f in reversed(warn):
            print(f"  {v:>6}  {f}")
    section("Test code per file", tests, base.get("test_files", {}), TEST_FILE_BUDGET)
    section("Functions", funcs, base.get("functions", {}), FN_BUDGET)

    total_code = sum(v.get("code", 0) for v in files.values())
    total_test = sum(v.get("test", 0) for v in files.values())
    print(f"\nTotal: {total_code} lines of code, {total_test} lines of tests, in {len(files)} files")
    if breaches:
        print(f"\n{len(breaches)} breach(es):")
        for b in breaches:
            print(f"  {b}")
            if "GITHUB_ACTIONS" in __import__("os").environ:
                print(f"::warning::size budget: {b}")
        return 1 if "--enforce" in args else 0
    print("\nNo breaches.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
