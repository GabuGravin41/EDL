#!/usr/bin/env python3
"""End-to-end integration tests for EDL's KiCad 9 and FreeCAD host-tool adapters."""

from __future__ import annotations

import importlib.machinery
import importlib.util
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    assert spec is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    loader.exec_module(mod)
    return mod


kicad_bridge = load_module("edl_kicad_bridge", REPO_ROOT / "adapters/kicad/edl_kicad_bridge.py")
freecad_macro = load_module("edl_freecad_macro", REPO_ROOT / "adapters/freecad/EDL_Import_Sync.FCMacro")


class TestAdapters(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="edl_adapters_test_"))
        shutil.copytree(REPO_ROOT / "examples", self.tmp / "examples")

    def tearDown(self) -> None:
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_kicad_bridge_pull_status_push_roundtrip(self) -> None:
        edl_file = self.tmp / "examples/mcu_board_3v3.edl"
        kicad_file = self.tmp / "board.kicad_sch"

        # 1. Pull EDL -> KiCad (.kicad_sch + real KiCad 9 ERC if installed)
        has_kicad = shutil.which("kicad-cli") is not None
        res = kicad_bridge.pull_edl_to_kicad(edl_file, kicad_file, run_erc=has_kicad)
        self.assertFalse(res["has_errors"])
        self.assertEqual(res["components"], 10)
        self.assertTrue(kicad_file.exists())

        # 2. Simulate visual value edit on R1 inside KiCad
        txt = kicad_file.read_text(encoding="utf-8")
        edited = re.sub(
            r'(\(property "Reference" "R1"[^\n]*\n\s*\(property "Value" ")10kohm"',
            r'\g<1>4.7kohm"',
            txt,
            count=1,
        )
        self.assertNotEqual(txt, edited, "expected R1 Value to be updated in simulated KiCad file")
        kicad_file.write_text(edited, encoding="utf-8")

        # 3. Status dry-run detects 1 patch without touching .edl
        code, status = kicad_bridge.sync_kicad_to_edl(edl_file, kicad_file, apply=False)
        self.assertEqual(code, 0)
        self.assertTrue(status["result"]["applied"])
        self.assertFalse(status["result"]["written"])
        self.assertIn("value: 10kohm", edl_file.read_text(encoding="utf-8"))

        # 4. Push applies surgical patch and re-verifies
        code, pushed = kicad_bridge.sync_kicad_to_edl(edl_file, kicad_file, apply=True)
        self.assertEqual(code, 0)
        self.assertTrue(pushed["result"]["written"])
        self.assertIn("value: 4.7kohm", edl_file.read_text(encoding="utf-8"))

    def test_freecad_macro_3d_plan_and_roundtrip_sync(self) -> None:
        edl_file = self.tmp / "examples/apartment_2br_electrical.edl"
        ir = freecad_macro.compile_edl_ir(edl_file)
        plan = freecad_macro.build_3d_plan(ir)

        self.assertEqual(plan["project"], "Apartment_2BR_With_Electrical")
        self.assertEqual(plan["ceiling_height_mm"], 2700.0)
        self.assertEqual(plan["groups"]["Structure"], 1)
        self.assertGreater(plan["groups"]["Walls"], 0)
        self.assertEqual(plan["groups"]["Fixtures"], 9)
        self.assertEqual(plan["groups"]["Electrical"], 18)

        # Every solid has positive dimensions and a non-empty EDL_ID
        for s in plan["solids"]:
            self.assertTrue(s["edl_id"])
            self.assertGreater(s["dx"], 0.0)
            self.assertGreater(s["dy"], 0.0)
            self.assertGreater(s["dz"], 0.0)

        # Simulate moving DiningTable by (+100mm, +100mm) in FreeCAD and syncing back to .edl
        dt = next(s for s in plan["solids"] if s["edl_id"] == "DiningTable")
        old_cx = dt["x"] + dt["dx"] * 0.5
        old_cy = dt["y"] + dt["dy"] * 0.5
        synced = freecad_macro.sync_freecad_centers_to_edl(
            edl_file, {"DiningTable": (old_cx + 100.0, old_cy + 100.0)}, apply=True
        )
        self.assertTrue(synced["result"]["written"], synced)
        self.assertIn("at: (2700mm, 1500mm)", edl_file.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
