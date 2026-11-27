#!/usr/bin/env python3
"""Task 54: LI_ION_CHARGER_FULL — Single-cell Li-Ion charger (BQ25895 + INA226 + TPD4E05U06)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t54(out_dir: str) -> str:
    sch = Schematic(
        title="Task 54: Li-Ion Battery Charger System (LI_ION_CHARGER_FULL)",
        project_name="t54_charger",
        paper="A3",
        comment1="BQ25895 5A I2C Li-Ion Buck Charger + INA226 16-bit Power Monitor + TPD4E05U06 USB ESD Protection",
        comment2="Inputs: VBUS (5V), D+, D-, GND | Outputs: BAT (4.2V), SYS",
    )

    # A3: 420 x 297 mm (usable ~25..394, ~25..244 above title block)
    sch.add_section_box("USB INPUT & ESD PROTECTION (TPD4E05U06)", 25.4, 25.4, 134.62, 243.84)
    sch.add_section_box("BQ25895 5A I2C LI-ION CHARGER & BUCK STAGE", 137.16, 25.4, 289.56, 243.84)
    sch.add_section_box("INA226 BATTERY POWER MONITOR & OUTPUTS", 292.1, 25.4, 393.7, 243.84)

    # =========================================================================
    # 1. USB INPUT CONNECTOR (J1) & ESD PROTECTION: TPD4E05U06 (U3) + VBUS TVS
    # =========================================================================
    j1 = sch.add_symbol("Connector_Generic:Conn_01x04", "J1", "USB_IN", 45.72, 119.38, mirror="y")
    sch.stub_label(j1, "1", "VBUS", 10.16)
    sch.stub_label(j1, "2", "D+", 10.16)
    sch.stub_label(j1, "3", "D-", 10.16)
    sch.pin_gnd(j1, "4", 5.08)

    sch.add_pwr_flag_on_net("GND", 40.64, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("VBUS", 40.64, 40.64, is_gnd=False)

    # VBUS TVS protection (SMBJ5.0A) + input bypass caps C1 (10uF), C2 (1uF)
    vbus_y = 76.2
    sch.wire((68.58, vbus_y), (127.0, vbus_y))
    sch.label("VBUS", (68.58, vbus_y), rot=0, justify="right bottom")
    sch.label("VBUS", (127.0, vbus_y), rot=0, justify="left bottom")
    d_vbus = sch.add_symbol("Device:D_TVS", "D1", "SMBJ5.0A", 86.36, 86.36, rot=90)
    c1 = sch.add_symbol("Device:C", "C1", "10uF", 101.6, 86.36)
    c2 = sch.add_symbol("Device:C", "C2", "1uF", 114.3, 86.36)
    sch.wire((86.36, vbus_y), d_vbus.pin("2"))
    sch.wire((101.6, vbus_y), c1.pin("1"))
    sch.wire((114.3, vbus_y), c2.pin("1"))
    sch.pin_gnd(d_vbus, "1", 2.54)
    sch.pin_gnd(c1, "2", 2.54)
    sch.pin_gnd(c2, "2", 2.54)

    # TPD4E05U06 (U3) protecting D+ and D- (flow-through D1+/D1- -> D2+/D2- per TI datasheet!)
    u3 = sch.add_symbol("test:TPD4E05U06", "U3", "TPD4E05U06", 96.52, 144.78)
    for nc_pin in ("10", "9", "7", "6"):
        sch.no_connect(u3.pin(nc_pin))
    # Stacked GND pins 3 & 8 are both at (96.52, 157.48)
    sch.pin_gnd(u3, "3", 5.08)

    # Connect D+ to D1+ (1) & D2+ (4), D- to D1- (2) & D2- (5) (flow-through ESD clamping)
    sch.wire(u3.pin("1"), (71.12, u3.pin("1")[1]))
    sch.label("D+", (71.12, u3.pin("1")[1]), rot=0, justify="right bottom")
    sch.wire(u3.pin("2"), (71.12, u3.pin("2")[1]))
    sch.label("D-", (71.12, u3.pin("2")[1]), rot=0, justify="right bottom")

    sch.wire(u3.pin("4"), (124.46, u3.pin("4")[1]))
    sch.label("D+", (124.46, u3.pin("4")[1]), rot=0, justify="left bottom")
    sch.wire(u3.pin("5"), (124.46, u3.pin("5")[1]))
    sch.label("D-", (124.46, u3.pin("5")[1]), rot=0, justify="left bottom")

    # Host I2C & Status Header J2 (I2C_SDA, I2C_SCL, CHG_INT, GND)
    j2 = sch.add_symbol("Connector_Generic:Conn_01x04", "J2", "HOST_I2C", 45.72, 208.28, mirror="y")
    sch.stub_label(j2, "1", "I2C_SCL", 10.16)
    sch.stub_label(j2, "2", "I2C_SDA", 10.16)
    sch.stub_label(j2, "3", "CHG_INT", 10.16)
    sch.pin_gnd(j2, "4", 5.08)

    # =========================================================================
    # 2. BQ25895 5A I2C LI-ION CHARGER (U1)
    # =========================================================================
    u1 = sch.add_symbol("test:BQ25895", "U1", "BQ25895", 205.74, 134.62)
    sch.no_connect(u1.pin("24"))  # DSEL (unused open-drain USB mux output)
    # Stacked PGND pins (17, 18, 25) share (205.74, 157.48)
    sch.pin_gnd(u1, "17", 7.62)

    # --- Left side pins of BQ25895 ---
    # Pin 1 VBUS
    p_vbus = u1.pin("1")
    sch.wire(p_vbus, (175.26, p_vbus[1]))
    sch.label("VBUS", (175.26, p_vbus[1]), rot=0, justify="right bottom")
    c_vbus = sch.add_symbol("Device:C", "C3", "1uF", 185.42, p_vbus[1] - 12.7)
    sch.wire((185.42, p_vbus[1]), c_vbus.pin("2"))
    sch.wire(c_vbus.pin("1"), (185.42, p_vbus[1] - 20.32))
    sch.add_power("power:GND", 185.42, p_vbus[1] - 22.86, rot=180)
    sch.wire((185.42, p_vbus[1] - 20.32), (185.42, p_vbus[1] - 22.86))

    # Pins 2 D+, 3 D-
    sch.wire(u1.pin("2"), (175.26, u1.pin("2")[1]))
    sch.label("D+", (175.26, u1.pin("2")[1]), rot=0, justify="right bottom")
    sch.wire(u1.pin("3"), (175.26, u1.pin("3")[1]))
    sch.label("D-", (175.26, u1.pin("3")[1]), rot=0, justify="right bottom")

    # Pins 5 SCL, 6 SDA with 4.7k pull-ups to SYS (valid 3.5-4.35V rail)
    p_scl = u1.pin("5")
    p_sda = u1.pin("6")
    sch.wire(p_scl, (147.32, p_scl[1]))
    sch.label("I2C_SCL", (147.32, p_scl[1]), rot=0, justify="right bottom")
    sch.wire(p_sda, (147.32, p_sda[1]))
    sch.label("I2C_SDA", (147.32, p_sda[1]), rot=0, justify="right bottom")

    r_scl = sch.add_symbol("Device:R", "R1", "4.7k", 167.64, 111.76)
    r_sda = sch.add_symbol("Device:R", "R2", "4.7k", 157.48, 111.76)
    sch.wire((167.64, p_scl[1]), r_scl.pin("2"))
    sch.wire((157.48, p_sda[1]), r_sda.pin("2"))
    sch.wire(r_scl.pin("1"), (167.64, 101.6))
    sch.wire(r_sda.pin("1"), (157.48, 101.6))
    sch.wire((157.48, 101.6), (167.64, 101.6))
    sch.label("SYS", (157.48, 101.6), rot=0, justify="right bottom")

    # Pin 7 ~{INT} -> 10k pull-up to SYS + label CHG_INT
    p_int = u1.pin("7")
    sch.wire(p_int, (147.32, p_int[1]))
    sch.label("CHG_INT", (147.32, p_int[1]), rot=0, justify="right bottom")
    r_int = sch.add_symbol("Device:R", "R3", "10k", 157.48, p_int[1] + 10.16)
    sch.wire((157.48, p_int[1]), r_int.pin("1"))
    sch.wire(r_int.pin("2"), (157.48, p_int[1] + 17.78))
    sch.label("SYS", (157.48, p_int[1] + 17.78), rot=0, justify="right bottom")

    # Pin 8 OTG (pull-down 10k to GND), Pin 9 ~{CE} (pull-down 10k to GND -> charging enabled)
    p_otg = u1.pin("8")
    p_ce = u1.pin("9")
    r_otg = sch.add_symbol("Device:R", "R4", "10k", 182.88, 165.1)
    sch.wire_poly([p_otg, (182.88, p_otg[1]), r_otg.pin("1")])
    sch.wire(p_ce, (182.88, p_ce[1]))
    sch.pin_gnd(r_otg, "2", 2.54)

    # Pin 12 ~{QON} -> 10k pull-up to SYS
    p_qon = u1.pin("12")
    r_qon = sch.add_symbol("Device:R", "R5", "10k", 167.64, p_qon[1] + 10.16)
    sch.wire_poly([p_qon, (167.64, p_qon[1]), r_qon.pin("1")])
    sch.wire(r_qon.pin("2"), (167.64, p_qon[1] + 17.78))
    sch.label("SYS", (167.64, p_qon[1] + 17.78), rot=0, justify="right bottom")

    # Pin 10 ILIM -> R6 (130R) to GND (I_LIM = 390/130 = 3.0A per BQ25895 datasheet)
    p_ilim = u1.pin("10")
    r_ilim = sch.add_symbol("Device:R", "R6", "130", 190.5, p_ilim[1] + 12.7)
    sch.wire_poly([p_ilim, (190.5, p_ilim[1]), r_ilim.pin("1")])
    sch.pin_gnd(r_ilim, "2", 2.54)

    # --- Right side pins of BQ25895 ---
    # Pin 4 STAT -> 1k R7 -> LED1 -> REGN (active-low charge status indicator)
    p_stat = u1.pin("4")  # y = 116.84
    r_stat = sch.add_symbol("Device:R", "R7", "1k", 236.22, p_stat[1] - 25.4, rot=90)
    led1 = sch.add_symbol("Device:LED", "D2", "CHG_STAT", 254.0, p_stat[1] - 25.4)
    # LED pin 1=K (left, -3.81), pin 2=A (right, +3.81) -> STAT pulls K low, A tied to REGN!
    sch.wire_poly([p_stat, (223.52, p_stat[1]), (223.52, p_stat[1] - 25.4), r_stat.pin("1")])
    sch.wire(r_stat.pin("2"), led1.pin("1"))
    sch.wire(led1.pin("2"), (269.24, p_stat[1] - 25.4))
    sch.label("REGN", (269.24, p_stat[1] - 25.4), rot=0, justify="left bottom")

    # Pin 23 PMID -> C4 (10uF) + C5 (10uF) + C6 (100nF) to GND (mandatory buck input bypass)
    p_pmid = u1.pin("23")  # y = 124.46
    pmid_y = 109.22
    sch.wire_poly([p_pmid, (228.6, p_pmid[1]), (228.6, pmid_y), (266.7, pmid_y)])
    c4 = sch.add_symbol("Device:C", "C4", "10uF", 241.3, pmid_y + 7.62)
    c5 = sch.add_symbol("Device:C", "C5", "10uF", 254.0, pmid_y + 7.62)
    c6 = sch.add_symbol("Device:C", "C6", "100nF", 266.7, pmid_y + 7.62)
    for c_sym, cx in ((c4, 241.3), (c5, 254.0), (c6, 266.7)):
        sch.wire((cx, pmid_y), c_sym.pin("1"))
        sch.pin_gnd(c_sym, "2", 2.54)

    # Pins 19, 20 SW (stacked at 215.9, 129.54) -> L1 (1uH) -> SYS (pins 15, 16 stacked at 215.9, 139.70)
    # Pin 21 BTST (215.9, 134.62) -> C7 (47nF bootstrap cap) -> SW!
    p_sw = u1.pin("19")
    p_btst = u1.pin("21")
    p_sys = u1.pin("15")
    p_bat = u1.pin("13")

    l1 = sch.add_symbol("Device:L", "L1", "1uH", 246.38, p_sw[1], rot=90)
    sch.wire(p_sw, l1.pin("1"))
    # Bootstrap cap C7 (47nF) between BTST and SW (left of L1)
    c_btst = sch.add_symbol("Device:C", "C7", "47nF", 231.14, round((p_sw[1] + p_btst[1]) / 2.0, 4))
    # Wait: p_sw y = 129.54, p_btst y = 134.62 (diff is 5.08mm, while cap height is 7.62mm!).
    # So place C7 horizontally at y = p_btst[1] (134.62) with rot=90, connecting BTST to SW via vertical drop!
    sch.symbols.pop()  # replace with clean horizontal C7
    c_btst = sch.add_symbol("Device:C", "C7", "47nF", 228.6, p_btst[1], rot=90)
    sch.wire(p_btst, c_btst.pin("1"))
    sch.wire_poly([c_btst.pin("2"), (236.22, p_btst[1]), (236.22, p_sw[1])])

    # L1 output connects to SYS bus (pins 15, 16) + C8 (22uF), C9 (22uF), C10 (100nF) to GND
    sys_y = p_sys[1]  # 139.70
    sch.wire_poly([l1.pin("2"), (259.08, p_sw[1]), (259.08, sys_y), (284.48, sys_y)])
    sch.wire(p_sys, (259.08, sys_y))
    sch.label("SYS", (284.48, sys_y), rot=0, justify="left bottom")

    c8 = sch.add_symbol("Device:C", "C8", "22uF", 266.7, sys_y + 12.7)
    c9 = sch.add_symbol("Device:C", "C9", "22uF", 279.4, sys_y + 12.7)
    sch.wire((266.7, sys_y), c8.pin("1"))
    sch.wire((279.4, sys_y), c9.pin("1"))
    sch.pin_gnd(c8, "2", 2.54)
    sch.pin_gnd(c9, "2", 2.54)

    # Pins 13, 14 BAT (stacked at 215.9, 142.24) -> BAT_CHG (to INA226 shunt!) with C11 (10uF) to GND
    bat_chg_y = 175.26
    sch.wire_poly([p_bat, (233.68, p_bat[1]), (233.68, bat_chg_y), (284.48, bat_chg_y)])
    sch.label("BAT_CHG", (284.48, bat_chg_y), rot=0, justify="left bottom")
    c11 = sch.add_symbol("Device:C", "C11", "10uF", 254.0, bat_chg_y + 10.16)
    sch.wire((254.0, bat_chg_y), c11.pin("1"))
    sch.pin_gnd(c11, "2", 2.54)

    # Pin 22 REGN (y = 147.32) -> C12 (4.7uF) to GND + label REGN
    p_regn = u1.pin("22")
    regn_y = 200.66
    sch.wire_poly([p_regn, (226.06, p_regn[1]), (226.06, regn_y), (248.92, regn_y)])
    sch.label("REGN", (248.92, regn_y), rot=0, justify="left bottom")
    c12 = sch.add_symbol("Device:C", "C12", "4.7uF", 236.22, regn_y + 10.16)
    sch.wire((236.22, regn_y), c12.pin("1"))
    sch.pin_gnd(c12, "2", 2.54)

    # Pin 11 TS (y = 152.40) -> TI BQ25895 JEITA NTC network: R8 (5.23k) to REGN, R9 (30.1k) || TH1 (10k NTC) to GND
    p_ts = u1.pin("11")
    ts_y = 220.98
    sch.wire_poly([p_ts, (220.98, p_ts[1]), (220.98, ts_y), (279.4, ts_y)])
    r_ts1 = sch.add_symbol("Device:R", "R8", "5.23k", 256.54, ts_y - 10.16)
    sch.wire((256.54, ts_y), r_ts1.pin("2"))
    sch.wire(r_ts1.pin("1"), (256.54, regn_y))
    sch.wire((248.92, regn_y), (256.54, regn_y))

    r_ts2 = sch.add_symbol("Device:R", "R9", "30.1k", 266.7, ts_y + 10.16)
    th1 = sch.add_symbol("Device:Thermistor_NTC", "TH1", "10k_NTC", 279.4, ts_y + 10.16)
    sch.wire((266.7, ts_y), r_ts2.pin("1"))
    sch.wire((279.4, ts_y), th1.pin("1"))
    sch.pin_gnd(r_ts2, "2", 2.54)
    sch.pin_gnd(th1, "2", 2.54)

    # =========================================================================
    # 3. INA226 16-BIT POWER MONITOR (U2) & BATTERY SHUNT + OUTPUT CONNECTORS
    # =========================================================================
    # High-side battery current shunt R_SHUNT (10mR) between BAT_CHG and BAT
    shunt_y = 76.2
    r_shunt = sch.add_symbol("Device:R", "R_SHUNT", "10mR", 342.9, shunt_y, rot=90)
    sch.wire((307.34, shunt_y), r_shunt.pin("1"))
    sch.label("BAT_CHG", (307.34, shunt_y), rot=0, justify="right bottom")
    sch.wire(r_shunt.pin("2"), (378.46, shunt_y))
    sch.label("BAT", (378.46, shunt_y), rot=0, justify="left bottom")
    c_bat_out = sch.add_symbol("Device:C", "C13", "10uF", 368.3, shunt_y + 10.16)
    sch.wire((368.3, shunt_y), c_bat_out.pin("1"))
    sch.pin_gnd(c_bat_out, "2", 2.54)

    # INA226 (U2) placed at (342.9, 144.78)
    u2 = sch.add_symbol("test:INA226", "U2", "INA226", 342.9, 144.78)

    # Left side of INA226: A1 (1), A0 (2) -> GND (I2C addr 0x40); SDA (3) -> I2C_SDA; SCL (4) -> I2C_SCL
    p_a1 = u2.pin("1")
    p_a0 = u2.pin("2")
    sch.wire(p_a1, (322.58, p_a1[1]))
    sch.wire(p_a0, (322.58, p_a0[1]))
    sch.wire((322.58, p_a1[1]), (322.58, p_a0[1]))
    sch.wire((322.58, p_a1[1]), (322.58, 134.62))
    sch.add_power("power:GND", 322.58, 132.08, rot=180)
    sch.wire((322.58, 134.62), (322.58, 132.08))

    sch.wire(u2.pin("3"), (309.88, u2.pin("3")[1]))
    sch.label("I2C_SDA", (309.88, u2.pin("3")[1]), rot=0, justify="right bottom")
    sch.wire(u2.pin("4"), (309.88, u2.pin("4")[1]))
    sch.label("I2C_SCL", (309.88, u2.pin("4")[1]), rot=0, justify="right bottom")

    # Right side of INA226: VIN+ (8), VIN- (7), GND (6), VS (5)
    p_vinp = u2.pin("8")  # y = 140.97
    p_vinm = u2.pin("7")  # y = 143.51
    p_u2_gnd = u2.pin("6") # y = 146.05
    p_u2_vs = u2.pin("5")  # y = 148.59

    # Kelvin RC filter: BAT_CHG -> R10 (10R) -> VIN+ (8); BAT -> R11 (10R) -> VIN- (7); C14 (100nF) across VIN+/VIN-
    r_fp = sch.add_symbol("Device:R", "R10", "10", 332.74, 114.3)
    r_fn = sch.add_symbol("Device:R", "R11", "10", 353.06, 114.3)
    # Connect top of R10 to BAT_CHG (left of R_SHUNT) and top of R11 to BAT (right of R_SHUNT)
    sch.wire_poly([(332.74, shunt_y), r_fp.pin("1")])
    sch.wire_poly([(353.06, shunt_y), r_fn.pin("1")])
    # Connect bottom of R10 -> VIN+ and bottom of R11 -> VIN-
    sch.wire_poly([r_fp.pin("2"), (332.74, 127.0), (368.3, 127.0), (368.3, p_vinp[1]), p_vinp])
    sch.wire_poly([r_fn.pin("2"), (353.06, 129.54), (375.92, 129.54), (375.92, p_vinm[1]), p_vinm])
    # Differential filter cap C14 (100nF) across VIN+ and VIN-
    c_flt = sch.add_symbol("Device:C", "C14", "100nF", 383.54, 134.62)
    sch.wire_poly([(368.3, 127.0), (383.54, 127.0), c_flt.pin("1")])
    sch.wire_poly([(375.92, p_vinm[1]), (383.54, p_vinm[1]), c_flt.pin("2")])

    # INA226 GND (6) -> GND, VS (5) -> SYS with C15 (100nF) decoupling
    sch.wire(p_u2_gnd, (365.76, p_u2_gnd[1]))
    sch.wire((365.76, p_u2_gnd[1]), (365.76, 154.94))
    sch.add_power("power:GND", 365.76, 154.94, rot=0)

    sch.wire_poly([p_u2_vs, (360.68, p_u2_vs[1]), (360.68, 165.1), (375.92, 165.1)])
    sch.label("SYS", (360.68, 165.1), rot=0, justify="right bottom")
    c15 = sch.add_symbol("Device:C", "C15", "100nF", 375.92, 172.72)
    sch.wire((375.92, 165.1), c15.pin("1"))
    sch.pin_gnd(c15, "2", 2.54)

    # Output Power Connectors J3 (BAT, GND) & J4 (SYS, GND)
    j3 = sch.add_symbol("Connector_Generic:Conn_01x02", "J3", "BAT_OUT", 381.0, 200.66)
    sch.stub_label(j3, "1", "BAT", 7.62)
    sch.pin_gnd(j3, "2", 5.08)

    j4 = sch.add_symbol("Connector_Generic:Conn_01x02", "J4", "SYS_OUT", 381.0, 226.06)
    sch.stub_label(j4, "1", "SYS", 7.62)
    sch.pin_gnd(j4, "2", 5.08)

    sch.add_note(
        "Design Notes (Task 54 - LI_ION_CHARGER_FULL):\n"
        "- TPD4E05U06 clamps USB D+/D- in flow-through routing; SMBJ5.0A TVS clamps VBUS.\n"
        "- BQ25895: 1uH buck inductor, 47nF BTST cap, 2x10uF+100nF PMID, 4.7uF REGN, R_ILIM=130R (3A), JEITA NTC (5.23k/30.1k/10k).\n"
        "- INA226 (I2C addr 0x40, shared I2C with BQ25895 0x6A): 10mR battery shunt with 10R/100nF Kelvin sense filter.",
        33.02,
        226.06,
        size=1.27,
    )

    return sch.write(out_dir, "t54_charger.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t54(out)
    print("Wrote:", p)
