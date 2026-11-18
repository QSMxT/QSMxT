#!/usr/bin/env python3
"""Summarises how long each workspace crate took to compile and each test binary took to run.

Reads what the CI test run leaves under the target directory:
- cargo's `--timings` report (`cargo-timings/cargo-timing.html`): each compiled unit's time;
- nextest's JUnit report (`nextest/default/junit.xml`, see `.config/nextest.toml`): each
  test's time, by test binary.

Prints both as tables, to the job summary as well when run in GitHub Actions, and lists the tests
over 10 s (the slow tail to fix or mark `#[ignore = "slow: ..."]`). Report only: always exits 0.

    python3 .github/scripts/test_timings.py [target-dir]
"""

import json
import os
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

WORKSPACE = {"qsmxt", "qsmxt-engine", "qsmxt-io"}
SLOW_S = 10.0


def newest(root, name):
    found = sorted(root.rglob(name), key=lambda p: p.stat().st_mtime)
    return found[-1] if found else None


def compile_table(root):
    path = newest(root, "cargo-timing.html")
    if not path:
        return ["No cargo timings report found (was the run given `--timings`?)."]
    m = re.search(r"const UNIT_DATA = (\[.*?\]);\n", path.read_text(encoding="utf-8"), re.S)
    if not m:
        return [f"{path}: no unit data."]
    units = json.loads(m.group(1))
    ours = sorted((u for u in units if u["name"] in WORKSPACE), key=lambda u: -u["duration"])
    deps = sum(u["duration"] for u in units if u["name"] not in WORKSPACE)
    lines = ["### Compile time", "", "| Unit | Seconds |", "|---|---:|"]
    for u in ours:
        lines.append(f"| {u['name']}{u['target']} | {u['duration']:.1f} |")
    lines.append(f"| dependencies ({len(units) - len(ours)} units, total) | {deps:.1f} |")
    return lines


def test_table(root):
    path = newest(root, "junit.xml")
    if not path:
        return ["No nextest JUnit report found (see .config/nextest.toml)."], []
    rows, slow = [], []
    for suite in ET.parse(path).getroot().iter("testsuite"):
        times = []
        for case in suite.iter("testcase"):
            if case.find("skipped") is not None:
                continue
            t = float(case.get("time") or 0)
            times.append((t, case.get("name")))
            if t > SLOW_S:
                slow.append((t, suite.get("name"), case.get("name")))
        if times:
            top = max(times)
            rows.append((sum(t for t, _ in times), suite.get("name"), len(times), top))
    lines = ["### Test time by binary", "",
             "| Test binary | Tests | Sum of test times (s) | Slowest test (s) |", "|---|---:|---:|---|"]
    for total, name, n, (t, case) in sorted(rows, reverse=True):
        lines.append(f"| {name} | {n} | {total:.1f} | {t:.1f} `{case}` |")
    return lines, sorted(slow, reverse=True)


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target")
    lines = compile_table(root) + [""]
    tests, slow = test_table(root)
    lines += tests
    if slow:
        lines += ["", f"### Tests over {SLOW_S:.0f} s", ""]
        lines += [f"- {t:.1f} s `{suite}` `{case}`" for t, suite, case in slow]
    text = "\n".join(lines) + "\n"
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
