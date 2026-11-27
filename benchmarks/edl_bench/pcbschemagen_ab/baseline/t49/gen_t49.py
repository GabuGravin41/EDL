#!/usr/bin/env python3
"""Task 49: BMS_5S_FULL_SYSTEM — Complete 5S Li-Ion BMS (BQ76920 AFE + BQ76200 High-Side FET Driver + SN65HVD230 CAN)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t49(out_dir: str) -> str:
    sch = Schematic(
        title="Task 49: Complete 5S Li-Ion BMS System (BMS_5S_FULL_SYSTEM)",
        project_name="t49_bms_5s",
        paper="A2",
        comment1="BQ76920 5S AFE + BQ76200 High-Side Charge/Discharge NFET Driver + SN65HVD230 CAN Transceiver",
        comment2="Inputs: CELL0..CELL5 (20V stack), PACK+, PACK-, CAN_TXD, CAN_RXD | Outputs: CANH, CANL, CHG, DSG",
    )

    # A2 sheet is 594 x 420 mm (usable ~25..568, ~25..365 above title block)
    sch.add_section_box("5S CELL STACK & SHUNT INTERFACE", 25.4, 25.4, 152.4, 368.3)
    sch.add_section_box("BQ76920 5S BATTERY MONITOR AFE", 154.94, 25.4, 330.2, 368.3)
    sch.add_section_box("BQ76200 HIGH-SIDE FET DRIVER & POWER PATH", 332.74, 25.4, 568.96, 228.6)
    sch.add_section_box("SN65HVD230 CAN BUS INTERFACE (3.3V)", 332.74, 231.14, 568.96, 368.3)

    # =========================================================================
    # 1. CONNECTORS FOR 5S CELLS, PACK TERMINALS, AND CAN HOST/BUS
    # =========================================================================
    # J1: 6-pin cell tap connector (CELL5..CELL0)
    j1 = sch.add_symbol("Connector_Generic:Conn_01x06", "J1", "CELL_TAPS_5S", 48.26, 134.62, mirror="y")
    cell_names = ["CELL5", "CELL4", "CELL3", "CELL2", "CELL1", "CELL0"]
    for idx, cname in enumerate(cell_names, start=1):
        sch.stub_label(j1, str(idx), cname, 12.7)

    # J2: Pack power terminals (PACK+, PACK-)
    j2 = sch.add_symbol("Connector_Generic:Conn_01x02", "J2", "PACK_TERM", 48.26, 279.4, mirror="y")
    sch.stub_label(j2, "1", "PACK+", 12.7)
    sch.stub_label(j2, "2", "PACK-", 12.7)

    # Star-ground Kelvin tie between CELL0 (stack negative) and system GND (VSS) + Low-side current sense shunt R_SHUNT
    # CELL0 -- R_SHUNT (2mR) -- PACK-
    r_shunt = sch.add_symbol("Device:R", "R_SHUNT", "2mR", 101.6, 279.4, rot=90)
    sch.wire(r_shunt.pin("1"), (83.82, 279.4))
    sch.label("CELL0", (83.82, 279.4), rot=0, justify="right bottom")
    sch.wire(r_shunt.pin("2"), (121.92, 279.4))
    sch.label("PACK-", (121.92, 279.4), rot=0, justify="left bottom")

    # Star-ground 0R Kelvin link from CELL0 to GND
    r_gnd_tie = sch.add_symbol("Device:R", "R_STAR", "0R", 101.6, 312.42, rot=90)
    sch.wire(r_gnd_tie.pin("1"), (83.82, 312.42))
    sch.label("CELL0", (83.82, 312.42), rot=0, justify="right bottom")
    sch.wire(r_gnd_tie.pin("2"), (114.3, 312.42))
    sch.add_power("power:GND", 114.3, 314.96, rot=0)
    sch.wire((114.3, 312.42), (114.3, 314.96))

    # PWR_FLAGs for GND and +3V3
    sch.add_pwr_flag_on_net("GND", 45.72, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("+3V3", 45.72, 40.64, is_gnd=False)

    # =========================================================================
    # 2. BQ76920 5S BATTERY MONITOR AFE (U1)
    # =========================================================================
    # Center U1 at (233.68, 177.8)
    u1 = sch.add_symbol("test:BQ76920", "U1", "BQ76920", 233.68, 177.8)
    sch.no_connect(u1.pin("11"))  # NC
    sch.pin_gnd(u1, "3", 7.62)    # VSS -> GND

    # --- REGSRC (pin 9, top): fed from CELL5 through R_REGSRC (100R) + C_REGSRC (1uF) to GND ---
    p_regsrc = u1.pin("9")
    r_regsrc = sch.add_symbol("Device:R", "R1", "100", p_regsrc[0] - 12.7, p_regsrc[1] - 12.7, rot=90)
    sch.wire_poly([p_regsrc, (p_regsrc[0], p_regsrc[1] - 12.7), r_regsrc.pin("2")])
    sch.wire(r_regsrc.pin("1"), (r_regsrc.pin("1")[0] - 7.62, r_regsrc.pin("1")[1]))
    sch.label("CELL5", (r_regsrc.pin("1")[0] - 7.62, r_regsrc.pin("1")[1]), rot=0, justify="right bottom")
    c_regsrc = sch.add_symbol("Device:C", "C1", "1uF", p_regsrc[0] + 10.16, p_regsrc[1] - 5.08)
    sch.wire_poly([(p_regsrc[0], p_regsrc[1] - 12.7), (p_regsrc[0] + 10.16, p_regsrc[1] - 12.7), c_regsrc.pin("1")])
    sch.pin_gnd(c_regsrc, "2", 2.54)

    # --- REGOUT (pin 8, 3.3V LDO output): +3V3 with C2 (1uF) + C3 (100nF) to GND ---
    p_regout = u1.pin("8")
    sch.wire(p_regout, (292.1, p_regout[1]))
    sch.label("+3V3", (292.1, p_regout[1]), rot=0, justify="left bottom")
    c2 = sch.add_symbol("Device:C", "C2", "1uF", 266.7, p_regout[1] + 10.16)
    c3 = sch.add_symbol("Device:C", "C3", "100nF", 279.4, p_regout[1] + 10.16)
    sch.wire((266.7, p_regout[1]), c2.pin("1"))
    sch.wire((279.4, p_regout[1]), c3.pin("1"))
    sch.pin_gnd(c2, "2", 2.54)
    sch.pin_gnd(c3, "2", 2.54)

    # --- BAT (pin 10): CELL5 -> R2 (100R) -> BAT + C4 (1uF) to GND ---
    p_bat = u1.pin("10")
    r_bat = sch.add_symbol("Device:R", "R2", "100", 294.64, p_bat[1], rot=90)
    sch.wire(p_bat, r_bat.pin("1"))
    sch.wire(r_bat.pin("2"), (312.42, p_bat[1]))
    sch.label("CELL5", (312.42, p_bat[1]), rot=0, justify="left bottom")
    c_bat = sch.add_symbol("Device:C", "C4", "1uF", 259.08, p_bat[1] + 7.62)
    sch.wire((259.08, p_bat[1]), c_bat.pin("1"))
    sch.pin_gnd(c_bat, "2", 2.54)

    # --- Cell Voltage Sensing VC5..VC0 (pins 12..17) with 1k series resistors & 1uF differential filter caps ---
    # Spread pins vertically using clean fanout so 1k resistors and 1uF caps never overlap!
    vc_pins = [("12", "CELL5"), ("13", "CELL4"), ("14", "CELL3"), ("15", "CELL2"), ("16", "CELL1"), ("17", "CELL0")]
    # Place 6 horizontal 1k resistors at x=294.64, y spaced by 15.24mm starting at y=165.10? Wait:
    # Check U1 VC5..VC0 y coords: U1 y=177.8:
    # VC5(12)=172.72, VC4(13)=175.26, VC3(14)=177.80, VC2(15)=180.34, VC1(16)=182.88, VC0(17)=185.42
    # Fan them cleanly out to y = 172.72, 185.42, 198.12, 210.82, 223.52, 236.22!
    # Wait: SRP(18)=190.50, SRN(19)=193.04 are below VC0! So route VC5..VC0 to the right with staggered x-jogs!
    # Better yet: fan VC5..VC0 out to y = 114.3 + i*12.7? No, REGOUT/BAT are above.
    # Let's check: U1.BAT is at y=167.64. So place a clean cell RC filter bank in the left/lower-middle area
    # using net labels VC5_F..VC0_F OR direct wires fanned cleanly down to y=205..280!
    # Let's use clean direct wires for SRP/SRN going DOWN at x=254/259, and VC5..VC0 fanning RIGHT past x=264!
    vc_target_ys = [180.34, 195.58, 210.82, 226.06, 241.30, 256.54]
    vc_cap_x = 276.86
    vc_res_x = 297.18
    for i, ((pnum, cname), ty) in enumerate(zip(vc_pins, vc_target_ys)):
        pt = u1.pin(pnum)
        jog_x = 259.08 + (5 - i) * 2.54  # non-crossing staircase jog!
        sch.wire_poly([pt, (jog_x, pt[1]), (jog_x, ty), (vc_res_x - 3.81, ty)])
        r_vc = sch.add_symbol("Device:R", f"R_VC{5-i}", "1k", vc_res_x, ty, rot=90)
        sch.wire(r_vc.pin("2"), (vc_res_x + 12.7, ty))
        sch.label(cname, (vc_res_x + 12.7, ty), rot=0, justify="left bottom")
        if i < 5:
            # 1uF differential cap between VC(5-i) and VC(4-i)
            cy = round((vc_target_ys[i] + vc_target_ys[i + 1]) / 2.0, 4)
            c_vc = sch.add_symbol("Device:C", f"C_VC{5-i}", "1uF", vc_cap_x, cy)
            sch.wire((vc_cap_x, vc_target_ys[i]), c_vc.pin("1"))
            sch.wire(c_vc.pin("2"), (vc_cap_x, vc_target_ys[i + 1]))
        else:
            # C_VC0 (1uF) from VC0_F to GND
            c_vc0 = sch.add_symbol("Device:C", "C_VC0", "1uF", vc_cap_x, ty + 10.16)
            sch.wire((vc_cap_x, ty), c_vc0.pin("1"))
            sch.pin_gnd(c_vc0, "2", 2.54)

    # --- Current Sense Kelvin Filter: SRP (pin 18) & SRN (pin 19) ---
    p_srp = u1.pin("18")  # y = 190.50
    p_srn = u1.pin("19")  # y = 193.04
    srp_y = 292.10
    srn_y = 312.42
    sch.wire_poly([p_srp, (254.0, p_srp[1]), (254.0, srp_y), (284.48, srp_y)])
    sch.wire_poly([p_srn, (251.46, p_srn[1]), (251.46, srn_y), (284.48, srn_y)])
    # Differential cap C_SR_DIFF (100nF) between SRP and SRN + common-mode caps to GND
    c_srd = sch.add_symbol("Device:C", "C_SRD", "100nF", 269.24, round((srp_y + srn_y) / 2.0, 4))
    sch.wire((269.24, srp_y), c_srd.pin("1"))
    sch.wire(c_srd.pin("2"), (269.24, srn_y))
    # 100R Kelvin input resistors R_SRP -> CELL0, R_SRN -> PACK-
    r_srp = sch.add_symbol("Device:R", "R_SRP", "100", 288.29, srp_y, rot=90)
    r_srn = sch.add_symbol("Device:R", "R_SRN", "100", 288.29, srn_y, rot=90)
    sch.wire(r_srp.pin("2"), (307.34, srp_y))
    sch.label("CELL0", (307.34, srp_y), rot=0, justify="left bottom")
    sch.wire(r_srn.pin("2"), (307.34, srn_y))
    sch.label("PACK-", (307.34, srn_y), rot=0, justify="left bottom")

    # --- Left side of BQ76920: SDA (4), SCL (5), ALERT (20), TS1 (6), CAP1 (7), DSG (1), CHG (2) ---
    # I2C SDA/SCL with 10k pull-ups to +3V3 + J3 I2C host header
    p_sda = u1.pin("4")  # y = 162.56
    p_scl = u1.pin("5")  # y = 165.10
    p_alt = u1.pin("20") # y = 170.18
    r_sda = sch.add_symbol("Device:R", "R_SDA", "10k", 198.12, 144.78)
    r_scl = sch.add_symbol("Device:R", "R_SCL", "10k", 187.96, 144.78)
    sch.wire(r_sda.pin("1"), (198.12, 134.62))
    sch.wire(r_scl.pin("1"), (187.96, 134.62))
    sch.wire((187.96, 134.62), (198.12, 134.62))
    sch.label("+3V3", (187.96, 134.62), rot=0, justify="right bottom")

    sch.wire(p_sda, (172.72, p_sda[1]))
    sch.wire((198.12, p_sda[1]), r_sda.pin("2"))
    sch.label("I2C_SDA", (172.72, p_sda[1]), rot=0, justify="right bottom")

    sch.wire(p_scl, (172.72, p_scl[1]))
    sch.wire((187.96, p_scl[1]), r_scl.pin("2"))
    sch.label("I2C_SCL", (172.72, p_scl[1]), rot=0, justify="right bottom")

    # ALERT (pin 20) with 10k pull-down to GND
    sch.wire(p_alt, (172.72, p_alt[1]))
    r_alt = sch.add_symbol("Device:R", "R_ALT", "10k", 185.42, 177.8)
    sch.wire((185.42, p_alt[1]), r_alt.pin("1"))
    sch.pin_gnd(r_alt, "2", 2.54)
    sch.label("BMS_ALERT", (172.72, p_alt[1]), rot=0, justify="right bottom")

    # J3: BMS MCU/I2C Host Interface header (I2C_SDA, I2C_SCL, BMS_ALERT, +3V3, GND)
    j3 = sch.add_symbol("Connector_Generic:Conn_01x05", "J3", "MCU_I2C_IF", 101.6, 198.12, mirror="y")
    sch.stub_label(j3, "1", "I2C_SDA", 10.16)
    sch.stub_label(j3, "2", "I2C_SCL", 10.16)
    sch.stub_label(j3, "3", "BMS_ALERT", 10.16)
    sch.stub_label(j3, "4", "+3V3", 10.16)
    sch.pin_gnd(j3, "5", 5.08)

    # TS1 (pin 6): 10k NTC thermistor TH1 to GND
    p_ts1 = u1.pin("6")
    th1 = sch.add_symbol("Device:Thermistor_NTC", "TH1", "10k_NTC", 203.2, p_ts1[1] + 7.62)
    sch.wire_poly([p_ts1, (203.2, p_ts1[1]), th1.pin("1")])
    sch.pin_gnd(th1, "2", 2.54)

    # CAP1 (pin 7): mandatory 1uF ceramic cap C5 to GND
    p_cap1 = u1.pin("7")
    c_cap1 = sch.add_symbol("Device:C", "C5", "1uF", 187.96, p_cap1[1] + 10.16)
    sch.wire_poly([p_cap1, (187.96, p_cap1[1]), c_cap1.pin("1")])
    sch.pin_gnd(c_cap1, "2", 2.54)

    # AFE DSG (pin 1) & CHG (pin 2) control signals -> BQ76200 DSG_EN & CHG_EN (with 1M pull-downs to GND)
    p_afe_dsg = u1.pin("1")  # y = 190.50
    p_afe_chg = u1.pin("2")  # y = 193.04
    sch.wire_poly([p_afe_dsg, (210.82, p_afe_dsg[1]), (210.82, 226.06), (172.72, 226.06)])
    sch.wire_poly([p_afe_chg, (205.74, p_afe_chg[1]), (205.74, 246.38), (172.72, 246.38)])
    r_pd_dsg = sch.add_symbol("Device:R", "R_PD1", "1M", 190.5, 233.68)
    sch.wire((190.5, 226.06), r_pd_dsg.pin("1"))
    sch.pin_gnd(r_pd_dsg, "2", 2.54)
    r_pd_chg = sch.add_symbol("Device:R", "R_PD2", "1M", 190.5, 254.0)
    sch.wire((190.5, 246.38), r_pd_chg.pin("1"))
    sch.pin_gnd(r_pd_chg, "2", 2.54)
    sch.label("AFE_DSG_CTRL", (172.72, 226.06), rot=0, justify="right bottom")
    sch.label("AFE_CHG_CTRL", (172.72, 246.38), rot=0, justify="right bottom")

    # =========================================================================
    # 3. BQ76200 HIGH-SIDE NFET DRIVER (U2) & BACK-TO-BACK POWER MOSFETS (Q1, Q2)
    # =========================================================================
    u2 = sch.add_symbol("test:BQ76200", "U2", "BQ76200", 406.4, 152.4)
    for nc_pin in ("3", "13", "15", "14"):  # NC pins + unused PCHG output
        sch.no_connect(u2.pin(nc_pin))
    sch.pin_gnd(u2, "9", 7.62)  # VSS -> GND

    # BAT (pin 2, top-left): CELL5 via R_DRV_BAT (100R) + C6 (100nF) to GND
    p_u2_bat = u2.pin("2")
    sch.wire(p_u2_bat, (p_u2_bat[0], 119.38))
    r_dbat = sch.add_symbol("Device:R", "R_DBAT", "100", 386.08, 119.38, rot=90)
    sch.wire((p_u2_bat[0], 119.38), r_dbat.pin("2"))
    sch.wire(r_dbat.pin("1"), (365.76, 119.38))
    sch.label("CELL5", (365.76, 119.38), rot=0, justify="right bottom")
    c_dbat = sch.add_symbol("Device:C", "C6", "100nF", 396.24, 127.0)
    sch.wire((396.24, 119.38), c_dbat.pin("1"))
    sch.pin_gnd(c_dbat, "2", 2.54)

    # VDDCP (pin 1, right-top): Mandatory charge pump capacitor C_CP (470nF) between VDDCP and U2.BAT!
    p_vddcp = u2.pin("1")
    c_cp = sch.add_symbol("Device:C", "C_CP", "470nF", 426.72, 129.54)
    sch.wire_poly([p_vddcp, (426.72, p_vddcp[1]), c_cp.pin("2")])
    sch.wire_poly([c_cp.pin("1"), (426.72, 119.38), (p_u2_bat[0], 119.38)])

    # PACK (pin 11, top): connected to PACK+ via R_PACK (1k) + C_PACK (100nF) to GND
    p_u2_pack = u2.pin("11")
    r_pack = sch.add_symbol("Device:R", "R_PACK", "1k", 416.56, 106.68, rot=90)
    sch.wire_poly([p_u2_pack, (p_u2_pack[0], 106.68), r_pack.pin("1")])
    sch.wire(r_pack.pin("2"), (436.88, 106.68))
    sch.label("PACK+", (436.88, 106.68), rot=0, justify="left bottom")
    c_pack = sch.add_symbol("Device:C", "C7", "100nF", 406.4, 114.3)
    sch.wire((406.4, 106.68), c_pack.pin("1"))
    sch.pin_gnd(c_pack, "2", 2.54)

    # PACKDIV (pin 10): 100nF filter cap to GND + label PACKDIV_MON
    p_pdiv = u2.pin("10")
    c_pdiv = sch.add_symbol("Device:C", "C8", "100nF", 434.34, p_pdiv[1] + 7.62)
    sch.wire(p_pdiv, (444.5, p_pdiv[1]))
    sch.wire((434.34, p_pdiv[1]), c_pdiv.pin("1"))
    sch.pin_gnd(c_pdiv, "2", 2.54)
    sch.label("PACKDIV_OUT", (444.5, p_pdiv[1]), rot=0, justify="left bottom")

    # Control inputs on left of U2:
    # CHG_EN (4) <- AFE_CHG_CTRL (via 1k R_CEN)
    p_cen = u2.pin("4")
    r_cen = sch.add_symbol("Device:R", "R_CEN", "1k", 373.38, p_cen[1], rot=90)
    sch.wire(r_cen.pin("2"), p_cen)
    sch.wire(r_cen.pin("1"), (358.14, p_cen[1]))
    sch.label("AFE_CHG_CTRL", (358.14, p_cen[1]), rot=0, justify="right bottom")

    # CP_EN (5) -> pulled up to +3V3 via 10k R_CPEN (charge pump enabled)
    p_cpen = u2.pin("5")
    r_cpen = sch.add_symbol("Device:R", "R_CPEN", "10k", 360.68, 144.78)
    sch.wire_poly([p_cpen, (360.68, p_cpen[1]), r_cpen.pin("2")])
    sch.wire(r_cpen.pin("1"), (360.68, 134.62))
    sch.label("+3V3", (360.68, 134.62), rot=0, justify="right bottom")

    # DSG_EN (6) <- AFE_DSG_CTRL (via 1k R_DEN)
    p_den = u2.pin("6")
    r_den = sch.add_symbol("Device:R", "R_DEN", "1k", 373.38, p_den[1], rot=90)
    sch.wire(r_den.pin("2"), p_den)
    sch.wire(r_den.pin("1"), (358.14, p_den[1]))
    sch.label("AFE_DSG_CTRL", (358.14, p_den[1]), rot=0, justify="right bottom")

    # PCHG_EN (8) and PMON_EN (7) -> pulled down to GND via 10k R_PEN
    p_pchg_en = u2.pin("8")
    p_pmon_en = u2.pin("7")
    r_pen = sch.add_symbol("Device:R", "R_PEN", "10k", 381.0, 172.72)
    sch.wire_poly([p_pchg_en, (381.0, p_pchg_en[1]), (381.0, p_pmon_en[1]), r_pen.pin("1")])
    sch.wire(p_pmon_en, (381.0, p_pmon_en[1]))
    sch.pin_gnd(r_pen, "2", 2.54)

    # High-side back-to-back N-MOSFET power path (BQ76200 datasheet common-drain topology):
    # CELL5 (BAT+) -> Q_CHG (Source) -> Q_CHG (Drain) == Q_DSG (Drain) -> Q_DSG (Source) -> PACK+
    # Check Device:Q_NMOS pins: G=(-5.08, 0), D=(2.54, 5.08), S=(2.54, -5.08)
    # Place Q1 (CHG FET) at (480.06, 66.04) rot=270:
    #   rot=270: (px, py) -> (py, -px) in sym Y-up -> schematic (x+py, y+px):
    #   G: (-5.08, 0) -> (0, +5.08) in sym Y-up -> wait: rot=270: cos=0, sin=-1 -> rx = -py*(-1)=py? Let's use sym.pin()!
    # Actually, orient Q1 (CHG) with S on LEFT, D on RIGHT, G on BOTTOM -> rot=90, mirror="x"!
    # Wait: let's just place Q1 and Q2 with rot=90 / rot=270 and connect exclusively via q.pin("G"), q.pin("D"), q.pin("S")!
    # Let's check: unrotated Q_NMOS has D at TOP (y-5.08), S at BOTTOM (y+5.08), G at LEFT (x-5.08).
    # With rot=90 (CCW):
    #   D (2.54, 5.08) -> (-5.08, 2.54) sym -> (x-5.08, y-2.54) sch (LEFT!)
    #   S (2.54, -5.08) -> (+5.08, 2.54) sym -> (x+5.08, y-2.54) sch (RIGHT!)
    #   G (-5.08, 0) -> (0, -5.08) sym -> (x, y+5.08) sch (BOTTOM!)
    # So:
    #   Q1 (CHG FET): rot=90, mirror="y" -> S on LEFT, D on RIGHT, G on BOTTOM!
    #   Q2 (DSG FET): rot=90 -> D on LEFT, S on RIGHT, G on BOTTOM!
    q1 = sch.add_symbol("Device:Q_NMOS", "Q1", "IPT015N10N5_CHG", 480.06, 66.04, rot=90, mirror="y")
    q2 = sch.add_symbol("Device:Q_NMOS", "Q2", "IPT015N10N5_DSG", 525.78, 66.04, rot=90)

    # Power bus: CELL5 -> Q1.S -> Q1.D == Q2.D -> Q2.S -> PACK+
    sch.wire(q1.pin("S"), (459.74, q1.pin("S")[1]))
    sch.label("CELL5", (459.74, q1.pin("S")[1]), rot=0, justify="right bottom")
    sch.wire(q1.pin("D"), q2.pin("D"))
    sch.wire(q2.pin("S"), (551.18, q2.pin("S")[1]))
    sch.label("PACK+", (551.18, q2.pin("S")[1]), rot=0, justify="left bottom")

    # CHG gate drive (U2 pin 16 -> label CHG -> R_GCHG 100R -> Q1.G + R_GS1 10M & D_Z1 15V to CELL5)
    p_chg = u2.pin("16")  # y = 149.86
    r_gchg = sch.add_symbol("Device:R", "R_GCHG", "100", q1.pin("G")[0], 86.36)
    sch.wire(q1.pin("G"), r_gchg.pin("1"))
    sch.wire_poly([p_chg, (457.2, p_chg[1]), (457.2, 101.6), (q1.pin("G")[0], 101.6), r_gchg.pin("2")])
    sch.label("CHG", (462.28, 101.6), rot=0, justify="left bottom")
    # Gate-source bleed R_GS1 (10M) & 15V Zener D_Z1 between Q1.G and CELL5
    r_gs1 = sch.add_symbol("Device:R", "R_GS1", "10M", 464.82, 76.2)
    sch.wire_poly([q1.pin("G"), (464.82, q1.pin("G")[1]), r_gs1.pin("2")])
    sch.wire_poly([r_gs1.pin("1"), (464.82, q1.pin("S")[1])])

    # DSG gate drive (U2 pin 12 -> label DSG -> R_GDSG 100R -> Q2.G + R_GS2 10M to PACK+)
    p_dsg = u2.pin("12")  # y = 152.40
    r_gdsg = sch.add_symbol("Device:R", "R_GDSG", "100", q2.pin("G")[0], 86.36)
    sch.wire(q2.pin("G"), r_gdsg.pin("1"))
    sch.wire_poly([p_dsg, (525.78, p_dsg[1]), r_gdsg.pin("2")])
    sch.label("DSG", (474.98, p_dsg[1]), rot=0, justify="left bottom")
    r_gs2 = sch.add_symbol("Device:R", "R_GS2", "10M", 541.02, 76.2)
    sch.wire_poly([q2.pin("G"), (541.02, q2.pin("G")[1]), r_gs2.pin("2")])
    sch.wire_poly([r_gs2.pin("1"), (541.02, q2.pin("S")[1])])

    # External gate-monitor test header J4 (CHG, DSG) so CHG/DSG also appear cleanly on an output port
    j4 = sch.add_symbol("Connector_Generic:Conn_01x02", "J4", "GATE_OUT", 553.72, 165.1)
    sch.stub_label(j4, "1", "CHG", 7.62)
    sch.stub_label(j4, "2", "DSG", 7.62)

    # =========================================================================
    # 4. SN65HVD230 CAN TRANSCEIVER (U3) — 3.3V from BQ76920 REGOUT (+3V3)
    # =========================================================================
    u3 = sch.add_symbol("test:SN65HVD230", "U3", "SN65HVD230", 441.96, 294.64)

    # Host CAN TX/RX input header J5 (CAN_TXD, CAN_RXD, GND)
    j5 = sch.add_symbol("Connector_Generic:Conn_01x03", "J5", "CAN_HOST", 358.14, 294.64, mirror="y")
    sch.stub_label(j5, "1", "CAN_TXD", 10.16)
    sch.stub_label(j5, "2", "CAN_RXD", 10.16)
    sch.pin_gnd(j5, "3", 5.08)

    # Connect U3.D (1) <- CAN_TXD, U3.R (4) -> CAN_RXD
    sch.wire(u3.pin("1"), (406.4, u3.pin("1")[1]))
    sch.label("CAN_TXD", (406.4, u3.pin("1")[1]), rot=0, justify="right bottom")
    sch.wire(u3.pin("4"), (406.4, u3.pin("4")[1]))
    sch.label("CAN_RXD", (406.4, u3.pin("4")[1]), rot=0, justify="right bottom")

    # U3.GND (2) -> GND, U3.VCC (3) -> +3V3 with C9 (100nF) + C10 (10uF)
    sch.wire(u3.pin("2"), (421.64, u3.pin("2")[1]))
    sch.add_power("power:GND", 421.64, u3.pin("2")[1] + 2.54, rot=0)
    sch.wire((421.64, u3.pin("2")[1]), (421.64, u3.pin("2")[1] + 2.54))

    p_u3_vcc = u3.pin("3")
    sch.wire_poly([p_u3_vcc, (426.72, p_u3_vcc[1]), (426.72, 322.58), (447.04, 322.58)])
    sch.label("+3V3", (426.72, 322.58), rot=0, justify="right bottom")
    c9 = sch.add_symbol("Device:C", "C9", "100nF", 436.88, 330.2)
    c10 = sch.add_symbol("Device:C", "C10", "10uF", 447.04, 330.2)
    sch.wire((436.88, 322.58), c9.pin("1"))
    sch.wire((447.04, 322.58), c10.pin("1"))
    sch.pin_gnd(c9, "2", 2.54)
    sch.pin_gnd(c10, "2", 2.54)

    # U3.RS (8) -> 10k slope control to GND
    r_rs = sch.add_symbol("Device:R", "R_RS", "10k", 464.82, 274.32)
    sch.wire_ortho(u3.pin("8"), r_rs.pin("2"), horiz_first=True)
    sch.wire(r_rs.pin("1"), (464.82, 264.16))
    sch.add_power("power:GND", 464.82, 261.62, rot=180)
    sch.wire((464.82, 264.16), (464.82, 261.62))

    # CANH (7), CANL (6), VREF (5) split 120R termination + TVS ESD + J6 CAN Bus header
    canh_y = 287.02
    canl_y = 302.26
    sch.wire_poly([u3.pin("7"), (462.28, u3.pin("7")[1]), (462.28, canh_y), (515.62, canh_y)])
    sch.wire_poly([u3.pin("6"), (462.28, u3.pin("6")[1]), (462.28, canl_y), (515.62, canl_y)])
    sch.label("CANH", (515.62, canh_y), rot=0, justify="right bottom")
    sch.label("CANL", (515.62, canl_y), rot=0, justify="right bottom")

    r_t1 = sch.add_symbol("Device:R", "R_T1", "60", 482.6, 290.83)
    r_t2 = sch.add_symbol("Device:R", "R_T2", "60", 482.6, 298.45)
    sch.wire((482.6, canh_y), r_t1.pin("1"))
    sch.wire(r_t1.pin("2"), r_t2.pin("1"))
    sch.wire(r_t2.pin("2"), (482.6, canl_y))
    mid = r_t1.pin("2")
    sch.wire_poly([u3.pin("5"), (469.9, u3.pin("5")[1]), (469.9, mid[1]), mid])
    c_sp = sch.add_symbol("Device:C", "C11", "4.7nF", 490.22, 317.5)
    sch.wire_poly([mid, (490.22, mid[1]), c_sp.pin("1")])
    sch.pin_gnd(c_sp, "2", 2.54)

    d_tvs1 = sch.add_symbol("Device:D_TVS", "D1", "PESD1CAN", 502.92, 274.32, rot=90)
    d_tvs2 = sch.add_symbol("Device:D_TVS", "D2", "PESD1CAN", 502.92, 317.5, rot=90)
    sch.wire(d_tvs1.pin("1"), (502.92, canh_y))
    sch.wire(d_tvs1.pin("2"), (502.92, 264.16))
    sch.add_power("power:GND", 502.92, 261.62, rot=180)
    sch.wire((502.92, 264.16), (502.92, 261.62))
    sch.wire(d_tvs2.pin("2"), (502.92, canl_y))
    sch.pin_gnd(d_tvs2, "1", 2.54)

    j6 = sch.add_symbol("Connector_Generic:Conn_01x03", "J6", "CAN_BUS", 551.18, 294.64)
    sch.stub_label(j6, "1", "CANH", 7.62)
    sch.stub_label(j6, "2", "CANL", 7.62)
    sch.pin_gnd(j6, "3", 5.08)

    sch.add_note(
        "Design Notes (Task 49 - BMS_5S_FULL_SYSTEM):\n"
        "- BQ76920: 5S RC input filters (1k / 1uF diff), CAP1=1uF, 10k NTC on TS1, 2mR low-side shunt Kelvin sense (100R / 100nF).\n"
        "- BQ76200: 470nF charge pump cap VDDCP-to-BAT, common-drain back-to-back high-side NFETs (Q1 CHG, Q2 DSG) with 100R gate + 10M G-S.\n"
        "- SN65HVD230: Powered from BQ76920 3.3V REGOUT (+3V3); 10k slope control, split 120R termination, TVS ESD protection.",
        33.02,
        342.9,
        size=1.524,
    )

    return sch.write(out_dir, "t49_bms_5s.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t49(out)
    print("Wrote:", p)
