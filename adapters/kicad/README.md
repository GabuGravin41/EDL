# EDL ↔ KiCad 9 Bridge Adapter

A thin, zero-dependency Python 3 bridge connecting KiCad 9 schematics to `.edl` source-of-truth files via `EDL_ID` properties and `edl sync`.

## Requirements
- Built `edl` binary (`cargo build --release`, or set `EDL_BIN=/path/to/edl`)
- Python 3.8+
- Optional: `kicad-cli` (KiCad 9) for live target-side ERC ingestion

## Standalone CLI Usage

```bash
# 1. Pull: compile .edl -> ERC-clean .kicad_sch tagged with EDL_ID
python3 adapters/kicad/edl_kicad_bridge.py pull examples/mcu_board_3v3.edl out/board.kicad_sch

# 2. Edit out/board.kicad_sch visually in KiCad (drag components, change values/footprints)

# 3. Inspect a verified dry-run diff before touching .edl
python3 adapters/kicad/edl_kicad_bridge.py status examples/mcu_board_3v3.edl out/board.kicad_sch

# 4. Push verified edits back into .edl (comments, formatting, and modules preserved!)
python3 adapters/kicad/edl_kicad_bridge.py push examples/mcu_board_3v3.edl out/board.kicad_sch

# Or run continuous two-way sync + live browser preview:
python3 adapters/kicad/edl_kicad_bridge.py watch examples/mcu_board_3v3.edl out/board.kicad_sch --port 8765
```

## Installing Inside KiCad
Copy `adapters/kicad/edl_kicad_bridge.py` into your KiCad `scripting/plugins/` directory (with `edl` on your `PATH`), restart KiCad, and run **Tools → External Plugins → EDL Bidirectional Sync** on any project with a sibling `<project>.edl` file.
