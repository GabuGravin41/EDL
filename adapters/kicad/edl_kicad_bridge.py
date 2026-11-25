#!/usr/bin/env python3
"""EDL <-> KiCad 9 Thin Bidirectional Adapter & ActionPlugin.

Works in two modes:
  1. Standalone CLI (zero external dependencies, Python 3.8+):
       python3 edl_kicad_bridge.py pull   design.edl out.kicad_sch [--erc]
       python3 edl_kicad_bridge.py status design.edl out.kicad_sch
       python3 edl_kicad_bridge.py push   design.edl out.kicad_sch
       python3 edl_kicad_bridge.py watch  design.edl out.kicad_sch [--port 8765]

  2. Inside KiCad (Tools -> External Plugins -> "EDL Sync"):
     Detects sibling `.edl` file alongside the open project and lets the engineer
     pull EDL updates or push visual edits back into `.edl` via `edl sync --apply`.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


def find_edl_binary(explicit: Optional[str] = None) -> str:
    """Locate the `edl` compiler binary (explicit flag -> $EDL_BIN -> PATH -> repo target/release)."""
    if explicit:
        return explicit
    env_bin = os.environ.get("EDL_BIN")
    if env_bin and Path(env_bin).is_file():
        return env_bin
    on_path = shutil.which("edl")
    if on_path:
        return on_path
    repo_root = Path(__file__).resolve().parents[2]
    for rel in ("target/release/edl", "target/debug/edl"):
        cand = repo_root / rel
        if cand.is_file():
            return str(cand)
    raise FileNotFoundError(
        "Could not locate `edl` binary. Build with `cargo build --release` or set EDL_BIN."
    )


def run_edl(args: List[str], edl_bin: Optional[str] = None) -> subprocess.CompletedProcess[str]:
    exe = find_edl_binary(edl_bin)
    return subprocess.run([exe, *args], text=True, capture_output=True)


def pull_edl_to_kicad(
    edl_file: Path,
    kicad_sch: Path,
    run_erc: bool = True,
    edl_bin: Optional[str] = None,
) -> Dict[str, Any]:
    """Compile `.edl`, export `.kicad_sch` tagged with EDL_IDs, and optionally verify via KiCad CLI ERC."""
    kicad_sch.parent.mkdir(parents=True, exist_ok=True)
    exp = run_edl(["export", str(edl_file), "--kicad", str(kicad_sch)], edl_bin=edl_bin)
    if exp.returncode != 0:
        raise RuntimeError(f"edl export failed:\n{exp.stderr or exp.stdout}")

    build_args = ["build", str(edl_file), "--json"]
    if run_erc:
        build_args.append("--kicad-erc")
    bld = run_edl(build_args, edl_bin=edl_bin)
    if bld.returncode != 0:
        raise RuntimeError(f"edl build failed:\n{bld.stderr or bld.stdout}")

    ir = json.loads(bld.stdout)
    return {
        "action": "pull",
        "edl_file": str(edl_file),
        "kicad_sch": str(kicad_sch),
        "project": ir["project"]["name"],
        "components": ir["project"]["component_count"],
        "nets": ir["project"]["net_count"],
        "has_errors": ir["project"]["has_errors"],
        "evidence": [
            {"check_id": e["check_id"], "status": e["status"], "details": e["details"]}
            for e in ir.get("evidence", [])
        ],
    }


def sync_kicad_to_edl(
    edl_file: Path,
    kicad_sch: Path,
    apply: bool = False,
    edl_bin: Optional[str] = None,
) -> Tuple[int, Dict[str, Any]]:
    """Diff human edits in `.kicad_sch` against `.edl` and optionally apply verified patches."""
    args = ["sync", str(edl_file), "--from-kicad", str(kicad_sch), "--json"]
    if apply:
        args.append("--apply")
    proc = run_edl(args, edl_bin=edl_bin)
    if proc.returncode not in (0, 2):
        raise RuntimeError(f"edl sync failed (exit {proc.returncode}):\n{proc.stderr or proc.stdout}")
    return proc.returncode, json.loads(proc.stdout)


def launch_watch(
    edl_file: Path,
    kicad_sch: Path,
    port: int = 8765,
    edl_bin: Optional[str] = None,
) -> int:
    exe = find_edl_binary(edl_bin)
    return subprocess.call(
        [exe, "watch", str(edl_file), "--port", str(port), "--kicad", str(kicad_sch)]
    )


# ---------------------------------------------------------------------------
# Optional KiCad pcbnew ActionPlugin registration (only active inside KiCad)
# ---------------------------------------------------------------------------
try:
    import pcbnew  # type: ignore
    import wx  # type: ignore

    class EDLSyncActionPlugin(pcbnew.ActionPlugin):
        def defaults(self) -> None:
            self.name = "EDL Bidirectional Sync"
            self.category = "Engineering Description Language"
            self.description = "Pull EDL schematic changes or sync visual KiCad edits back to .edl"
            self.show_toolbar_button = True

        def Run(self) -> None:
            board = pcbnew.GetBoard()
            board_path = Path(board.GetFileName()) if board and board.GetFileName() else None
            if not board_path:
                wx.MessageBox("Save the KiCad project first so EDL can locate the sibling .edl file.", "EDL Sync")
                return
            sch_path = board_path.with_suffix(".kicad_sch")
            edl_path = board_path.with_suffix(".edl")
            if not edl_path.exists():
                wx.MessageBox(f"No sibling EDL file found at:\n{edl_path}", "EDL Sync")
                return
            try:
                _, data = sync_kicad_to_edl(edl_path, sch_path, apply=False)
                plan = data["plan"]
                patches = [a for a in plan["actions"] if a["status"] == "patch"]
                msg = (
                    f"EDL Sync ({edl_path.name} <- {sch_path.name})\n"
                    f"Compared: {plan['entities_compared']} | Patchable changes: {len(patches)}\n\n"
                    + "\n".join(f"• [{a['status']}] {a['description']}" for a in plan["actions"])
                )
                if not patches:
                    wx.MessageBox(msg + "\n\nNo patchable changes detected.", "EDL Sync")
                    return
                dlg = wx.MessageDialog(
                    None,
                    msg + "\n\nApply these verified patches to the .edl source?",
                    "EDL Sync",
                    wx.YES_NO | wx.ICON_QUESTION,
                )
                if dlg.ShowModal() == wx.ID_YES:
                    _, applied = sync_kicad_to_edl(edl_path, sch_path, apply=True)
                    wx.MessageBox(applied["result"]["message"], "EDL Sync")
                dlg.Destroy()
            except Exception as exc:
                wx.MessageBox(f"EDL Sync error:\n{exc}", "EDL Sync", wx.OK | wx.ICON_ERROR)

    if wx.GetApp() is not None:
        EDLSyncActionPlugin().register()
except Exception:
    pass


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="EDL <-> KiCad 9 bidirectional bridge")
    parser.add_argument("--edl-bin", default=None, help="Explicit path to `edl` binary")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_pull = sub.add_parser("pull", help="Compile .edl and export .kicad_sch")
    p_pull.add_argument("edl_file", type=Path)
    p_pull.add_argument("kicad_sch", type=Path)
    p_pull.add_argument("--no-erc", action="store_true", help="Skip kicad-cli sch erc verification")

    p_status = sub.add_parser("status", help="Dry-run diff of KiCad edits against .edl")
    p_status.add_argument("edl_file", type=Path)
    p_status.add_argument("kicad_sch", type=Path)

    p_push = sub.add_parser("push", help="Apply verified KiCad edits back into .edl")
    p_push.add_argument("edl_file", type=Path)
    p_push.add_argument("kicad_sch", type=Path)

    p_watch = sub.add_parser("watch", help="Run live two-way sync + browser preview")
    p_watch.add_argument("edl_file", type=Path)
    p_watch.add_argument("kicad_sch", type=Path)
    p_watch.add_argument("--port", type=int, default=8765)

    args = parser.parse_args(argv)
    if args.cmd == "pull":
        res = pull_edl_to_kicad(
            args.edl_file, args.kicad_sch, run_erc=not args.no_erc, edl_bin=args.edl_bin
        )
        print(json.dumps(res, indent=2))
        return 1 if res["has_errors"] else 0
    if args.cmd == "status":
        code, data = sync_kicad_to_edl(args.edl_file, args.kicad_sch, apply=False, edl_bin=args.edl_bin)
        print(json.dumps(data, indent=2))
        return code
    if args.cmd == "push":
        code, data = sync_kicad_to_edl(args.edl_file, args.kicad_sch, apply=True, edl_bin=args.edl_bin)
        print(json.dumps(data, indent=2))
        return code
    if args.cmd == "watch":
        return launch_watch(args.edl_file, args.kicad_sch, port=args.port, edl_bin=args.edl_bin)
    return 1


if __name__ == "__main__":
    sys.exit(main())
