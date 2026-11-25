#!/usr/bin/env python3
"""Batch-import and 100%-netlist-verify all official KiCad 9 demo projects into Option-A EDL."""

import argparse
import glob
import os
import subprocess
import sys


def main():
    ap = argparse.ArgumentParser(description="Convert & verify KiCad 9 demo corpus into EDL.")
    ap.add_argument("--demos-dir", default="/usr/share/kicad/demos", help="Root KiCad demos directory")
    ap.add_argument("--out-dir", default="out/kicad_corpus_library", help="Output directory for converted EDL demos")
    ap.add_argument("--edl-bin", default="./target/debug/edl", help="Path to compiled edl binary")
    args = ap.parse_args()

    demos = sorted(glob.glob(os.path.join(args.demos_dir, "**", "*.kicad_pro"), recursive=True))
    if not demos:
        print(f"No .kicad_pro files found under {args.demos_dir}", file=sys.stderr)
        sys.exit(1)

    passed, failed = [], []
    for pro in demos:
        d = os.path.dirname(pro)
        stem = os.path.splitext(os.path.basename(pro))[0]
        root_sch = os.path.join(d, stem + ".kicad_sch")
        if not os.path.isfile(root_sch):
            schs = sorted(glob.glob(os.path.join(d, "*.kicad_sch")))
            if not schs:
                continue
            root_sch = schs[0]
        rel = os.path.relpath(root_sch, args.demos_dir)
        target_dir = os.path.join(args.out_dir, stem.replace(" ", "_"))
        res = subprocess.run(
            [args.edl_bin, "import-kicad", root_sch, "--out-dir", target_dir, "--verify"],
            capture_output=True,
            text=True,
            timeout=90,
        )
        if res.returncode == 0:
            match_line = next((l for l in res.stdout.splitlines() if "100% EXACT MATCH" in l), "OK")
            passed.append((rel, match_line))
        else:
            err = (res.stderr.strip() or res.stdout.strip()).splitlines()
            failed.append((rel, " | ".join(err[:3]) if err else "unknown error"))

    total = len(passed) + len(failed)
    print(f"\n=== KICAD 9 CORPUS SCORECARD: {len(passed)}/{total} (100% Exact Netlist Round-Trip) ===")
    for rel, info in passed:
        print(f"  [PASS] {rel:58s} {info}")
    if failed:
        print(f"\n--- FAILURES ({len(failed)}) ---")
        for rel, msg in failed:
            print(f"  [FAIL] {rel:58s} {msg[:180]}")
        sys.exit(1)


if __name__ == "__main__":
    main()
