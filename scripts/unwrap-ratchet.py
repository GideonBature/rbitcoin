#!/usr/bin/env python3
"""Per-file ceiling for bare production unwraps.

Counts `.unwrap(` and `.unwrap_err(` under crates/. Skips rbitcoin-test,
rbitcoin-bench, tests/, benches/, *tests.rs, and #[cfg(test)] modules.
Does not count unwrap_or, lock/read/write poison unwraps, or try_into().unwrap().

lint/unwrap-baseline.txt is sorted `path<TAB>count` lines for counts above zero.
A file above its line, a positive file with no line, or a line that is now
lower (or whose file is gone) fails. --write rewrites the file from the tree.
"""

import argparse
import os
import re
import sys

UNWRAP_RE = re.compile(r"\.unwrap(?:_err)?\s*\(")
LOCK_RE = re.compile(r"\.(?:lock|read|write)\s*\(\s*\)\s*\.unwrap")
SKIP_CRATES = {"rbitcoin-test", "rbitcoin-bench"}


def strip_code(line: str) -> str:
    if line.lstrip().startswith("//"):
        return ""
    return re.sub(r"//.*", "", line)


def counts_as_bare(code: str, prev: str) -> bool:
    if not UNWRAP_RE.search(code):
        return False
    if "try_into" in code or "try_into" in prev:
        return False
    if LOCK_RE.search(code) or LOCK_RE.search(prev + code):
        return False
    if re.search(r"\.(?:lock|read|write)\s*\(\s*\)\s*$", prev) and ".unwrap" in code:
        return False
    return True


def file_count(path: str) -> int:
    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    depth = 0
    test_stack: list[int] = []
    pending = False
    prev = ""
    n = 0
    for line in lines:
        code = strip_code(line)
        if "cfg(test)" in code:
            pending = True
        if not test_stack and counts_as_bare(code, prev):
            n += 1
        opens = code.count("{")
        closes = code.count("}")
        if pending and "{" in code:
            test_stack.append(depth)
            pending = False
        elif pending and code.strip() and not code.strip().startswith("#"):
            if not any(k in code for k in ("mod ", "fn ", "impl ", "struct ")):
                pending = False
        depth += opens - closes
        while test_stack and depth <= test_stack[-1]:
            test_stack.pop()
        if depth < 0:
            depth = 0
        prev = code.rstrip()
    return n


def scan(root: str) -> dict[str, int]:
    crates = os.path.join(root, "crates")
    counts: dict[str, int] = {}
    for dirpath, dirnames, filenames in os.walk(crates):
        dirnames[:] = [d for d in dirnames if d not in ("target",)]
        rel_dir = os.path.relpath(dirpath, root).replace(os.sep, "/")
        parts = rel_dir.split("/")
        if "tests" in parts or "benches" in parts:
            continue
        if len(parts) > 1 and parts[1] in SKIP_CRATES:
            continue
        for name in filenames:
            if not name.endswith(".rs") or name.endswith("tests.rs"):
                continue
            rel = f"{rel_dir}/{name}"
            n = file_count(os.path.join(dirpath, name))
            if n:
                counts[rel] = n
    return counts


def read_baseline(path: str) -> dict[str, int]:
    if not os.path.exists(path):
        sys.exit(f"missing baseline {path}")
    out: dict[str, int] = {}
    for raw in open(path, encoding="utf-8"):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        path_s, count_s = line.split("\t")
        out[path_s] = int(count_s)
    return out


def write_baseline(path: str, counts: dict[str, int]) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    lines = [f"{p}\t{counts[p]}\n" for p in sorted(counts)]
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def check(counts: dict[str, int], base: dict[str, int]) -> list[str]:
    errors = []
    for path in sorted(set(counts) | set(base)):
        got = counts.get(path, 0)
        if path not in base:
            errors.append(f"{path}: {got} unwraps, baseline has no line")
            continue
        want = base[path]
        if got > want:
            errors.append(f"{path}: {got} unwraps, baseline {want}")
        elif got < want:
            errors.append(f"{path}: {got} unwraps, baseline {want} (lower the line)")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=None)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    root = os.path.abspath(args.root or os.path.join(os.path.dirname(__file__), ".."))
    baseline = os.path.join(root, "lint", "unwrap-baseline.txt")
    counts = scan(root)
    if args.write:
        write_baseline(baseline, counts)
        print(f"wrote {len(counts)} lines to {baseline}")
        return 0
    errors = check(counts, read_baseline(baseline))
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
