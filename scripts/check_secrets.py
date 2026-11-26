#!/usr/bin/env python3
"""Fails if a tracked file looks like a private key or wallet (PRD G9)."""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
PATTERNS = [
    (re.compile(r"\[\s*(?:\d{1,3}\s*,\s*){63}\d{1,3}\s*\]"), "64-byte keypair array"),
    (re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"), "PEM private key"),
    (re.compile(r"(?i)\b(?:mnemonic|seed phrase)\s*[:=]"), "seed phrase"),
]


def main() -> int:
    files = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.split()
    bad = []
    for f in files:
        p = ROOT / f
        if f.startswith("keys/") or f.endswith("-keypair.json") or f.endswith(".keypair.json"):
            bad.append((f, "key file path"))
            continue
        try:
            text = p.read_text(errors="ignore")
        except (IsADirectoryError, FileNotFoundError):
            continue
        for rx, what in PATTERNS:
            if rx.search(text):
                bad.append((f, what))
    for f, what in bad:
        print(f"FAIL {f}: {what}")
    print(f"scanned {len(files)} files, {len(bad)} findings")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
