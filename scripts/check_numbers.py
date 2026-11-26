#!/usr/bin/env python3
"""PRD G7 / S1: every experiment number in the submission documents matches research/results/.

A number taken from a result file is written with a hidden marker right after it:

    φ는 0.785<!--n:econ.json:phi.trueup_10:.3f--> ...      (value 0.785 at key phi.trueup_10)
    손실 확률 0.035%<!--n:econ.json:checks.F2.senior_loss.drop460:.3f-->

`file` is relative to research/results, `path` is dotted (list indices as numbers), `fmt` is a Python
format spec applied to the value (optional suffix `*100` multiplies first: `.1f*100`). The checker fails
when a marker's number differs from the formatted value or the key does not exist, and when a line
reports an outside fact with a number (밝혔·보도했·발표했·집계·추정했·따르면·기록했) without a source link
or reference number.

Usage: python3 scripts/check_numbers.py [FILES...]   (default: README.md, docs/*.md except PRD)
"""
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RES = os.path.join(ROOT, "research", "results")
MARK = re.compile(r"([−-]?[\d,]*\.?\d+)\s*(%|%p|배|건|개)?<!--n:([\w./-]+):([\w.\-]+)(?::([^>]*?))?-->")
DEFAULT = ["README.md", *sorted(p for p in glob.glob("docs/*.md") if not p.endswith(("PRD.md", "deployments.md")))]
REPORTED = re.compile(r"(밝혔|보도했|보도됐|발표했|집계|추정했|따르면|기록했)")
SOURCE = re.compile(r"\[\[?\d+\]?\]|\]\(https?://|https?://|research/results|<!--n:")
_cache = {}


def value(file, path):
    if file not in _cache:
        _cache[file] = json.load(open(os.path.join(RES, file)))
    v = _cache[file]
    for k in path.split("."):
        v = v[int(k)] if isinstance(v, list) else v[k]
    return v


def fmt(v, spec):
    spec = spec or "g"
    scale = 1.0
    if spec.endswith("*100"):
        spec, scale = spec[:-4], 100.0
    return format(float(v) * scale, spec)


def check(path):
    text = open(os.path.join(ROOT, path), encoding="utf-8").read()
    bad, n = [], 0
    for m in MARK.finditer(text):
        n += 1
        shown, _, file, key, spec = m.groups()
        try:
            want = fmt(value(file, key), spec)
        except (KeyError, IndexError, FileNotFoundError, ValueError, TypeError) as e:
            bad.append(f"{path}: unknown {file}:{key} ({e})")
            continue
        if shown.replace(",", "").replace("−", "-") != want.replace(",", ""):
            line = text.count("\n", 0, m.start()) + 1
            bad.append(f"{path}:{line}: shows {shown}, {file}:{key} is {want}")
    code = False
    for i, line in enumerate(text.split("\n"), 1):
        if line.startswith("```"):
            code = not code
        if not code and REPORTED.search(line) and re.search(r"\d", line) and not SOURCE.search(line):
            bad.append(f"{path}:{i}: outside fact without a source: {line.strip()[:80]}")
    return n, bad


def main(files):
    total, bad = 0, []
    for f in files or DEFAULT:
        if os.path.exists(os.path.join(ROOT, f)):
            n, b = check(f)
            total += n
            bad += b
    for b in bad:
        print("FAIL", b)
    print(f"{total} result numbers checked, {len(bad)} mismatches")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
