#!/usr/bin/env python3
"""
Hospital 11kV / 400V Dual-Transformer + Standby Generator SLD Generator
Standards: IEC 60909-0 (Short-Circuit), IEC 60364-5-52 (Cable Sizing & dV), IEC 60617 (SLD Symbols)

Generates from a single verified engineering model:
  1. sld_manifest.json
  2. hospital_sld.svg
  3. hospital_sld.dxf (valid layered AutoCAD R12 / AC1009 DXF)
"""
import json
import math
import xml.sax.saxutils as sax
from pathlib import Path

OUT_DIR = Path("/home/lindah/projects/EDL/benchmarks/v3_2_industry_ab/b5_hospital_sld/baseline")
OUT_DIR.mkdir(parents=True, exist_ok=True)

# ==============================================================================
# 1. ENGINEERING COMPUTATION ENGINE (IEC 60909-0 & IEC 60364-5-52)
# ==============================================================================
U_MV = 11000.0
U_LV = 400.0
FREQ = 50.0
SK_UTIL_MVA = 250.0
C_MAX_MV = 1.10
C_MAX_LV = 1.05

TX_KVA = 1000.0
TX_UK_PCT = 6.0
TX_UR_PCT = 1.10
TX_UX_PCT = math.sqrt(TX_UK_PCT**2 - TX_UR_PCT**2)

GEN_KVA = 1000.0
GEN_PF = 0.80
GEN_KW = GEN_KVA * GEN_PF
GEN_XD2_PCT = 15.0

# --- Short-Circuit Calculations (IEC 60909-0:2016) ---
ik_mv_ka = (SK_UTIL_MVA * 1e6) / (math.sqrt(3) * U_MV) / 1000.0

# Upstream utility impedance referred to 400 V
z_q = (C_MAX_MV * U_LV**2) / (SK_UTIL_MVA * 1e6)
x_q = 0.995 * z_q
r_q = 0.1 * x_q

# Transformer impedance referred to 400 V with IEC 60909-0:2016 KT correction
r_t = (TX_UR_PCT / 100.0) * (U_LV**2) / (TX_KVA * 1e3)
x_t = (TX_UX_PCT / 100.0) * (U_LV**2) / (TX_KVA * 1e3)
kt = 0.95 * C_MAX_LV / (1.0 + 0.6 * (TX_UX_PCT / 100.0))
r_tk, x_tk = kt * r_t, kt * x_t

# 8 m 2000A Cu sandwich busduct TX -> MSB
r_bd_tx, x_bd_tx = 8.0 * 0.024e-3, 8.0 * 0.018e-3

# Single TX at MSB_A / MSB_B
r_msb_1tx = r_q + r_tk + r_bd_tx
x_msb_1tx = x_q + x_tk + x_bd_tx
z_msb_1tx = math.hypot(r_msb_1tx, x_msb_1tx)
ik_msb_1tx_ka = (C_MAX_LV * U_LV) / (math.sqrt(3) * z_msb_1tx) / 1000.0
kappa_1tx = 1.02 + 0.98 * math.exp(-3.0 * r_msb_1tx / x_msb_1tx)
ip_msb_1tx_ka = kappa_1tx * math.sqrt(2) * ik_msb_1tx_ka

# Hypothetical 2 TX paralleled (prevented by IL_BUS_TIE)
r_msb_2tx = r_q + 0.5 * (r_tk + r_bd_tx)
x_msb_2tx = x_q + 0.5 * (x_tk + x_bd_tx)
z_msb_2tx = math.hypot(r_msb_2tx, x_msb_2tx)
ik_msb_2tx_ka = (C_MAX_LV * U_LV) / (math.sqrt(3) * z_msb_2tx) / 1000.0

# EMSB Normal mode (10 m 1600A Cu busduct MSB_B -> ATS1 -> EMSB)
r_bd_emsb, x_bd_emsb = 10.0 * 0.032e-3, 10.0 * 0.020e-3
r_emsb_norm = r_msb_1tx + r_bd_emsb
x_emsb_norm = x_msb_1tx + x_bd_emsb
z_emsb_norm = math.hypot(r_emsb_norm, x_emsb_norm)
ik_emsb_norm_ka = (C_MAX_LV * U_LV) / (math.sqrt(3) * z_emsb_norm) / 1000.0

# EMSB Standby mode (GEN1 + 15 m 4x(3x240mm2) Cu XLPE)
x_g = (GEN_XD2_PCT / 100.0) * (U_LV**2) / (GEN_KVA * 1e3)
r_g = 0.15 * x_g
sin_phi_g = math.sqrt(1.0 - GEN_PF**2)
kg = C_MAX_LV / (1.0 + (GEN_XD2_PCT / 100.0) * sin_phi_g)
r_gen_cab = 15.0 * 0.0754e-3 / 4.0
x_gen_cab = 15.0 * 0.080e-3 / 4.0
z_emsb_gen = math.hypot(kg * r_g + r_gen_cab, kg * x_g + x_gen_cab)
ik_emsb_gen_ka = (C_MAX_LV * U_LV) / (math.sqrt(3) * z_emsb_gen) / 1000.0

# --- IEC 60364-5-52 Table B.52.12 Cu XLPE 90C (Method E perforated tray, 30C base) ---
K_DERATE = 0.88  # 35C ambient (0.96) x tray grouping (0.92)
CABLE_DB = {
    25:  (127, 0.927, 0.086),
    35:  (158, 0.668, 0.083),
    50:  (192, 0.494, 0.082),
    70:  (246, 0.342, 0.080),
    95:  (298, 0.247, 0.080),
    120: (346, 0.196, 0.079),
    150: (399, 0.159, 0.079),
    185: (456, 0.127, 0.078),
    240: (538, 0.097, 0.078),
    300: (621, 0.077, 0.078),
}

RAW_FEEDERS = [
    # Normal Bus MSB_A
    {"id": "F7_IMAGING",    "q_id": "Q_F7",  "label1": "MRI / X-Ray",    "label2": "Imaging Suite",  "bus": "MSB_A", "load_kw": 180, "pf": 0.90, "length_m": 40, "breaker_a": 400, "icu_ka": 50.0, "runs": 1, "mm2": 240, "fire_rated": False, "shed": None},
    {"id": "F8_CHILLER1",   "q_id": "Q_F8",  "label1": "HVAC",           "label2": "Chiller #1",     "bus": "MSB_A", "load_kw": 220, "pf": 0.85, "length_m": 65, "breaker_a": 500, "icu_ka": 50.0, "runs": 2, "mm2": 150, "fire_rated": False, "shed": "S1"},
    # Normal Bus MSB_B
    {"id": "F9_CHILLER2",   "q_id": "Q_F9",  "label1": "HVAC",           "label2": "Chiller #2",     "bus": "MSB_B", "load_kw": 220, "pf": 0.85, "length_m": 70, "breaker_a": 500, "icu_ka": 50.0, "runs": 2, "mm2": 150, "fire_rated": False, "shed": "S2"},
    {"id": "F10_GEN_PWR",   "q_id": "Q_F10", "label1": "Admin Ltg &",    "label2": "General Power",  "bus": "MSB_B", "load_kw": 100, "pf": 0.90, "length_m": 50, "breaker_a": 200, "icu_ka": 50.0, "runs": 1, "mm2":  95, "fire_rated": False, "shed": "S2"},
    # Essential Bus EMSB
    {"id": "F1_MED_UPS",    "q_id": "Q_F1",  "label1": "Medical Proc &", "label2": "ICU UPS",        "bus": "EMSB",  "load_kw": 150, "pf": 0.90, "length_m": 35, "breaker_a": 320, "icu_ka": 36.0, "runs": 1, "mm2": 185, "fire_rated": False, "shed": None},
    {"id": "F2_FIRE_PUMP",  "q_id": "Q_F2",  "label1": "Fire & Life-",   "label2": "Safety Pump",    "bus": "EMSB",  "load_kw":  75, "pf": 0.85, "length_m": 55, "breaker_a": 200, "icu_ka": 36.0, "runs": 1, "mm2":  95, "fire_rated": True,  "shed": None},
    {"id": "F3_CLINIC_DB1", "q_id": "Q_F3",  "label1": "Outpatient",     "label2": "Clinic Sub-DB1", "bus": "EMSB",  "load_kw": 120, "pf": 0.90, "length_m": 45, "breaker_a": 250, "icu_ka": 36.0, "runs": 1, "mm2": 120, "fire_rated": False, "shed": None},
    {"id": "F4_CLINIC_DB2", "q_id": "Q_F4",  "label1": "Outpatient",     "label2": "Clinic Sub-DB2", "bus": "EMSB",  "load_kw": 110, "pf": 0.90, "length_m": 60, "breaker_a": 250, "icu_ka": 36.0, "runs": 1, "mm2": 120, "fire_rated": False, "shed": None},
    {"id": "F5_LAB_STERIL", "q_id": "Q_F5",  "label1": "Sterilization",  "label2": "& Lab Wing",     "bus": "EMSB",  "load_kw":  85, "pf": 0.88, "length_m": 40, "breaker_a": 200, "icu_ka": 36.0, "runs": 1, "mm2":  70, "fire_rated": False, "shed": None},
    {"id": "F6_ELEVATORS",  "q_id": "Q_F6",  "label1": "Medical",        "label2": "Elevators",      "bus": "EMSB",  "load_kw":  90, "pf": 0.85, "length_m": 50, "breaker_a": 200, "icu_ka": 36.0, "runs": 1, "mm2":  95, "fire_rated": False, "shed": None},
]

feeders = []
bus_pq = {"MSB_A": [0.0, 0.0], "MSB_B": [0.0, 0.0], "EMSB": [0.0, 0.0]}

for f in RAW_FEEDERS:
    p_kw = f["load_kw"]
    pf = f["pf"]
    sin_phi = math.sqrt(1.0 - pf**2)
    q_kvar = (p_kw / pf) * sin_phi
    ib_a = (p_kw * 1000.0) / (math.sqrt(3) * U_LV * pf)
    iz_base, r90_m, x_m = CABLE_DB[f["mm2"]]
    iz_a = round(f["runs"] * iz_base * K_DERATE, 1)
    dv_v = math.sqrt(3) * ib_a * f["length_m"] * ((r90_m * pf + x_m * sin_phi) * 1e-3) / f["runs"]
    vdrop_pct = round((dv_v / U_LV) * 100.0, 2)

    bus_ik = ik_emsb_norm_ka if f["bus"] == "EMSB" else ik_msb_1tx_ka
    assert ib_a <= f["breaker_a"] <= iz_a, f"Cable/breaker violation on {f['id']}"
    assert vdrop_pct <= 4.0, f"Voltage drop violation on {f['id']}: {vdrop_pct}%"
    assert f["icu_ka"] >= bus_ik, f"Icu violation on {f['id']}"

    bus_pq[f["bus"]][0] += p_kw
    bus_pq[f["bus"]][1] += q_kvar
    rec = dict(f)
    rec["ib_a"] = round(ib_a, 1)
    rec["iz_a"] = iz_a
    rec["vdrop_pct"] = vdrop_pct
    feeders.append(rec)

# Contingency totals
p_emsb, q_emsb = bus_pq["EMSB"]
s_emsb = math.hypot(p_emsb, q_emsb)
p_msba, q_msba = bus_pq["MSB_A"]
s_msba = math.hypot(p_msba, q_msba)
p_msbb, q_msbb = bus_pq["MSB_B"]
s_msbb = math.hypot(p_msbb, q_msbb)

p_tot = p_emsb + p_msba + p_msbb
q_tot = q_emsb + q_msba + q_msbb
s_tot = math.hypot(p_tot, q_tot)

# Normal TX1 (MSB_A) & TX2 (MSB_B + EMSB)
s_tx1_df07 = 0.70 * s_msba
s_tx2_df07 = 0.70 * math.hypot(p_msbb + p_emsb, q_msbb + q_emsb)
s_tx2_100e = math.hypot(p_emsb + 0.70 * p_msbb, q_emsb + 0.70 * q_msbb)

# N-1 Single TX with Stage-1 Shed (1 Chiller = 220 kW, PF 0.85) at DF = 0.70
p_ch = 220.0
q_ch = (220.0 / 0.85) * math.sqrt(1.0 - 0.85**2)
s_n1_s1_df07 = 0.70 * math.hypot(p_tot - p_ch, q_tot - q_ch)

# N-1 Single TX at 100% peak coincidence with Stage-2 Shed (F8 + F9 + F10) -> EMSB + F7_IMAGING
p_n1_s2_peak = p_emsb + 180.0
q_n1_s2_peak = q_emsb + (180.0 / 0.90) * math.sqrt(1.0 - 0.90**2)
s_n1_s2_peak = math.hypot(p_n1_s2_peak, q_n1_s2_peak)

assert p_emsb <= GEN_KW and s_emsb <= GEN_KVA, "GEN1 capacity exceeded!"
assert s_tx1_df07 <= TX_KVA and s_tx2_df07 <= TX_KVA and s_tx2_100e <= TX_KVA, "Normal TX loading exceeded!"
assert s_n1_s1_df07 <= TX_KVA and s_n1_s2_peak <= TX_KVA, "N-1 TX loading exceeded!"

# ==============================================================================
# 2. WRITE sld_manifest.json (Exact required schema, F1..F10 order)
# ==============================================================================
feeder_order = [
    "F1_MED_UPS", "F2_FIRE_PUMP", "F3_CLINIC_DB1", "F4_CLINIC_DB2", "F5_LAB_STERIL",
    "F6_ELEVATORS", "F7_IMAGING", "F8_CHILLER1", "F9_CHILLER2", "F10_GEN_PWR",
]
f_by_id = {f["id"]: f for f in feeders}

manifest_breakers = [
    {"id": "Q_MV_IN",    "bus": "RMU_11KV", "in_a": 630,  "icu_ka": 25.0, "type": "VCB"},
    {"id": "Q_MV_TX1",   "bus": "RMU_11KV", "in_a": 630,  "icu_ka": 25.0, "type": "VCB"},
    {"id": "Q_MV_TX2",   "bus": "RMU_11KV", "in_a": 630,  "icu_ka": 25.0, "type": "VCB"},
    {"id": "Q_TX1",      "bus": "MSB_A",    "in_a": 1600, "icu_ka": 50.0, "type": "ACB"},
    {"id": "Q_TX2",      "bus": "MSB_B",    "in_a": 1600, "icu_ka": 50.0, "type": "ACB"},
    {"id": "Q_BT",       "bus": "MSB_A",    "in_a": 1600, "icu_ka": 50.0, "type": "ACB"},
    {"id": "Q_ATS_FEED", "bus": "MSB_B",    "in_a": 1600, "icu_ka": 50.0, "type": "ACB"},
    {"id": "Q_NORM",     "bus": "EMSB",     "in_a": 1600, "icu_ka": 50.0, "type": "ACB_ATS"},
    {"id": "Q_GEN",      "bus": "EMSB",     "in_a": 1600, "icu_ka": 50.0, "type": "ACB_ATS"},
]
for fid in feeder_order:
    f = f_by_id[fid]
    manifest_breakers.append({
        "id": f["q_id"],
        "bus": f["bus"],
        "in_a": f["breaker_a"],
        "icu_ka": f["icu_ka"],
        "type": "MCCB",
    })

manifest = {
    "project": {
        "title": "Hospital 11kV / 400V Dual-Transformer + Standby Generator Power SLD",
        "standards": ["IEC 60909-0:2016", "IEC 60364-5-52", "IEC 60617"],
        "frequency_hz": 50,
        "utility_intake": {
            "id": "UTIL_11KV",
            "voltage_v": 11000,
            "sk_mva": 250.0,
            "ik_ka": round(ik_mv_ka, 2),
        },
    },
    "sources": [
        {"id": "TX1",  "kind": "transformer", "kva": 1000, "voltage_v": 400, "primary_voltage_v": 11000, "vector_group": "Dyn11", "uk_pct": 6.0},
        {"id": "TX2",  "kind": "transformer", "kva": 1000, "voltage_v": 400, "primary_voltage_v": 11000, "vector_group": "Dyn11", "uk_pct": 6.0},
        {"id": "GEN1", "kind": "generator",   "kva": 1000, "kw": 800, "pf": 0.80, "voltage_v": 400, "uk_pct": 15.0, "xd_subtransient_pct": 15.0},
    ],
    "buses": [
        {"id": "RMU_11KV", "voltage_v": 11000, "ik_ka": round(ik_mv_ka, 2),       "essential": False},
        {"id": "MSB_A",    "voltage_v": 400,   "ik_ka": round(ik_msb_1tx_ka, 2),  "ik_paralleled_ka": round(ik_msb_2tx_ka, 2), "ip_ka": round(ip_msb_1tx_ka, 2), "essential": False},
        {"id": "MSB_B",    "voltage_v": 400,   "ik_ka": round(ik_msb_1tx_ka, 2),  "ik_paralleled_ka": round(ik_msb_2tx_ka, 2), "ip_ka": round(ip_msb_1tx_ka, 2), "essential": False},
        {"id": "EMSB",     "voltage_v": 400,   "ik_ka": round(ik_emsb_norm_ka, 2),"ik_gen_ka": round(ik_emsb_gen_ka, 2), "essential": True},
    ],
    "interlocks": [
        {
            "id": "IL_BUS_TIE",
            "devices": ["Q_TX1", "Q_TX2", "Q_BT"],
            "rule": "max_two_closed",
            "description": "Mechanical Kirk-key + electrical 2-of-3 interlock preventing unintentional parallel operation of TX1 & TX2.",
        },
        {
            "id": "IL_ATS",
            "devices": ["Q_NORM", "Q_GEN"],
            "rule": "mutually_exclusive",
            "description": "ATS1 mechanical + electrical interlock prohibiting Utility-Generator paralleling.",
        },
    ],
    "breakers": manifest_breakers,
    "feeders": [
        {
            "id": f_by_id[fid]["id"],
            "bus": f_by_id[fid]["bus"],
            "load_kw": f_by_id[fid]["load_kw"],
            "pf": f_by_id[fid]["pf"],
            "length_m": f_by_id[fid]["length_m"],
            "ib_a": f_by_id[fid]["ib_a"],
            "breaker_a": f_by_id[fid]["breaker_a"],
            "icu_ka": f_by_id[fid]["icu_ka"],
            "cable_runs": f_by_id[fid]["runs"],
            "cable_mm2": f_by_id[fid]["mm2"],
            "iz_a": f_by_id[fid]["iz_a"],
            "vdrop_pct": f_by_id[fid]["vdrop_pct"],
            "fire_rated": f_by_id[fid]["fire_rated"],
        }
        for fid in feeder_order
    ],
    "contingency_analysis": {
        "total_connected_kw": round(p_tot, 1),
        "total_connected_kva": round(s_tot, 1),
        "emsb_essential_kw": round(p_emsb, 1),
        "emsb_essential_kva": round(s_emsb, 1),
        "gen1_loading_kw_pct": round(100.0 * p_emsb / GEN_KW, 1),
        "gen1_loading_kva_pct": round(100.0 * s_emsb / GEN_KVA, 1),
        "normal_tx1_df07_kva": round(s_tx1_df07, 1),
        "normal_tx2_df07_kva": round(s_tx2_df07, 1),
        "normal_tx2_100pct_emsb_70pct_msbb_kva": round(s_tx2_100e, 1),
        "n1_single_tx_shed_1_chiller_df07_kva": round(s_n1_s1_df07, 1),
        "n1_single_tx_shed_stage2_100pct_peak_kva": round(s_n1_s2_peak, 1),
        "load_shedding_stages": {
            "stage_1_auto_n1": ["F8_CHILLER1"],
            "stage_2_peak_override": ["F8_CHILLER1", "F9_CHILLER2", "F10_GEN_PWR"],
        },
    },
}

(OUT_DIR / "sld_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Wrote sld_manifest.json")

# ==============================================================================
# 3. DUAL-BACKEND VECTOR CANVAS (SVG + LAYERED DXF R12)
# ==============================================================================
W, H = 1680.0, 1120.0   # SVG logical canvas (px), mapped 0.5x to 840x560 mm (A1) in DXF

class DualCanvas:
    def __init__(self, width=W, height=H):
        self.w = width
        self.h = height
        self.svg_defs = []
        self.svg_body = []
        self.dxf_entities = []
        self.scale_dxf = 0.5  # 1680 px -> 840 mm (A1 landscape)

    def _dxf_pt(self, x, y):
        return x * self.scale_dxf, (self.h - y) * self.scale_dxf

    def line(self, x1, y1, x2, y2, stroke="#111827", sw=1.8, dash=None, layer="EQUIPMENT", ltype="CONTINUOUS"):
        da = f' stroke-dasharray="{dash}"' if dash else ""
        self.svg_body.append(
            f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
            f'stroke="{stroke}" stroke-width="{sw}" stroke-linecap="round"{da}/>'
        )
        dx1, dy1 = self._dxf_pt(x1, y1)
        dx2, dy2 = self._dxf_pt(x2, y2)
        self.dxf_entities.append(("LINE", layer, ltype, (dx1, dy1, dx2, dy2)))

    def rect(self, x, y, w, h, stroke="#111827", fill="none", sw=1.4, dash=None, rx=0, layer="EQUIPMENT", ltype="CONTINUOUS"):
        da = f' stroke-dasharray="{dash}"' if dash else ""
        rxa = f' rx="{rx}"' if rx else ""
        self.svg_body.append(
            f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{da}{rxa}/>'
        )
        if stroke != "none":
            pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
            for (ax, ay), (bx, by) in zip(pts, pts[1:]):
                dx1, dy1 = self._dxf_pt(ax, ay)
                dx2, dy2 = self._dxf_pt(bx, by)
                self.dxf_entities.append(("LINE", layer, ltype, (dx1, dy1, dx2, dy2)))

    def circle(self, cx, cy, r, stroke="#111827", fill="none", sw=1.8, layer="EQUIPMENT"):
        self.svg_body.append(
            f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>'
        )
        dx, dy = self._dxf_pt(cx, cy)
        self.dxf_entities.append(("CIRCLE", layer, "CONTINUOUS", (dx, dy, r * self.scale_dxf)))

    def polygon(self, pts, stroke="#111827", fill="none", sw=1.5, layer="EQUIPMENT"):
        pstr = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        self.svg_body.append(
            f'<polygon points="{pstr}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round"/>'
        )
        closed = list(pts) + [pts[0]]
        for (ax, ay), (bx, by) in zip(closed, closed[1:]):
            dx1, dy1 = self._dxf_pt(ax, ay)
            dx2, dy2 = self._dxf_pt(bx, by)
            self.dxf_entities.append(("LINE", layer, "CONTINUOUS", (dx1, dy1, dx2, dy2)))

    def polyline(self, pts, stroke="#111827", sw=1.8, dash=None, layer="EQUIPMENT", ltype="CONTINUOUS"):
        pstr = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        da = f' stroke-dasharray="{dash}"' if dash else ""
        self.svg_body.append(
            f'<polyline points="{pstr}" fill="none" stroke="{stroke}" stroke-width="{sw}" '
            f'stroke-linejoin="round" stroke-linecap="round"{da}/>'
        )
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            dx1, dy1 = self._dxf_pt(ax, ay)
            dx2, dy2 = self._dxf_pt(bx, by)
            self.dxf_entities.append(("LINE", layer, ltype, (dx1, dy1, dx2, dy2)))

    def text(self, x, y, txt, size=11, fill="#0F172A", weight="normal", anchor="start", layer="ANNOTATIONS", mono=False):
        fam = "'DejaVu Sans Mono', monospace" if mono else "'DejaVu Sans', Arial, sans-serif"
        esc = sax.escape(str(txt))
        self.svg_body.append(
            f'<text x="{x:.1f}" y="{y:.1f}" font-family="{fam}" font-size="{size}" '
            f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{esc}</text>'
        )
        # Map DXF text cleanly (ASCII-safe for DXF R12 compatibility)
        dxf_txt = (
            str(txt)
            .replace("Φ", "Ph").replace("φ", "phi").replace("Δ", "d").replace("≤", "<=")
            .replace("≥", ">=").replace("×", "x").replace("²", "2").replace("—", "-")
            .replace("–", "-").replace("↔", "<->").replace("→", "->").replace("″", "''")
            .replace("°", " deg").replace("•", "-")
        ).encode("ascii", "replace").decode("ascii")
        dx, dy = self._dxf_pt(x, y)
        h_mm = max(1.8, size * self.scale_dxf * 0.72)
        halign = {"start": 0, "middle": 1, "end": 2}.get(anchor, 0)
        self.dxf_entities.append(("TEXT", layer, "CONTINUOUS", (dx, dy, h_mm, dxf_txt, halign)))

    def dot(self, cx, cy, r=3.8, fill="#1D4ED8", layer="EQUIPMENT"):
        self.circle(cx, cy, r, stroke=fill, fill=fill, sw=1.0, layer=layer)

    def render_svg(self):
        return (
            f'<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w:.0f} {self.h:.0f}" '
            f'width="{self.w:.0f}" height="{self.h:.0f}">\n'
            f'<rect x="0" y="0" width="{self.w:.0f}" height="{self.h:.0f}" fill="#FFFFFF"/>\n'
            + "\n".join(self.svg_body)
            + "\n</svg>\n"
        )

    def render_dxf(self):
        layers = [
            ("BORDER_TITLE", 7, "CONTINUOUS"),
            ("MV_11KV",      1, "CONTINUOUS"),
            ("LV_NORMAL",    5, "CONTINUOUS"),
            ("LV_ESSENTIAL", 3, "CONTINUOUS"),
            ("FIRE_SAFETY",  1, "DASHED"),
            ("EQUIPMENT",    7, "CONTINUOUS"),
            ("INTERLOCKS",   6, "DASHED"),
            ("ANNOTATIONS",  7, "CONTINUOUS"),
        ]
        out = [
            "0", "SECTION", "2", "HEADER",
            "9", "$ACADVER", "1", "AC1009",
            "9", "$INSUNITS", "70", "4",
            "9", "$EXTMIN", "10", "0.0", "20", "0.0", "30", "0.0",
            "9", "$EXTMAX", "10", f"{self.w * self.scale_dxf:.2f}", "20", f"{self.h * self.scale_dxf:.2f}", "30", "0.0",
            "0", "ENDSEC",
            "0", "SECTION", "2", "TABLES",
            # LTYPE table
            "0", "TABLE", "2", "LTYPE", "70", "2",
            "0", "LTYPE", "2", "CONTINUOUS", "70", "0", "3", "Solid line", "72", "65", "73", "0", "40", "0.0",
            "0", "LTYPE", "2", "DASHED", "70", "0", "3", "Dashed __ __ __", "72", "65", "73", "2", "40", "6.0", "49", "4.0", "49", "-2.0",
            "0", "ENDTAB",
            # LAYER table
            "0", "TABLE", "2", "LAYER", "70", str(len(layers)),
        ]
        for lname, color, ltype in layers:
            out += ["0", "LAYER", "2", lname, "70", "0", "62", str(color), "6", ltype]
        out += [
            "0", "ENDTAB",
            "0", "TABLE", "2", "STYLE", "70", "1",
            "0", "STYLE", "2", "STANDARD", "70", "0", "40", "0.0", "41", "1.0", "50", "0.0", "71", "0", "42", "2.5", "3", "txt", "4", "",
            "0", "ENDTAB",
            "0", "ENDSEC",
            "0", "SECTION", "2", "ENTITIES",
        ]
        for kind, layer, ltype, data in self.dxf_entities:
            if kind == "LINE":
                x1, y1, x2, y2 = data
                out += [
                    "0", "LINE", "8", layer, "6", ltype,
                    "10", f"{x1:.3f}", "20", f"{y1:.3f}", "30", "0.0",
                    "11", f"{x2:.3f}", "21", f"{y2:.3f}", "31", "0.0",
                ]
            elif kind == "CIRCLE":
                cx, cy, r = data
                out += [
                    "0", "CIRCLE", "8", layer, "6", ltype,
                    "10", f"{cx:.3f}", "20", f"{cy:.3f}", "30", "0.0",
                    "40", f"{r:.3f}",
                ]
            elif kind == "TEXT":
                dx, dy, h_mm, txt, halign = data
                out += [
                    "0", "TEXT", "8", layer, "7", "STANDARD",
                    "10", f"{dx:.3f}", "20", f"{dy:.3f}", "30", "0.0",
                    "40", f"{h_mm:.3f}", "1", txt,
                ]
                if halign != 0:
                    out += ["72", str(halign), "11", f"{dx:.3f}", "21", f"{dy:.3f}", "31", "0.0"]
        out += ["0", "ENDSEC", "0", "EOF"]
        return "\n".join(out) + "\n"


# ==============================================================================
# 4. IEC 60617 SYMBOL PRIMITIVES
# ==============================================================================
C_MV   = "#B91C1C"   # Crimson Red — 11 kV MV
C_NORM = "#1D4ED8"   # Engineering Blue — 400 V Normal LV
C_ESS  = "#047857"   # Emerald Green — 400 V Essential / Standby LV
C_FIRE = "#DC2626"   # Fire Red — Fire & Life-Safety Pump
C_IL   = "#9333EA"   # Vivid Purple — Mechanical/Electrical Interlocks
C_DARK = "#0F172A"   # Slate 900
C_MUT  = "#475569"   # Slate 600
C_BOX  = "#F8FAFC"   # Subtle panel background

def draw_breaker_v(cv, x, y_top, y_bot, color, layer, drawout=False, state=None, state_side="left"):
    """Standard IEC 60617 vertical circuit breaker (switch blade + X cross + optional draw-out chevrons)."""
    ym = 0.5 * (y_top + y_bot)
    c1, c2 = ym - 12, ym + 12
    # Stems
    cv.line(x, y_top, x, c1, stroke=color, sw=2.2, layer=layer)
    cv.line(x, c2, x, y_bot, stroke=color, sw=2.2, layer=layer)
    # Draw-out chevrons for ACB / VCB
    if drawout:
        for yy, sgn in [(y_top + 6, 1), (y_bot - 6, -1)]:
            cv.polyline([(x - 6, yy - 4*sgn), (x, yy + 2*sgn), (x + 6, yy - 4*sgn)], stroke=color, sw=1.5, layer="EQUIPMENT")
            cv.polyline([(x - 6, yy + 1*sgn), (x, yy + 7*sgn), (x + 6, yy + 1*sgn)], stroke=color, sw=1.5, layer="EQUIPMENT")
    # IEC CB X mark at top contact
    cv.line(x - 4.5, c1 - 4.5, x + 4.5, c1 + 4.5, stroke=C_DARK, sw=1.8, layer="EQUIPMENT")
    cv.line(x - 4.5, c1 + 4.5, x + 4.5, c1 - 4.5, stroke=C_DARK, sw=1.8, layer="EQUIPMENT")
    # Angled moving contact
    cv.line(x, c2, x - 11, c1 + 1, stroke=C_DARK, sw=2.2, layer="EQUIPMENT")
    cv.dot(x, c2, r=2.4, fill=C_DARK, layer="EQUIPMENT")
    # Small trip unit box on left
    cv.rect(x - 20, ym - 5, 8, 10, stroke=C_DARK, fill="#FFFFFF", sw=1.1, layer="EQUIPMENT")
    cv.line(x - 12, ym, x - 5, ym, stroke=C_DARK, sw=1.0, dash="2,2", layer="EQUIPMENT")
    if state:
        bg = "#DCFCE7" if state == "NC" else "#FEF3C7"
        fg = "#166534" if state == "NC" else "#92400E"
        bx = (x - 46) if state_side == "left" else (x + 12)
        cv.rect(bx, ym - 8, 22, 15, stroke=fg, fill=bg, sw=1.0, rx=3, layer="ANNOTATIONS")
        cv.text(bx + 11, ym + 3, state, size=8.5, fill=fg, weight="bold", anchor="middle")

def draw_breaker_h(cv, x_left, x_right, y, color, layer, drawout=True, state="NO"):
    """Horizontal IEC 60617 Bus-Tie ACB."""
    xm = 0.5 * (x_left + x_right)
    c1, c2 = xm - 13, xm + 13
    cv.line(x_left, y, c1, y, stroke=color, sw=2.6, layer=layer)
    cv.line(c2, y, x_right, y, stroke=color, sw=2.6, layer=layer)
    if drawout:
        for xx, sgn in [(x_left + 8, 1), (x_right - 8, -1)]:
            cv.polyline([(xx - 4*sgn, y - 6), (xx + 2*sgn, y), (xx - 4*sgn, y + 6)], stroke=color, sw=1.5, layer="EQUIPMENT")
            cv.polyline([(xx + 1*sgn, y - 6), (xx + 7*sgn, y), (xx + 1*sgn, y + 6)], stroke=color, sw=1.5, layer="EQUIPMENT")
    # X mark at left contact
    cv.line(c1 - 4.5, y - 4.5, c1 + 4.5, y + 4.5, stroke=C_DARK, sw=1.8, layer="EQUIPMENT")
    cv.line(c1 - 4.5, y + 4.5, c1 + 4.5, y - 4.5, stroke=C_DARK, sw=1.8, layer="EQUIPMENT")
    # Open angled contact blade
    cv.line(c2, y, c1 + 1, y - 12, stroke=C_DARK, sw=2.3, layer="EQUIPMENT")
    cv.dot(c2, y, r=2.5, fill=C_DARK, layer="EQUIPMENT")
    if state:
        bg = "#FEF3C7" if state == "NO" else "#DCFCE7"
        fg = "#92400E" if state == "NO" else "#166534"
        cv.rect(xm - 12, y + 10, 24, 15, stroke=fg, fill=bg, sw=1.0, rx=3, layer="ANNOTATIONS")
        cv.text(xm, y + 21, state, size=8.5, fill=fg, weight="bold", anchor="middle")

def draw_transformer(cv, cx, cy):
    """IEC 60617 two-winding Dyn11 transformer with earth symbol."""
    r = 21.0
    cv.circle(cx, cy - 13, r, stroke=C_MV, fill="#FFFFFF", sw=2.2, layer="EQUIPMENT")
    cv.circle(cx, cy + 13, r, stroke=C_NORM, fill="none", sw=2.2, layer="EQUIPMENT")
    # Delta triangle inside upper winding
    cv.polygon([(cx, cy - 21), (cx - 7, cy - 9), (cx + 7, cy - 9)], stroke=C_MV, fill="none", sw=1.4, layer="EQUIPMENT")
    # Star Y inside lower winding
    cv.line(cx, cy + 15, cx, cy + 24, stroke=C_NORM, sw=1.4, layer="EQUIPMENT")
    cv.line(cx, cy + 15, cx - 6.5, cy + 9, stroke=C_NORM, sw=1.4, layer="EQUIPMENT")
    cv.line(cx, cy + 15, cx + 6.5, cy + 9, stroke=C_NORM, sw=1.4, layer="EQUIPMENT")
    # Neutral earthing off right side of LV winding
    cv.polyline([(cx + r, cy + 13), (cx + r + 14, cy + 13), (cx + r + 14, cy + 25)], stroke=C_DARK, sw=1.4, layer="EQUIPMENT")
    draw_earth(cv, cx + r + 14, cy + 25)

def draw_earth(cv, cx, y):
    cv.line(cx - 8, y, cx + 8, y, stroke=C_DARK, sw=1.6, layer="EQUIPMENT")
    cv.line(cx - 5, y + 4, cx + 5, y + 4, stroke=C_DARK, sw=1.4, layer="EQUIPMENT")
    cv.line(cx - 2, y + 8, cx + 2, y + 8, stroke=C_DARK, sw=1.2, layer="EQUIPMENT")

def draw_generator(cv, cx, cy):
    """IEC 60617 synchronous generator symbol + neutral earth."""
    r = 28.0
    cv.circle(cx, cy, r, stroke=C_ESS, fill="#ECFDF5", sw=2.4, layer="EQUIPMENT")
    cv.text(cx, cy - 1, "G", size=18, fill=C_ESS, weight="bold", anchor="middle")
    # Sine wave '~'
    cv.text(cx, cy + 16, "3~", size=11, fill=C_ESS, weight="bold", anchor="middle")
    # Neutral earth
    cv.polyline([(cx + r, cy), (cx + r + 14, cy), (cx + r + 14, cy + 16)], stroke=C_DARK, sw=1.4, layer="EQUIPMENT")
    draw_earth(cv, cx + r + 14, cy + 16)

def draw_ct(cv, cx, cy, label="50/51"):
    """IEC CT toroidal mark + protection relay bubble."""
    cv.circle(cx, cy, 4.5, stroke=C_DARK, fill="#FFFFFF", sw=1.4, layer="EQUIPMENT")
    cv.line(cx + 4.5, cy, cx + 18, cy, stroke=C_DARK, sw=1.1, layer="EQUIPMENT")
    cv.circle(cx + 30, cy, 12, stroke=C_DARK, fill="#FFFFFF", sw=1.2, layer="EQUIPMENT")
    cv.text(cx + 30, cy + 3.5, label, size=7.5, fill=C_DARK, weight="bold", anchor="middle")

def draw_3ph_ticks(cv, cx, cy, color):
    for dy in (-4, 0, 4):
        cv.line(cx - 5, cy + dy + 3, cx + 5, cy + dy - 3, stroke=color, sw=1.2, layer="EQUIPMENT")

def draw_interlock_diamond(cv, cx, cy, tag, line1, line2):
    cv.polygon([(cx, cy - 14), (cx + 16, cy), (cx, cy + 14), (cx - 16, cy)],
               stroke=C_IL, fill="#FAF5FF", sw=1.8, layer="INTERLOCKS")
    cv.text(cx, cy + 3.5, "M/E", size=8.5, fill=C_IL, weight="bold", anchor="middle")
    cv.rect(cx - 72, cy + 18, 144, 30, stroke=C_IL, fill="#FAF5FF", sw=1.1, rx=4, layer="INTERLOCKS")
    cv.text(cx, cy + 30, f"{tag}: {line1}", size=8.2, fill=C_IL, weight="bold", anchor="middle")
    cv.text(cx, cy + 42, line2, size=7.8, fill="#581C87", anchor="middle")


# ==============================================================================
# 5. BUILD COMPLETE HOSPITAL SLD GEOMETRY
# ==============================================================================
cv = DualCanvas(W, H)

# --- 5.1 Outer Sheet Border & Zone Markers ---
cv.rect(16, 16, W - 32, H - 32, stroke=C_DARK, fill="none", sw=2.2, layer="BORDER_TITLE")
cv.rect(24, 24, W - 48, H - 48, stroke="#94A3B8", fill="none", sw=0.8, layer="BORDER_TITLE")
for i, col_lbl in enumerate("ABCDEFGH"):
    zx = 24 + (i + 0.5) * (W - 48) / 8.0
    cv.text(zx, 22.5, col_lbl, size=7.5, fill=C_MUT, anchor="middle", layer="BORDER_TITLE")
    cv.text(zx, H - 17.5, col_lbl, size=7.5, fill=C_MUT, anchor="middle", layer="BORDER_TITLE")
for i in range(1, 6):
    zy = 24 + (i - 0.5) * (H - 48) / 5.0
    cv.text(20, zy + 3, str(i), size=7.5, fill=C_MUT, anchor="middle", layer="BORDER_TITLE")
    cv.text(W - 20, zy + 3, str(i), size=7.5, fill=C_MUT, anchor="middle", layer="BORDER_TITLE")

# Top Sheet Banner
cv.rect(24, 24, W - 48, 28, stroke=C_DARK, fill="#0F172A", sw=1.0, layer="BORDER_TITLE")
cv.text(42, 43, "HOSPITAL / MEDICAL CENTER — 11 kV / 400 V DUAL-TRANSFORMER + STANDBY GENERATOR SINGLE-LINE DIAGRAM (IEC 60909 / IEC 60364)",
        size=12.5, fill="#FFFFFF", weight="bold", anchor="start", layer="BORDER_TITLE")
cv.text(W - 40, 43, "50 Hz  |  2×1000 kVA Dyn11  |  1000 kVA GEN1  |  DWG: E-SLD-001 REV A",
        size=10.5, fill="#CBD5E1", weight="bold", anchor="end", layer="BORDER_TITLE")

# --- 5.2 MEDIUM VOLTAGE 11 kV INTAKE & RMU_11KV ---
X_TX1 = 215.0
X_TX2 = 595.0
X_UTIL = 405.0

# Utility Grid Feeder
cv.rect(X_UTIL - 115, 62, 230, 32, stroke=C_MV, fill="#FEF2F2", sw=1.4, rx=4, layer="MV_11KV")
cv.text(X_UTIL, 76, "UTIL_11KV — 11 kV UTILITY INTAKE", size=10.0, fill=C_MV, weight="bold", anchor="middle")
cv.text(X_UTIL, 89, f"3Φ 50Hz | Sk'' = 250 MVA | Ik'' = {ik_mv_ka:.2f} kA", size=8.6, fill="#7F1D1D", anchor="middle")
# Incoming arrow down into Q_MV_IN
cv.line(X_UTIL, 94, X_UTIL, 110, stroke=C_MV, sw=2.4, layer="MV_11KV")
cv.polygon([(X_UTIL, 113), (X_UTIL - 4.5, 105), (X_UTIL + 4.5, 105)], stroke=C_MV, fill=C_MV, sw=1.0, layer="MV_11KV")

# RMU Enclosure
cv.rect(92, 116, 626, 132, stroke=C_MV, fill="#FFFBFB", sw=1.3, dash="6,4", rx=6, layer="MV_11KV", ltype="DASHED")
cv.text(104, 132, "RMU_11KV — 11 kV 3-WAY SF6 / VACUUM RMU", size=9.0, fill=C_MV, weight="bold")
cv.text(104, 144, "Rated: 11 kV, 630 A, Icu = 25 kA", size=8.0, fill="#7F1D1D")

# Q_MV_IN
draw_breaker_v(cv, X_UTIL, 113, 166, C_MV, "MV_11KV", drawout=False, state="NC")
cv.text(X_UTIL + 14, 138, "Q_MV_IN", size=9.0, fill=C_DARK, weight="bold")
cv.text(X_UTIL + 14, 150, "VCB 630A / 25kA", size=8.0, fill=C_MUT)

# 11 kV RMU Busbar
Y_MV_BUS = 172.0
cv.line(130, Y_MV_BUS, 680, Y_MV_BUS, stroke=C_MV, sw=5.0, layer="MV_11KV")
cv.dot(X_UTIL, Y_MV_BUS, r=4.2, fill=C_MV, layer="MV_11KV")
cv.dot(X_TX1, Y_MV_BUS, r=4.2, fill=C_MV, layer="MV_11KV")
cv.dot(X_TX2, Y_MV_BUS, r=4.2, fill=C_MV, layer="MV_11KV")
cv.text(678, Y_MV_BUS - 8, f"11 kV BUS (Ik'' = {ik_mv_ka:.2f} kA)", size=8.5, fill=C_MV, weight="bold", anchor="end")

# Outgoing MV VCBs Q_MV_TX1 & Q_MV_TX2
for x_tx, q_mv, tx_id in [(X_TX1, "Q_MV_TX1", "TX1"), (X_TX2, "Q_MV_TX2", "TX2")]:
    draw_breaker_v(cv, x_tx, Y_MV_BUS, 232, C_MV, "MV_11KV", drawout=False, state="NC")
    cv.text(x_tx + 14, 200, q_mv, size=9.0, fill=C_DARK, weight="bold")
    cv.text(x_tx + 14, 212, "VCB 630A / 25kA", size=8.0, fill=C_MUT)
    draw_ct(cv, x_tx, 240, "50/51")
    # MV cable down to TX primary
    cv.line(x_tx, 232, x_tx, 273, stroke=C_MV, sw=2.2, layer="MV_11KV")
    cv.text(x_tx - 10, 263, "3×1C 95mm² 11kV", size=7.6, fill=C_MV, anchor="end")

# --- 5.3 DUAL 1000 kVA TRANSFORMERS (TX1, TX2) & MAIN ACBs (Q_TX1, Q_TX2) ---
Y_TX = 307.0
for x_tx, tx_id, q_lv, bus_id in [
    (X_TX1, "TX1", "Q_TX1", "MSB_A"),
    (X_TX2, "TX2", "Q_TX2", "MSB_B"),
]:
    draw_transformer(cv, x_tx, Y_TX)
    # Transformer nameplate callout on left of symbol
    cv.rect(x_tx - 150, Y_TX - 29, 122, 58, stroke="#CBD5E1", fill=C_BOX, sw=1.0, rx=4, layer="ANNOTATIONS")
    cv.text(x_tx - 89, Y_TX - 14, f"{tx_id} — 1000 kVA", size=9.8, fill=C_DARK, weight="bold", anchor="middle")
    cv.text(x_tx - 89, Y_TX - 1,  "11 kV / 400 V, Dyn11", size=8.3, fill=C_MUT, anchor="middle")
    cv.text(x_tx - 89, Y_TX + 11, "uk = 6.0% | ur = 1.1%", size=8.3, fill=C_MUT, anchor="middle")
    cv.text(x_tx - 89, Y_TX + 23, "Ir = 1443 A (LV)", size=8.3, fill=C_NORM, weight="bold", anchor="middle")

    # LV Busduct TX -> Q_TX
    cv.line(x_tx, Y_TX + 34, x_tx, 396, stroke=C_NORM, sw=2.6, layer="LV_NORMAL")
    draw_3ph_ticks(cv, x_tx, 365, C_NORM)
    cv.text(x_tx - 10, 368, "2000A Cu Busduct (8m)", size=7.6, fill=C_NORM, anchor="end")

    # Main Incomer ACB
    draw_breaker_v(cv, x_tx, 396, 468, C_NORM, "LV_NORMAL", drawout=True, state="NC")
    if tx_id == "TX1":
        cv.text(x_tx - 52, 422, q_lv, size=9.6, fill=C_DARK, weight="bold", anchor="end")
        cv.text(x_tx - 52, 435, "ACB 1600 A", size=8.3, fill=C_NORM, weight="bold", anchor="end")
        cv.text(x_tx - 52, 447, "Icu = 50 kA (LSIG)", size=8.0, fill=C_MUT, anchor="end")
    else:
        cv.text(x_tx + 14, 422, q_lv, size=9.6, fill=C_DARK, weight="bold")
        cv.text(x_tx + 14, 435, "ACB 1600 A", size=8.3, fill=C_NORM, weight="bold")
        cv.text(x_tx + 14, 447, "50 kA (LSIG)", size=8.0, fill=C_MUT)

    # Stem down to MSB busbar
    cv.line(x_tx, 468, x_tx, 540, stroke=C_NORM, sw=2.8, layer="LV_NORMAL")

# --- 5.4 SPLIT MAIN LV SWITCHBOARD (MSB_A, MSB_B) + INTERLOCKED BUS-TIE (Q_BT) ---
Y_LV_BUS = 540.0

# Switchboard enclosure dashed border around MSB_A + Q_BT + MSB_B
cv.rect(68, 492, 682, 74, stroke=C_NORM, fill="#F8FAFF", sw=1.1, dash="6,4", rx=6, layer="LV_NORMAL", ltype="DASHED")
cv.text(80, 507, "MAIN LV SWITCHBOARD (400 V, 50 Hz, 2000 A Cu)", size=8.8, fill=C_NORM, weight="bold")

# MSB_A Busbar
cv.line(82, Y_LV_BUS, 355, Y_LV_BUS, stroke=C_NORM, sw=6.0, layer="LV_NORMAL")
cv.dot(X_TX1, Y_LV_BUS, r=4.8, fill=C_NORM, layer="LV_NORMAL")
cv.text(85, Y_LV_BUS - 11, f"MSB_A (Normal) | Ik'' = {ik_msb_1tx_ka:.2f} kA", size=9.2, fill=C_NORM, weight="bold")

# MSB_B Busbar
cv.line(455, Y_LV_BUS, 738, Y_LV_BUS, stroke=C_NORM, sw=6.0, layer="LV_NORMAL")
cv.dot(X_TX2, Y_LV_BUS, r=4.8, fill=C_NORM, layer="LV_NORMAL")
cv.text(460, Y_LV_BUS - 11, f"MSB_B (Normal) | Ik'' = {ik_msb_1tx_ka:.2f} kA", size=9.2, fill=C_NORM, weight="bold")

# Bus-Tie ACB Q_BT
draw_breaker_h(cv, 355, 455, Y_LV_BUS, C_NORM, "LV_NORMAL", drawout=True, state="NO")
cv.text(405, Y_LV_BUS - 25, "Q_BT (Bus-Tie)", size=8.6, fill=C_DARK, weight="bold", anchor="middle")
cv.text(405, Y_LV_BUS - 15, "ACB 1600A / 50kA", size=7.6, fill=C_MUT, anchor="middle")

# 2-of-3 Interlock (IL_BUS_TIE) linking Q_TX1, Q_TX2, Q_BT
Y_IL_BT = 428.0
cv.polyline([(X_TX1 + 9, Y_IL_BT), (389, Y_IL_BT)], stroke=C_IL, sw=1.4, dash="4,3", layer="INTERLOCKS", ltype="DASHED")
cv.polyline([(X_TX2 - 48, Y_IL_BT), (421, Y_IL_BT)], stroke=C_IL, sw=1.4, dash="4,3", layer="INTERLOCKS", ltype="DASHED")
cv.line(405, Y_IL_BT + 48, 405, Y_LV_BUS - 35, stroke=C_IL, sw=1.4, dash="4,3", layer="INTERLOCKS", ltype="DASHED")
draw_interlock_diamond(cv, 405, Y_IL_BT, "IL_BUS_TIE", "2-of-3", "Max 2 of {Q_TX1, Q_TX2, Q_BT}")

# --- 5.5 STANDBY GENERATOR (GEN1), ATS1 & ESSENTIAL BUS (EMSB) ---
X_NORM_ATS = 1135.0
X_GEN_ATS  = 1395.0
X_ATS_MID  = 1265.0

# Normal feed tap from MSB_B (x = 724) -> Q_ATS_FEED -> up to y = 326 -> right to Q_NORM
X_TAP = 724.0
cv.dot(X_TAP, Y_LV_BUS, r=4.5, fill=C_NORM, layer="LV_NORMAL")
cv.line(X_TAP, Y_LV_BUS, X_TAP, 480, stroke=C_NORM, sw=2.6, layer="LV_NORMAL")
draw_breaker_v(cv, X_TAP, 408, 480, C_NORM, "LV_NORMAL", drawout=True, state="NC", state_side="right")
cv.text(X_TAP + 40, 434, "Q_ATS_FEED", size=8.8, fill=C_DARK, weight="bold")
cv.text(X_TAP + 40, 446, "ACB 1600A / 50kA", size=7.8, fill=C_MUT)

cv.polyline([(X_TAP, 408), (X_TAP, 326), (X_NORM_ATS, 326), (X_NORM_ATS, 366)], stroke=C_NORM, sw=2.6, layer="LV_NORMAL")
cv.text(925, 319, "Normal Utility Feed to ATS1 — 1600 A Cu Busduct (10 m)", size=8.4, fill=C_NORM, weight="bold", anchor="middle")

# Standby Diesel Generator GEN1
Y_GEN = 210.0
draw_generator(cv, X_GEN_ATS, Y_GEN)
cv.rect(X_GEN_ATS + 52, Y_GEN - 34, 175, 68, stroke=C_ESS, fill="#ECFDF5", sw=1.3, rx=5, layer="LV_ESSENTIAL")
cv.text(X_GEN_ATS + 139, Y_GEN - 17, "GEN1 — STANDBY DIESEL", size=9.6, fill=C_ESS, weight="bold", anchor="middle")
cv.text(X_GEN_ATS + 139, Y_GEN - 3,  "1000 kVA / 800 kW, 400 V", size=8.6, fill=C_DARK, weight="bold", anchor="middle")
cv.text(X_GEN_ATS + 139, Y_GEN + 10, "PF = 0.80 | Xd'' = 15.0%", size=8.2, fill=C_MUT, anchor="middle")
cv.text(X_GEN_ATS + 139, Y_GEN + 23, f"EMSB Load: {p_emsb:.0f} kW ({100*p_emsb/GEN_KW:.1f}%)", size=8.2, fill=C_ESS, weight="bold", anchor="middle")

# Generator feeder down to Q_GEN
cv.line(X_GEN_ATS, Y_GEN + 28, X_GEN_ATS, 366, stroke=C_ESS, sw=2.6, layer="LV_ESSENTIAL")
draw_3ph_ticks(cv, X_GEN_ATS, 295, C_ESS)
cv.text(X_GEN_ATS + 12, 292, "4×(3×240mm²) Cu XLPE", size=8.0, fill=C_ESS, weight="bold")
cv.text(X_GEN_ATS + 12, 304, "L = 15 m | Ik''(GEN) = 10.22 kA", size=7.8, fill=C_MUT)

# ATS1 Enclosure
cv.rect(1005, 344, 500, 148, stroke=C_ESS, fill="#F0FDF4", sw=1.5, dash="6,4", rx=7, layer="LV_ESSENTIAL", ltype="DASHED")
cv.text(1018, 360, "ATS1 — AUTOMATIC TRANSFER SWITCH (1600 A, 4-POLE, OPEN TRANSITION)", size=9.0, fill=C_ESS, weight="bold")

# Q_NORM & Q_GEN inside ATS1
draw_breaker_v(cv, X_NORM_ATS, 366, 444, C_NORM, "LV_NORMAL", drawout=True, state="NC", state_side="left")
cv.text(X_NORM_ATS - 52, 398, "Q_NORM", size=9.2, fill=C_NORM, weight="bold", anchor="end")
cv.text(X_NORM_ATS - 52, 410, "1600A / 50kA", size=7.8, fill=C_MUT, anchor="end")

draw_breaker_v(cv, X_GEN_ATS, 366, 444, C_ESS, "LV_ESSENTIAL", drawout=True, state="NO", state_side="left")
cv.text(X_GEN_ATS + 14, 394, "Q_GEN", size=9.2, fill=C_ESS, weight="bold")
cv.text(X_GEN_ATS + 14, 406, "1600A / 50kA", size=7.8, fill=C_MUT)
cv.text(X_GEN_ATS + 14, 418, "Auto-Start <10s", size=7.6, fill=C_ESS)

# ATS1 Mutually Exclusive Interlock (IL_ATS)
cv.line(X_NORM_ATS + 10, 405, X_ATS_MID - 16, 405, stroke=C_IL, sw=1.5, dash="4,3", layer="INTERLOCKS", ltype="DASHED")
cv.line(X_ATS_MID + 16, 405, X_GEN_ATS - 48, 405, stroke=C_IL, sw=1.5, dash="4,3", layer="INTERLOCKS", ltype="DASHED")
draw_interlock_diamond(cv, X_ATS_MID, 405, "IL_ATS", "MUTUALLY EXCL.", "No Utility-Gen Paralleling")

# ATS1 Common Output -> EMSB Busbar
cv.polyline([(X_NORM_ATS, 444), (X_NORM_ATS, 476), (X_GEN_ATS, 476), (X_GEN_ATS, 444)], stroke=C_ESS, sw=3.2, layer="LV_ESSENTIAL")
cv.dot(X_ATS_MID, 476, r=4.2, fill=C_ESS, layer="LV_ESSENTIAL")
cv.line(X_ATS_MID, 476, X_ATS_MID, Y_LV_BUS, stroke=C_ESS, sw=3.2, layer="LV_ESSENTIAL")

# EMSB Switchboard Enclosure & Busbar
cv.rect(808, 492, 834, 74, stroke=C_ESS, fill="#F0FDF4", sw=1.2, dash="6,4", rx=6, layer="LV_ESSENTIAL", ltype="DASHED")
cv.text(820, 507, "ESSENTIAL MAIN SWITCHBOARD — EMSB (400 V, 3Φ+N+PE, 1600 A Cu, GEN1-BACKED)", size=9.0, fill=C_ESS, weight="bold")
cv.line(820, Y_LV_BUS, 1630, Y_LV_BUS, stroke=C_ESS, sw=6.0, layer="LV_ESSENTIAL")
cv.dot(X_ATS_MID, Y_LV_BUS, r=4.8, fill=C_ESS, layer="LV_ESSENTIAL")
cv.text(824, Y_LV_BUS - 11, f"EMSB (Essential) | Ik''(Norm) = {ik_emsb_norm_ka:.2f} kA | Ik''(GEN) = {ik_emsb_gen_ka:.2f} kA", size=9.0, fill=C_ESS, weight="bold")

# --- 5.6 10 OUTGOING SUB-MAIN FEEDERS ---
FEEDER_X = {
    # MSB_A (x: 82..355)
    "F7_IMAGING":    148.0,
    "F8_CHILLER1":   292.0,
    # MSB_B (x: 455..738)
    "F9_CHILLER2":   518.0,
    "F10_GEN_PWR":   658.0,
    # EMSB (x: 820..1630)
    "F1_MED_UPS":    882.0,
    "F2_FIRE_PUMP": 1018.0,
    "F3_CLINIC_DB1":1154.0,
    "F4_CLINIC_DB2":1290.0,
    "F5_LAB_STERIL":1426.0,
    "F6_ELEVATORS": 1562.0,
}

for f in feeders:
    cx = FEEDER_X[f["id"]]
    is_ess = (f["bus"] == "EMSB")
    is_fire = f["fire_rated"]
    col = C_FIRE if is_fire else (C_ESS if is_ess else C_NORM)
    lyr = "FIRE_SAFETY" if is_fire else ("LV_ESSENTIAL" if is_ess else "LV_NORMAL")

    # Busbar tap
    cv.dot(cx, Y_LV_BUS, r=4.0, fill=col, layer=lyr)
    cv.line(cx, Y_LV_BUS, cx, 574, stroke=col, sw=2.0, layer=lyr)

    # MCCB Breaker
    draw_breaker_v(cv, cx, 574, 626, col, lyr, drawout=False, state=None)
    cv.text(cx + 10, 594, f["q_id"], size=8.6, fill=C_DARK, weight="bold")
    cv.text(cx + 10, 606, f"{f['breaker_a']}A", size=8.2, fill=col, weight="bold")
    cv.text(cx + 10, 617, f"{int(f['icu_ka'])}kA", size=7.6, fill=C_MUT)

    # Cable run down to Load Box
    cv.line(cx, 626, cx, 742, stroke=col, sw=2.0, dash="5,2" if is_fire else None, layer=lyr, ltype="DASHED" if is_fire else "CONTINUOUS")
    draw_3ph_ticks(cv, cx, 643, col)

    # Cable annotation box centered on feeder stem
    cab_bg = "#FEF2F2" if is_fire else "#FFFFFF"
    cab_bd = C_FIRE if is_fire else "#CBD5E1"
    cv.rect(cx - 58, 658, 116, 62, stroke=cab_bd, fill=cab_bg, sw=1.0, rx=3, layer="ANNOTATIONS")
    c_type = "FR-XLPE" if is_fire else "Cu XLPE"
    cv.text(cx, 671, f"{f['runs']}×{f['mm2']}mm² {c_type}", size=8.0, fill=C_FIRE if is_fire else C_DARK, weight="bold", anchor="middle")
    cv.text(cx, 684, f"L = {f['length_m']} m", size=7.8, fill=C_MUT, anchor="middle")
    cv.text(cx, 697, f"Iz = {f['iz_a']:.0f} A", size=7.8, fill=C_MUT, anchor="middle")
    cv.text(cx, 710, f"ΔV = {f['vdrop_pct']:.2f}% (≤4%)", size=7.9, fill="#047857", weight="bold", anchor="middle")

    # Arrowhead into Load Box
    cv.polygon([(cx, 745), (cx - 4.5, 736), (cx + 4.5, 736)], stroke=col, fill=col, sw=1.0, layer=lyr)

    # Load Box
    bx, by, bw, bh = cx - 61, 745, 122, 96
    hdr_fill = C_FIRE if is_fire else (C_ESS if is_ess else C_NORM)
    body_fill = "#FFF5F5" if is_fire else ("#F0FDF4" if is_ess else "#EFF6FF")
    cv.rect(bx, by, bw, bh, stroke=hdr_fill, fill=body_fill, sw=1.4, rx=5, layer="EQUIPMENT")
    cv.rect(bx, by, bw, 20, stroke=hdr_fill, fill=hdr_fill, sw=1.0, rx=4, layer="EQUIPMENT")
    cv.text(cx, by + 14, f["id"], size=8.4, fill="#FFFFFF", weight="bold", anchor="middle")

    cv.text(cx, by + 34, f["label1"], size=8.3, fill=C_DARK, weight="bold", anchor="middle")
    cv.text(cx, by + 46, f["label2"], size=8.3, fill=C_DARK, weight="bold", anchor="middle")
    cv.line(bx + 8, by + 54, bx + bw - 8, by + 54, stroke="#CBD5E1", sw=0.8, layer="ANNOTATIONS")
    cv.text(cx, by + 68, f"{f['load_kw']} kW | PF {f['pf']:.2f}", size=8.3, fill=C_DARK, weight="bold", anchor="middle")
    cv.text(cx, by + 81, f"Ib = {f['ib_a']:.1f} A", size=8.0, fill=C_MUT, anchor="middle")

    # Special badges (Fire-Rated or Load Shedding Stage)
    if is_fire:
        cv.rect(cx - 44, by + 86, 88, 14, stroke=C_FIRE, fill="#FEE2E2", sw=0.9, rx=3, layer="ANNOTATIONS")
        cv.text(cx, by + 96, "FIRE-RATED 2hr", size=7.2, fill=C_FIRE, weight="bold", anchor="middle")
    elif f["shed"]:
        cv.rect(cx - 46, by + 86, 92, 14, stroke="#B45309", fill="#FEF3C7", sw=0.9, rx=3, layer="ANNOTATIONS")
        cv.text(cx, by + 96, f"BMS SHED {f['shed']} (N-1)", size=7.2, fill="#92400E", weight="bold", anchor="middle")

# --- 5.7 TOP-RIGHT ENGINEERING TABLES (IEC 60909 SC & BUS SCHEDULE) ---
TX0, TY0 = 748.0, 62.0
cv.rect(TX0, TY0, 565, 224, stroke=C_DARK, fill="#FFFFFF", sw=1.3, rx=4, layer="BORDER_TITLE")
cv.rect(TX0, TY0, 565, 22, stroke=C_DARK, fill="#1E293B", sw=1.0, layer="BORDER_TITLE")
cv.text(TX0 + 10, TY0 + 15, "IEC 60909-0 SHORT-CIRCUIT & SWITCHGEAR RATING VERIFICATION", size=9.2, fill="#FFFFFF", weight="bold")

sc_cols = [
    ("Bus / Node", 10), ("Un", 108), ("Mode / Source", 162), ("Ik'' (kA)", 320), ("Min Icu", 392), ("Status", 470)
]
cv.rect(TX0, TY0 + 22, 565, 19, stroke="#CBD5E1", fill="#F1F5F9", sw=0.8, layer="ANNOTATIONS")
for htxt, ox in sc_cols:
    cv.text(TX0 + ox, TY0 + 35, htxt, size=8.2, fill=C_DARK, weight="bold")

sc_rows = [
    ("RMU_11KV", "11 kV",  "Utility 250 MVA",             f"{ik_mv_ka:.2f}",        "25 kA", "PASS (1.9x)"),
    ("MSB_A",    "400 V",  "TX1 (1000 kVA, uk=6%)",       f"{ik_msb_1tx_ka:.2f}",   "50 kA", "PASS (2.1x)"),
    ("MSB_B",    "400 V",  "TX2 (1000 kVA, uk=6%)",       f"{ik_msb_1tx_ka:.2f}",   "50 kA", "PASS (2.1x)"),
    ("MSB A+B*", "400 V",  "2×TX Parallel (Blocked)",     f"{ik_msb_2tx_ka:.2f}",   "50 kA", "INTERLOCKED"),
    ("EMSB",     "400 V",  "Normal (from MSB_B / ATS1)",  f"{ik_emsb_norm_ka:.2f}", "36 kA", "PASS (1.5x)"),
    ("EMSB",     "400 V",  "Standby (GEN1, Xd''=15%)",    f"{ik_emsb_gen_ka:.2f}",  "36 kA", "PASS (3.5x)"),
]
for idx, r in enumerate(sc_rows):
    ry = TY0 + 41 + idx * 21
    bg = "#FFFFFF" if idx % 2 == 0 else "#F8FAFC"
    cv.rect(TX0, ry, 565, 21, stroke="#E2E8F0", fill=bg, sw=0.6, layer="ANNOTATIONS")
    for (_, ox), val in zip(sc_cols, r):
        c_val = "#047857" if "PASS" in val else (C_IL if "INTERLOCK" in val else C_DARK)
        w_val = "bold" if ("PASS" in val or "INTERLOCK" in val or ox == 10) else "normal"
        cv.text(TX0 + ox, ry + 14, val, size=8.1, fill=c_val, weight=w_val)

cv.text(TX0 + 10, TY0 + 184, f"IEC 60909 Params: c_max(MV)=1.10, c_max(LV)=1.05, KT={kt:.4f}, Rq/Xq=0.10, ip(MSB)={ip_msb_1tx_ka:.1f} kA.", size=7.8, fill=C_MUT)
cv.text(TX0 + 10, TY0 + 198, "IEC 60364-5-52 Cables: Cu XLPE 90°C, Method E perforated tray, k_derate=0.88 (35°C amb × grouping).", size=7.8, fill=C_MUT)
cv.text(TX0 + 10, TY0 + 212, "*TX1+TX2 paralleling prevented by IL_BUS_TIE (2-of-3); all LV breakers satisfy Icu >= Ik'' & Ib <= In <= Iz.", size=7.8, fill=C_MUT)

# --- 5.8 BOTTOM ENGINEERING PANELS (INTERLOCK LOGIC, N-1 PROOF, TITLE BLOCK) ---
BY0 = 868.0
BH  = 212.0

# Panel 1: Symbol Legend & Interlock Truth Table
cv.rect(40, BY0, 520, BH, stroke=C_DARK, fill="#FFFFFF", sw=1.3, rx=4, layer="BORDER_TITLE")
cv.rect(40, BY0, 520, 22, stroke=C_DARK, fill="#1E293B", sw=1.0, layer="BORDER_TITLE")
cv.text(50, BY0 + 15, "MANDATORY INTERLOCK LOGIC & IEC 60617 LEGEND", size=9.0, fill="#FFFFFF", weight="bold")

cv.text(52, BY0 + 38, "1. IL_BUS_TIE (2-of-3 Mechanical Kirk-Key + Electrical):", size=8.4, fill=C_IL, weight="bold")
cv.text(60, BY0 + 52, "• Normal Split:   Q_TX1=CLOSED, Q_TX2=CLOSED, Q_BT=OPEN", size=8.0, fill=C_DARK, mono=True)
cv.text(60, BY0 + 65, "• N-1 TX1 Out:    Q_TX1=OPEN,   Q_TX2=CLOSED, Q_BT=CLOSED", size=8.0, fill=C_DARK, mono=True)
cv.text(60, BY0 + 78, "• N-1 TX2 Out:    Q_TX1=CLOSED, Q_TX2=OPEN,   Q_BT=CLOSED", size=8.0, fill=C_DARK, mono=True)
cv.text(60, BY0 + 91, "• FORBIDDEN:      Q_TX1 + Q_TX2 + Q_BT all CLOSED (Blocked)", size=8.0, fill=C_FIRE, weight="bold", mono=True)

cv.text(52, BY0 + 112, "2. IL_ATS (ATS1 Mutually Exclusive Mech + Elec Interlock):", size=8.4, fill=C_IL, weight="bold")
cv.text(60, BY0 + 126, "• Normal Utility: Q_NORM=CLOSED, Q_GEN=OPEN (EMSB via MSB_B)", size=8.0, fill=C_DARK, mono=True)
cv.text(60, BY0 + 139, "• GEN1 Standby:   Q_NORM=OPEN,   Q_GEN=CLOSED (Open Transition)", size=8.0, fill=C_DARK, mono=True)
cv.text(60, BY0 + 152, "• FORBIDDEN:      Q_NORM + Q_GEN simultaneously CLOSED", size=8.0, fill=C_FIRE, weight="bold", mono=True)

# Color legend strip
cv.line(52, BY0 + 168, 548, BY0 + 168, stroke="#CBD5E1", sw=0.8, layer="ANNOTATIONS")
for lx, lcol, ltxt in [
    (54, C_MV, "11 kV MV"), (140, C_NORM, "400V Normal"), (246, C_ESS, "400V Essential"),
    (368, C_FIRE, "Fire-Rated"), (458, C_IL, "Interlock"),
]:
    cv.line(lx, BY0 + 186, lx + 18, BY0 + 186, stroke=lcol, sw=3.5, layer="ANNOTATIONS")
    cv.text(lx + 23, BY0 + 190, ltxt, size=8.0, fill=C_DARK, weight="bold")

# Panel 2: N-1 & Standby Contingency Proof
cv.rect(576, BY0, 560, BH, stroke=C_DARK, fill="#FFFFFF", sw=1.3, rx=4, layer="BORDER_TITLE")
cv.rect(576, BY0, 560, 22, stroke=C_DARK, fill="#1E293B", sw=1.0, layer="BORDER_TITLE")
cv.text(586, BY0 + 15, "N-1 TRANSFORMER & STANDBY GENERATOR CONTINGENCY PROOF", size=9.0, fill="#FFFFFF", weight="bold")

n1_rows = [
    ("Operating Scenario", "Active Sources", "Load (kVA)", "Limit", "Margin / Status"),
    ("Normal — TX1 (MSB_A, DF=0.70)",      "TX1 (Q_BT Open)",  f"{s_tx1_df07:.1f}",  "1000 kVA", f"{100*s_tx1_df07/1000:.1f}% — PASS"),
    ("Normal — TX2 (MSB_B+EMSB, DF=0.70)", "TX2 (Q_BT Open)",  f"{s_tx2_df07:.1f}",  "1000 kVA", f"{100*s_tx2_df07/1000:.1f}% — PASS"),
    ("Normal — TX2 (100% EMSB + 70% B)",   "TX2 (Worst Norm)", f"{s_tx2_100e:.1f}",  "1000 kVA", f"{100*s_tx2_100e/1000:.1f}% — PASS"),
    ("N-1 (1 TX, Shed S1: F8 Chiller1)",   "1×TX (Q_BT Cls)",  f"{s_n1_s1_df07:.1f}","1000 kVA", f"{100*s_n1_s1_df07/1000:.1f}% — PASS"),
    ("N-1 Peak (100% Coinc, Shed S1+S2)",  "1×TX (EMSB+F7)",   f"{s_n1_s2_peak:.1f}","1000 kVA", f"{100*s_n1_s2_peak/1000:.1f}% — PASS"),
    ("Utility Outage — GEN1 (100% EMSB)",  "GEN1 via ATS1",    f"{s_emsb:.1f}",      "1000 kVA", f"{100*p_emsb/GEN_KW:.1f}% kW — PASS"),
]
for idx, r in enumerate(n1_rows):
    ry = BY0 + 26 + idx * 23
    bg = "#F1F5F9" if idx == 0 else ("#FFFFFF" if idx % 2 == 1 else "#F8FAFC")
    cv.rect(576, ry, 560, 23, stroke="#E2E8F0", fill=bg, sw=0.6, layer="ANNOTATIONS")
    xs = [584, 784, 888, 956, 1024]
    for xi, val in zip(xs, r):
        c_val = "#047857" if "PASS" in val else C_DARK
        w_val = "bold" if (idx == 0 or "PASS" in val) else "normal"
        cv.text(xi, ry + 15, val, size=7.9, fill=c_val, weight=w_val)

cv.text(584, BY0 + 201, f"Total Connected Plant Load: {p_tot:.0f} kW / {s_tot:.1f} kVA  |  EMSB Essential: {p_emsb:.0f} kW / {s_emsb:.1f} kVA (<= 800 kW / 1000 kVA GEN1).",
        size=7.8, fill=C_MUT)

# Panel 3: Engineering Title Block
TBX = 1152.0
cv.rect(TBX, BY0, 488, BH, stroke=C_DARK, fill="#FFFFFF", sw=1.6, layer="BORDER_TITLE")
cv.rect(TBX, BY0, 488, 36, stroke=C_DARK, fill="#0F172A", sw=1.0, layer="BORDER_TITLE")
cv.text(TBX + 244, BY0 + 16, "REGIONAL HOSPITAL & MEDICAL CENTER", size=11.0, fill="#FFFFFF", weight="bold", anchor="middle")
cv.text(TBX + 244, BY0 + 30, "ELECTRICAL POWER SINGLE-LINE DIAGRAM (MV / LV)", size=9.0, fill="#93C5FD", weight="bold", anchor="middle")

tb_rows = [
    ("SYSTEM:", "11 kV / 400 V, 3Φ+N+PE, 50 Hz, TN-S"),
    ("TRANSFORMERS:", "2 × 1000 kVA, 11/0.4 kV, Dyn11, uk = 6.0% (N-1)"),
    ("STANDBY GEN:", "1 × 1000 kVA (800 kW), 400 V, 0.8 PF, Xd'' = 15%"),
    ("STANDARDS:", "IEC 60909-0 | IEC 60364-5-52 | IEC 60617 | IEC 61439"),
    ("CABLE DESIGN:", "Cu XLPE 90°C, ΔV_max = 1.00% (Spec <= 4.0%)"),
]
for idx, (k, v) in enumerate(tb_rows):
    ry = BY0 + 36 + idx * 24
    cv.line(TBX, ry + 24, TBX + 488, ry + 24, stroke="#CBD5E1", sw=0.8, layer="BORDER_TITLE")
    cv.text(TBX + 10, ry + 16, k, size=8.0, fill=C_MUT, weight="bold")
    cv.text(TBX + 108, ry + 16, v, size=8.2, fill=C_DARK, weight="bold")

# Bottom row of title block
ry_bot = BY0 + 156
cv.line(TBX + 150, ry_bot, TBX + 150, BY0 + BH, stroke="#CBD5E1", sw=0.8, layer="BORDER_TITLE")
cv.line(TBX + 290, ry_bot, TBX + 290, BY0 + BH, stroke="#CBD5E1", sw=0.8, layer="BORDER_TITLE")
cv.line(TBX + 390, ry_bot, TBX + 390, BY0 + BH, stroke="#CBD5E1", sw=0.8, layer="BORDER_TITLE")

cv.text(TBX + 10,  ry_bot + 16, "DRAWING NO.", size=7.3, fill=C_MUT)
cv.text(TBX + 10,  ry_bot + 36, "E-SLD-001", size=12.0, fill=C_DARK, weight="bold")
cv.text(TBX + 160, ry_bot + 16, "DISCIPLINE", size=7.3, fill=C_MUT)
cv.text(TBX + 160, ry_bot + 35, "Power Systems", size=10.0, fill=C_DARK, weight="bold")
cv.text(TBX + 300, ry_bot + 16, "SHEET / SCALE", size=7.3, fill=C_MUT)
cv.text(TBX + 300, ry_bot + 35, "1 of 1 / NTS", size=9.8, fill=C_DARK, weight="bold")
cv.text(TBX + 400, ry_bot + 16, "REVISION", size=7.3, fill=C_MUT)
cv.text(TBX + 400, ry_bot + 36, "REV A", size=12.0, fill=C_ESS, weight="bold")

# Write SVG & DXF
svg_path = OUT_DIR / "hospital_sld.svg"
dxf_path = OUT_DIR / "hospital_sld.dxf"
svg_path.write_text(cv.render_svg(), encoding="utf-8")
dxf_path.write_text(cv.render_dxf(), encoding="ascii")
print(f"Wrote {svg_path} ({svg_path.stat().st_size} bytes)")
print(f"Wrote {dxf_path} ({dxf_path.stat().st_size} bytes, {len(cv.dxf_entities)} entities)")
