#!/usr/bin/env python3
"""Task 62: MCU_SYNC_BUCK_SYSTEM — 200V->100V Digital Synchronous Buck (STM32F103C8T6 + 2x UCC5390E + 2x IMZA65R015M2H + INA240A1 + SN65HVD230)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t62(out_dir: str) -> str:
    sch = Schematic(
        title="Task 62: MCU Digital Synchronous Buck System (MCU_SYNC_BUCK_SYSTEM)",
        project_name="t62_mcu_sync_buck",
        paper="A2",
        comment1="STM32F103C8T6 + 2x UCC5390E Isolated Gate Drivers + 2x IMZA65R015M2H SiC MOSFETs + INA240A1 + SN65HVD230",
        comment2="Power Stage: 200V VIN -> 100V VOUT | Inputs: VIN, PWM, GND, CAN_TXD, CAN_RXD | Outputs: VOUT, CANH, CANL",
    )

    # A2: 594 x 420 mm (usable ~25..568, ~25..368 above title block)
    sch.add_section_box("MCU DIGITAL CONTROLLER (STM32F103C8T6 + CLOCK + RESET + ADC)", 25.4, 25.4, 213.36, 238.76)
    sch.add_section_box("CAN TELEMETRY INTERFACE (SN65HVD230 + TVS + SPLIT TERM)", 25.4, 241.3, 213.36, 368.3)
    sch.add_section_box("ISOLATED GATE DRIVERS (2x UCC5390E + BOOTSTRAP + R_ON/R_OFF)", 215.9, 25.4, 386.08, 238.76)
    sch.add_section_box("200V->100V SiC SYNC BUCK STAGE (2x IMZA65R015M2H + LC FILTER)", 388.62, 25.4, 568.96, 238.76)
    sch.add_section_box("CURRENT SENSE (INA240A1) & HV VOLTAGE SENSE DIVIDERS", 215.9, 241.3, 568.96, 368.3)

    # =========================================================================
    # 0. AUX POWER & EXTERNAL CONTROL CONNECTORS + PWR_FLAGS
    # =========================================================================
    # J1: Aux Power & External Control Inputs (+3V3, +15V_DRV, PWM, CAN_TXD, CAN_RXD, GND)
    j1 = sch.add_symbol("Connector_Generic:Conn_01x06", "J1", "CTRL_AUX_IF", 43.18, 60.96, mirror="y")
    sch.stub_label(j1, "1", "+3V3", 10.16)
    sch.stub_label(j1, "2", "+15V_DRV", 10.16)
    sch.stub_label(j1, "3", "PWM", 10.16)
    sch.stub_label(j1, "4", "CAN_TXD", 10.16)
    sch.stub_label(j1, "5", "CAN_RXD", 10.16)
    sch.pin_gnd(j1, "6", 5.08)

    sch.add_pwr_flag_on_net("GND", 86.36, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("+3V3", 86.36, 40.64, is_gnd=False)
    sch.add_pwr_flag_on_net("+15V_DRV", 121.92, 40.64, is_gnd=False)
    sch.add_pwr_flag_on_net("VIN", 157.48, 40.64, is_gnd=False)

    # =========================================================================
    # 1. STM32F103C8T6 DIGITAL POWER CONTROLLER (U1)
    # =========================================================================
    u1 = sch.add_symbol("test:STM32F103C8T6", "U1", "STM32F103C8T6", 137.16, 134.62)

    unused_mcu = (
        "2", "3", "4", "13", "14", "15", "17", "18", "19", "21", "22",
        "25", "27", "28", "30", "31", "34", "37", "38", "39", "40", "41", "42", "43", "45", "46",
    )
    for pn in unused_mcu:
        sch.no_connect(u1.pin(pn))

    # Power pins: VBAT(1), VDD_1(24), VDD_2(36), VDD_3(48) -> +3V3; VDDA(9) -> +3V3_A
    sch.stub_label(u1, "1", "+3V3", 10.16)
    sch.stub_label(u1, "24", "+3V3", 10.16)
    sch.stub_label(u1, "36", "+3V3", 10.16)
    sch.stub_label(u1, "48", "+3V3", 10.16)
    sch.stub_label(u1, "9", "+3V3_A", 10.16)

    # Grounds: VSSA(8), VSS_1(23), VSS_2(35), VSS_3(47)
    for gp, dx in (("8", -12.7), ("23", -12.7), ("35", 12.7), ("47", 15.24)):
        pt = u1.pin(gp)
        sch.wire(pt, (pt[0] + dx, pt[1]))
        sch.wire((pt[0] + dx, pt[1]), (pt[0] + dx, pt[1] + 2.54))
        sch.add_power("power:GND", pt[0] + dx, pt[1] + 2.54, rot=0)

    # MCU Decoupling + Ferrite Bead for VDDA (+3V3_A)
    dec_y = 203.2
    sch.wire((38.1, dec_y), (132.08, dec_y))
    sch.label("+3V3", (38.1, dec_y), rot=0, justify="right bottom")
    for idx, (cref, cval) in enumerate([("C1", "4.7uF"), ("C2", "100nF"), ("C3", "100nF"), ("C4", "100nF"), ("C5", "100nF")]):
        cx = 48.26 + idx * 17.78
        cs = sch.add_symbol("Device:C", cref, cval, cx, dec_y + 10.16)
        sch.wire((cx, dec_y), cs.pin("1"))
        sch.pin_gnd(cs, "2", 2.54)

    fb1 = sch.add_symbol("Device:FerriteBead", "FB1", "600R", 135.89, dec_y, rot=90)
    sch.wire(fb1.pin("2"), (187.96, dec_y))
    sch.label("+3V3_A", (187.96, dec_y), rot=0, justify="left bottom")
    c6 = sch.add_symbol("Device:C", "C6", "1uF", 157.48, dec_y + 10.16)
    c7 = sch.add_symbol("Device:C", "C7", "100nF", 172.72, dec_y + 10.16)
    sch.wire((157.48, dec_y), c6.pin("1"))
    sch.wire((172.72, dec_y), c7.pin("1"))
    sch.pin_gnd(c6, "2", 2.54)
    sch.pin_gnd(c7, "2", 2.54)

    # 8 MHz HSE Crystal (Y1) on PD0 (5) / PD1 (6)
    sch.stub_label(u1, "5", "OSC_IN", 10.16)
    sch.stub_label(u1, "6", "OSC_OUT", 10.16)
    y1 = sch.add_symbol("Device:Crystal", "Y1", "8MHz", 66.04, 106.68)
    c8 = sch.add_symbol("Device:C", "C8", "20pF", 53.34, 119.38)
    c9 = sch.add_symbol("Device:C", "C9", "20pF", 78.74, 119.38)
    sch.wire((45.72, 106.68), y1.pin("1"))
    sch.wire((53.34, 106.68), c8.pin("1"))
    sch.label("OSC_IN", (45.72, 106.68), rot=0, justify="right bottom")
    sch.wire(y1.pin("2"), (86.36, 106.68))
    sch.wire((78.74, 106.68), c9.pin("1"))
    sch.label("OSC_OUT", (86.36, 106.68), rot=0, justify="left bottom")
    sch.pin_gnd(c8, "2", 2.54)
    sch.pin_gnd(c9, "2", 2.54)

    # NRST (7) RC Reset
    sch.stub_label(u1, "7", "NRST", 10.16)
    r_rst = sch.add_symbol("Device:R", "R1", "10k", 53.34, 149.86)
    c_rst = sch.add_symbol("Device:C", "C10", "100nF", 53.34, 167.64)
    sch.wire(r_rst.pin("1"), (53.34, 139.7))
    sch.label("+3V3", (53.34, 139.7), rot=0, justify="left bottom")
    sch.wire(r_rst.pin("2"), c_rst.pin("1"))
    sch.wire((53.34, 157.48), (68.58, 157.48))
    sch.label("NRST", (68.58, 157.48), rot=0, justify="left bottom")
    sch.pin_gnd(c_rst, "2", 2.54)

    # BOOT0 (44) & PB2/BOOT1 (20) 10k pull-downs
    sch.stub_label(u1, "20", "BOOT1", 10.16)
    r_b1 = sch.add_symbol("Device:R", "R2", "10k", 83.82, 167.64)
    sch.wire(r_b1.pin("1"), (83.82, 160.02))
    sch.label("BOOT1", (83.82, 160.02), rot=0, justify="left bottom")
    sch.pin_gnd(r_b1, "2", 2.54)

    sch.stub_label(u1, "44", "BOOT0", 10.16)
    r_b0 = sch.add_symbol("Device:R", "R3", "10k", 190.5, 114.3)
    sch.wire(r_b0.pin("1"), (190.5, 106.68))
    sch.label("BOOT0", (190.5, 106.68), rot=0, justify="left bottom")
    sch.pin_gnd(r_b0, "2", 2.54)

    # Control / ADC / PWM / CAN Signals on STM32:
    # PA0 (10) <- VOUT_SENSE, PA1 (11) <- IOUT_SENSE, PA2 (12) <- VIN_SENSE, PA6 (16) <- PWM (cmd input)
    sch.stub_label(u1, "10", "VOUT_SENSE", 12.7)
    sch.stub_label(u1, "11", "IOUT_SENSE", 12.7)
    sch.stub_label(u1, "12", "VIN_SENSE", 12.7)
    sch.stub_label(u1, "16", "PWM", 12.7)
    # Right side: PA8 (29, TIM1_CH1) -> PWM_HS, PB13 (26, TIM1_CH1N) -> PWM_LS
    sch.stub_label(u1, "29", "PWM_HS", 15.24)
    sch.stub_label(u1, "26", "PWM_LS", 15.24)
    # PA11 (32, CAN_RX) <- CAN_RXD, PA12 (33, CAN_TX) -> CAN_TXD
    sch.stub_label(u1, "32", "CAN_RXD", 15.24)
    sch.stub_label(u1, "33", "CAN_TXD", 15.24)

    # =========================================================================
    # 2. SN65HVD230 CAN TRANSCEIVER (U5)
    # =========================================================================
    u5 = sch.add_symbol("test:SN65HVD230", "U5", "SN65HVD230", 86.36, 304.8)
    sch.wire(u5.pin("1"), (50.8, u5.pin("1")[1]))
    sch.label("CAN_TXD", (50.8, u5.pin("1")[1]), rot=0, justify="right bottom")
    sch.wire(u5.pin("4"), (50.8, u5.pin("4")[1]))
    sch.label("CAN_RXD", (50.8, u5.pin("4")[1]), rot=0, justify="right bottom")

    sch.wire(u5.pin("2"), (66.04, u5.pin("2")[1]))
    sch.wire((66.04, u5.pin("2")[1]), (66.04, u5.pin("2")[1] + 2.54))
    sch.add_power("power:GND", 66.04, u5.pin("2")[1] + 2.54, rot=0)

    p_u5_vcc = u5.pin("3")
    sch.wire_poly([p_u5_vcc, (71.12, p_u5_vcc[1]), (71.12, 330.2), (91.44, 330.2)])
    sch.label("+3V3", (71.12, 330.2), rot=0, justify="right bottom")
    c11 = sch.add_symbol("Device:C", "C11", "100nF", 81.28, 337.82)
    c12 = sch.add_symbol("Device:C", "C12", "10uF", 91.44, 337.82)
    sch.wire((81.28, 330.2), c11.pin("1"))
    sch.wire((91.44, 330.2), c12.pin("1"))
    sch.pin_gnd(c11, "2", 2.54)
    sch.pin_gnd(c12, "2", 2.54)

    # RS (8) -> 10k slope control to GND
    r_rs = sch.add_symbol("Device:R", "R4", "10k", 109.22, 281.94)
    sch.wire_ortho(u5.pin("8"), r_rs.pin("2"), horiz_first=True)
    sch.wire(r_rs.pin("1"), (109.22, 271.78))
    sch.add_power("power:GND", 109.22, 269.24, rot=180)
    sch.wire((109.22, 271.78), (109.22, 269.24))

    # CANH (7), CANL (6), VREF (5) split termination + TVS ESD + J2 CAN Bus connector
    canh_y = 297.18
    canl_y = 312.42
    sch.wire_poly([u5.pin("7"), (106.68, u5.pin("7")[1]), (106.68, canh_y), (160.02, canh_y)])
    sch.wire_poly([u5.pin("6"), (106.68, u5.pin("6")[1]), (106.68, canl_y), (160.02, canl_y)])
    sch.label("CANH", (160.02, canh_y), rot=0, justify="right bottom")
    sch.label("CANL", (160.02, canl_y), rot=0, justify="right bottom")

    r_t1 = sch.add_symbol("Device:R", "R5", "60", 124.46, 300.99)
    r_t2 = sch.add_symbol("Device:R", "R6", "60", 124.46, 308.61)
    sch.wire((124.46, canh_y), r_t1.pin("1"))
    sch.wire(r_t1.pin("2"), r_t2.pin("1"))
    sch.wire(r_t2.pin("2"), (124.46, canl_y))
    mid = r_t1.pin("2")
    sch.wire_poly([u5.pin("5"), (114.3, u5.pin("5")[1]), (114.3, mid[1]), mid])
    c_sp = sch.add_symbol("Device:C", "C13", "4.7nF", 132.08, 327.66)
    sch.wire_poly([mid, (132.08, mid[1]), c_sp.pin("1")])
    sch.pin_gnd(c_sp, "2", 2.54)

    d1 = sch.add_symbol("Device:D_TVS", "D1", "PESD1CAN", 147.32, 281.94, rot=90)
    d2 = sch.add_symbol("Device:D_TVS", "D2", "PESD1CAN", 147.32, 327.66, rot=90)
    sch.wire(d1.pin("1"), (147.32, canh_y))
    sch.wire(d1.pin("2"), (147.32, 271.78))
    sch.add_power("power:GND", 147.32, 269.24, rot=180)
    sch.wire((147.32, 271.78), (147.32, 269.24))
    sch.wire(d2.pin("2"), (147.32, canl_y))
    sch.pin_gnd(d2, "1", 2.54)

    j2 = sch.add_symbol("Connector_Generic:Conn_01x03", "J2", "CAN_BUS", 193.04, 304.8)
    sch.stub_label(j2, "1", "CANH", 7.62)
    sch.stub_label(j2, "2", "CANL", 7.62)
    sch.pin_gnd(j2, "3", 5.08)

    # =========================================================================
    # 3. TWO ISOLATED GATE DRIVERS: UCC5390E (U2 High-Side, U3 Low-Side)
    # =========================================================================
    # UCC5390E pins:
    #   Left:  1=VCC1(0,0), 2=IN+(0,-2.54), 3=IN-(0,-5.08), 4=GND1(0,-7.62)
    #   Right: 8=VEE2(27.94,0), 7=GND2(27.94,-2.54), 6=OUT(27.94,-5.08), 5=VCC2(27.94,-7.62)

    # --- U2: HIGH-SIDE GATE DRIVER (Floating domain referenced to HS_KS) ---
    u2 = sch.add_symbol("test:UCC5390E", "U2", "UCC5390E_HS", 264.16, 86.36)

    # Primary side of U2: VCC1(1) -> +3V3 + C14 (100nF), IN+(2) <- PWM_HS (with 10k pull-down R7), IN-(3) & GND1(4) -> GND
    p_u2_v1 = u2.pin("1")
    sch.wire_poly([p_u2_v1, (248.92, p_u2_v1[1]), (248.92, 68.58), (228.6, 68.58)])
    sch.label("+3V3", (228.6, 68.58), rot=0, justify="right bottom")
    c14 = sch.add_symbol("Device:C", "C14", "100nF", 241.3, 76.2)
    sch.wire((241.3, 68.58), c14.pin("1"))
    sch.pin_gnd(c14, "2", 2.54)

    p_u2_inp = u2.pin("2")
    sch.wire(p_u2_inp, (228.6, p_u2_inp[1]))
    sch.label("PWM_HS", (228.6, p_u2_inp[1]), rot=0, justify="right bottom")
    r_pd_hs = sch.add_symbol("Device:R", "R7", "10k", 233.68, 104.14)
    sch.wire((233.68, p_u2_inp[1]), r_pd_hs.pin("1"))
    sch.pin_gnd(r_pd_hs, "2", 2.54)

    p_u2_inm = u2.pin("3")
    p_u2_g1 = u2.pin("4")
    sch.wire(p_u2_inm, (254.0, p_u2_inm[1]))
    sch.wire(p_u2_g1, (254.0, p_u2_g1[1]))
    sch.wire((254.0, p_u2_inm[1]), (254.0, 109.22))
    sch.add_power("power:GND", 254.0, 109.22, rot=0)

    # Secondary side of U2 (High-Side):
    # VEE2(8) & GND2(7) -> HS_KS (Kelvin Source of Q1!)
    p_u2_vee2 = u2.pin("8")  # (292.1, 86.36)
    p_u2_gnd2 = u2.pin("7")  # (292.1, 88.90)
    p_u2_out = u2.pin("6")   # (292.1, 91.44)
    p_u2_vcc2 = u2.pin("5")  # (292.1, 93.98)

    hs_ks_y = 66.04
    sch.wire_poly([p_u2_vee2, (302.26, p_u2_vee2[1]), (302.26, hs_ks_y), (375.92, hs_ks_y)])
    sch.wire_poly([p_u2_gnd2, (302.26, p_u2_gnd2[1]), (302.26, p_u2_vee2[1])])
    sch.label("HS_KS", (375.92, hs_ks_y), rot=0, justify="left bottom")

    # Bootstrap supply for U2.VCC2(5): +15V_DRV -> R_BOOT (4.7R) -> D_BOOT (US1J 600V) -> VCC2_HS + C15 (1uF) + C16 (100nF) to HS_KS
    vcc2_hs_y = 114.3
    sch.wire_poly([p_u2_vcc2, (302.26, p_u2_vcc2[1]), (302.26, vcc2_hs_y), (332.74, vcc2_hs_y)])
    r_boot = sch.add_symbol("Device:R", "R8", "4.7", 264.16, 121.92, rot=90)
    d_boot = sch.add_symbol("Device:D", "D3", "US1J_600V", 284.48, 121.92, rot=180)
    # Device:D unrotated: 1=K(-3.81,0), 2=A(+3.81,0). rot=180 -> 2=A(-3.81,0) LEFT, 1=K(+3.81,0) RIGHT!
    sch.wire((246.38, 121.92), r_boot.pin("1"))
    sch.label("+15V_DRV", (246.38, 121.92), rot=0, justify="right bottom")
    sch.wire(r_boot.pin("2"), d_boot.pin("2"))  # Anode
    sch.wire_poly([d_boot.pin("1"), (302.26, 121.92), (302.26, vcc2_hs_y)])

    # Bootstrap caps C15 (1uF) & C16 (100nF) between VCC2_HS and HS_KS
    c15 = sch.add_symbol("Device:C", "C15", "1uF", 317.5, 106.68)
    c16 = sch.add_symbol("Device:C", "C16", "100nF", 332.74, 106.68)
    sch.wire((317.5, vcc2_hs_y), c15.pin("2"))
    sch.wire((332.74, vcc2_hs_y), c16.pin("2"))
    sch.wire_poly([c15.pin("1"), (317.5, 99.06), (347.98, 99.06)])
    sch.wire(c16.pin("1"), (332.74, 99.06))
    sch.label("HS_KS", (347.98, 99.06), rot=0, justify="left bottom")

    # Separate Turn-On / Turn-Off gate drive network for Q1 (HS):
    # Turn-on:  U2.OUT -> R9 (10R) -> HS_GATE
    # Turn-off: HS_GATE -> R10 (3.3R) -> D4 (Schottky, A at R10, K at U2.OUT)
    r_gon_hs = sch.add_symbol("Device:R", "R9", "10", 337.82, 76.2, rot=90)
    d_goff_hs = sch.add_symbol("Device:D_Schottky", "D4", "B340A", 330.2, 86.36)  # 1=K left, 2=A right!
    r_goff_hs = sch.add_symbol("Device:R", "R10", "3.3", 347.98, 86.36, rot=90)
    sch.wire_poly([p_u2_out, (312.42, p_u2_out[1]), (312.42, 76.2), r_gon_hs.pin("1")])
    sch.wire((312.42, 86.36), d_goff_hs.pin("1"))  # K to OUT
    sch.wire(d_goff_hs.pin("2"), r_goff_hs.pin("1"))  # A to R_GOFF
    sch.wire_poly([r_gon_hs.pin("2"), (360.68, 76.2), (360.68, 86.36), r_goff_hs.pin("2")])
    sch.wire((360.68, 76.2), (375.92, 76.2))
    sch.label("HS_GATE", (375.92, 76.2), rot=0, justify="left bottom")

    # --- U3: LOW-SIDE GATE DRIVER (Referenced to LS_KS) ---
    u3 = sch.add_symbol("test:UCC5390E", "U3", "UCC5390E_LS", 264.16, 177.8)

    # Primary side of U3: VCC1(1) -> +3V3 + C17 (100nF), IN+(2) <- PWM_LS (with 10k pull-down R11), IN-(3) & GND1(4) -> GND
    p_u3_v1 = u3.pin("1")
    sch.wire_poly([p_u3_v1, (248.92, p_u3_v1[1]), (248.92, 160.02), (228.6, 160.02)])
    sch.label("+3V3", (228.6, 160.02), rot=0, justify="right bottom")
    c17 = sch.add_symbol("Device:C", "C17", "100nF", 241.3, 167.64)
    sch.wire((241.3, 160.02), c17.pin("1"))
    sch.pin_gnd(c17, "2", 2.54)

    p_u3_inp = u3.pin("2")
    sch.wire(p_u3_inp, (228.6, p_u3_inp[1]))
    sch.label("PWM_LS", (228.6, p_u3_inp[1]), rot=0, justify="right bottom")
    r_pd_ls = sch.add_symbol("Device:R", "R11", "10k", 233.68, 195.58)
    sch.wire((233.68, p_u3_inp[1]), r_pd_ls.pin("1"))
    sch.pin_gnd(r_pd_ls, "2", 2.54)

    p_u3_inm = u3.pin("3")
    p_u3_g1 = u3.pin("4")
    sch.wire(p_u3_inm, (254.0, p_u3_inm[1]))
    sch.wire(p_u3_g1, (254.0, p_u3_g1[1]))
    sch.wire((254.0, p_u3_inm[1]), (254.0, 200.66))
    sch.add_power("power:GND", 254.0, 200.66, rot=0)

    # Secondary side of U3 (Low-Side):
    p_u3_vee2 = u3.pin("8")
    p_u3_gnd2 = u3.pin("7")
    p_u3_out = u3.pin("6")
    p_u3_vcc2 = u3.pin("5")

    ls_ks_y = 157.48
    sch.wire_poly([p_u3_vee2, (302.26, p_u3_vee2[1]), (302.26, ls_ks_y), (375.92, ls_ks_y)])
    sch.wire_poly([p_u3_gnd2, (302.26, p_u3_gnd2[1]), (302.26, p_u3_vee2[1])])
    sch.label("LS_KS", (375.92, ls_ks_y), rot=0, justify="left bottom")

    # U3.VCC2(5) -> +15V_DRV with local C18 (1uF) + C19 (100nF) to LS_KS
    vcc2_ls_y = 208.28
    sch.wire_poly([p_u3_vcc2, (302.26, p_u3_vcc2[1]), (302.26, vcc2_ls_y), (347.98, vcc2_ls_y)])
    sch.label("+15V_DRV", (347.98, vcc2_ls_y), rot=0, justify="left bottom")
    c18 = sch.add_symbol("Device:C", "C18", "1uF", 317.5, 200.66)
    c19 = sch.add_symbol("Device:C", "C19", "100nF", 332.74, 200.66)
    sch.wire((317.5, vcc2_ls_y), c18.pin("2"))
    sch.wire((332.74, vcc2_ls_y), c19.pin("2"))
    sch.wire_poly([c18.pin("1"), (317.5, 193.04), (347.98, 193.04)])
    sch.wire(c19.pin("1"), (332.74, 193.04))
    sch.label("LS_KS", (347.98, 193.04), rot=0, justify="left bottom")

    # Separate Turn-On / Turn-Off gate drive network for Q2 (LS):
    r_gon_ls = sch.add_symbol("Device:R", "R12", "10", 337.82, 167.64, rot=90)
    d_goff_ls = sch.add_symbol("Device:D_Schottky", "D5", "B340A", 330.2, 177.8)
    r_goff_ls = sch.add_symbol("Device:R", "R13", "3.3", 347.98, 177.8, rot=90)
    sch.wire_poly([p_u3_out, (312.42, p_u3_out[1]), (312.42, 167.64), r_gon_ls.pin("1")])
    sch.wire((312.42, 177.8), d_goff_ls.pin("1"))
    sch.wire(d_goff_ls.pin("2"), r_goff_ls.pin("1"))
    sch.wire_poly([r_gon_ls.pin("2"), (360.68, 167.64), (360.68, 177.8), r_goff_ls.pin("2")])
    sch.wire((360.68, 167.64), (375.92, 167.64))
    sch.label("LS_GATE", (375.92, 167.64), rot=0, justify="left bottom")

    # =========================================================================
    # 4. 200V -> 100V SiC SYNCHRONOUS BUCK POWER STAGE (Q1, Q2, L1, R_SHUNT)
    # =========================================================================
    # Input HV Connector J3 (VIN 200V, GND) & DC-link bulk/film/ceramic caps
    j3 = sch.add_symbol("Connector_Generic:Conn_01x02", "J3", "HV_IN_200V", 406.4, 53.34, mirror="y")
    sch.stub_label(j3, "1", "VIN", 7.62)
    sch.pin_gnd(j3, "2", 5.08)

    vin_bus_y = 48.26
    sch.wire((429.26, vin_bus_y), (474.98, vin_bus_y))
    sch.label("VIN", (429.26, vin_bus_y), rot=0, justify="right bottom")
    c_dc1 = sch.add_symbol("Device:C_Polarized", "C20", "100uF/450V", 436.88, 63.5)
    c_dc2 = sch.add_symbol("Device:C", "C21", "10uF/450V", 452.12, 63.5)
    c_dc3 = sch.add_symbol("Device:C", "C22", "100nF/450V", 464.82, 63.5)
    for c_sym, cx in ((c_dc1, 436.88), (c_dc2, 452.12), (c_dc3, 464.82)):
        sch.wire((cx, vin_bus_y), c_sym.pin("1"))
        sch.pin_gnd(c_sym, "2", 2.54)

    # High-Side SiC MOSFET Q1 (IMZA65R015M2H): D=(+5.08,-7.62), S=(+5.08,+7.62), G=(-5.08,0), KS=(-5.08,+2.54)
    q1 = sch.add_symbol("test:IMZA65R015M2H", "Q1", "IMZA65R015M2H", 469.9, 96.52)
    sch.wire_poly([q1.pin("1"), (q1.pin("1")[0], vin_bus_y)])  # D -> VIN

    # Q1 Gate + Kelvin Source + R_GS_HS (10k)
    sch.wire(q1.pin("4"), (441.96, q1.pin("4")[1]))
    sch.label("HS_GATE", (441.96, q1.pin("4")[1]), rot=0, justify="right bottom")
    sch.wire(q1.pin("3"), (441.96, q1.pin("3")[1]))
    sch.label("HS_KS", (441.96, q1.pin("3")[1]), rot=0, justify="right bottom")
    r_gs_hs = sch.add_symbol("Device:R", "R14", "10k", 454.66, 111.76)
    sch.wire((454.66, q1.pin("4")[1]), r_gs_hs.pin("1"))
    sch.wire_poly([r_gs_hs.pin("2"), (454.66, 121.92), (449.58, 121.92), (449.58, q1.pin("3")[1])])

    # Low-Side SiC MOSFET Q2 (IMZA65R015M2H)
    q2 = sch.add_symbol("test:IMZA65R015M2H", "Q2", "IMZA65R015M2H", 469.9, 175.26)

    # Half-bridge switch node SW_NODE between Q1.S (2) and Q2.D (1)
    sch.wire(q1.pin("2"), q2.pin("1"))
    sw_y = 134.62

    # Q2 Gate + Kelvin Source + R_GS_LS (10k)
    sch.wire(q2.pin("4"), (441.96, q2.pin("4")[1]))
    sch.label("LS_GATE", (441.96, q2.pin("4")[1]), rot=0, justify="right bottom")
    sch.wire(q2.pin("3"), (441.96, q2.pin("3")[1]))
    sch.label("LS_KS", (441.96, q2.pin("3")[1]), rot=0, justify="right bottom")
    r_gs_ls = sch.add_symbol("Device:R", "R15", "10k", 454.66, 190.5)
    sch.wire((454.66, q2.pin("4")[1]), r_gs_ls.pin("1"))
    sch.wire_poly([r_gs_ls.pin("2"), (454.66, 200.66), (449.58, 200.66), (449.58, q2.pin("3")[1])])

    # Low-side current sense shunt R_SHUNT (5mR) between Q2.S (SHUNT_P) and GND (CM = 0V, safely within INA240A1 -4V..80V!)
    r_shunt = sch.add_symbol("Device:R", "R_SHUNT", "5mR", q2.pin("2")[0], 208.28)
    sch.wire(q2.pin("2"), r_shunt.pin("1"))
    sch.wire((q2.pin("2")[0], 193.04), (497.84, 193.04))
    sch.label("SHUNT_P", (497.84, 193.04), rot=0, justify="left bottom")
    sch.pin_gnd(r_shunt, "2", 5.08)

    # Buck Output Filter: SW_NODE -> L_BUCK (100uH) -> VOUT (100V) + output caps C23..C25 + J4 Output Connector
    l_buck = sch.add_symbol("Device:L", "L1", "100uH", 500.38, sw_y, rot=90)
    sch.wire((q1.pin("2")[0], sw_y), l_buck.pin("1"))
    sch.label("SW_NODE", (485.14, sw_y), rot=0, justify="left bottom")
    sch.wire(l_buck.pin("2"), (553.72, sw_y))
    sch.label("VOUT", (553.72, sw_y), rot=0, justify="left bottom")

    c_out1 = sch.add_symbol("Device:C_Polarized", "C23", "100uF/250V", 518.16, 149.86)
    c_out2 = sch.add_symbol("Device:C", "C24", "10uF/250V", 533.4, 149.86)
    c_out3 = sch.add_symbol("Device:C", "C25", "100nF/250V", 546.1, 149.86)
    for c_sym, cx in ((c_out1, 518.16), (c_out2, 533.4), (c_out3, 546.1)):
        sch.wire((cx, sw_y), c_sym.pin("1"))
        sch.pin_gnd(c_sym, "2", 2.54)

    j4 = sch.add_symbol("Connector_Generic:Conn_01x02", "J4", "HV_OUT_100V", 553.72, 198.12)
    sch.stub_label(j4, "1", "VOUT", 7.62)
    sch.pin_gnd(j4, "2", 5.08)

    # =========================================================================
    # 5. INA240A1 CURRENT SENSE AMP (U4) & HV VOLTAGE SENSE DIVIDERS (VOUT, VIN)
    # =========================================================================
    # INA240A1 pins:
    #   Left:   8=+ (-7.62, +2.54), 1=- (-7.62, -2.54)
    #   Top:    6=V+ (-2.54, +7.62)
    #   Bottom: 2/4=GND (-2.54, -7.62), 3=REF2 (0, -7.62), 7=REF1 (+2.54, -7.62)
    #   Right:  5=~ OUT (+7.62, 0)
    u4 = sch.add_symbol("test:INA240A1", "U4", "INA240A1", 299.72, 304.8)

    # Kelvin RC filter on INA240A1 inputs: SHUNT_P -> R16 (10R) -> IN+(8); GND -> R17 (10R) -> IN-(1); C26 (1nF) diff cap
    p_in_p = u4.pin("8")  # y = 302.26
    p_in_m = u4.pin("1")  # y = 307.34
    r_cs1 = sch.add_symbol("Device:R", "R16", "10", 264.16, 294.64, rot=90)
    r_cs2 = sch.add_symbol("Device:R", "R17", "10", 264.16, 314.96, rot=90)
    sch.wire_poly([p_in_p, (281.94, p_in_p[1]), (281.94, 294.64), r_cs1.pin("2")])
    sch.wire_poly([p_in_m, (281.94, p_in_m[1]), (281.94, 314.96), r_cs2.pin("2")])
    sch.wire(r_cs1.pin("1"), (238.76, 294.64))
    sch.label("SHUNT_P", (238.76, 294.64), rot=0, justify="right bottom")
    sch.wire(r_cs2.pin("1"), (248.92, 314.96))
    sch.wire((248.92, 314.96), (248.92, 317.5))
    sch.add_power("power:GND", 248.92, 317.5, rot=0)

    c_cs = sch.add_symbol("Device:C", "C26", "1nF", 276.86, 304.8)
    sch.wire((276.86, 294.64), c_cs.pin("1"))
    sch.wire(c_cs.pin("2"), (276.86, 314.96))

    # V+ (6) -> +3V3 with C27 (100nF) decoupling
    p_u4_vp = u4.pin("6")
    sch.wire_poly([p_u4_vp, (p_u4_vp[0], 274.32), (322.58, 274.32)])
    sch.label("+3V3", (322.58, 274.32), rot=0, justify="left bottom")
    c27 = sch.add_symbol("Device:C", "C27", "100nF", 312.42, 281.94)
    sch.wire((312.42, 274.32), c27.pin("1"))
    sch.pin_gnd(c27, "2", 2.54)

    # Stacked GND (2, 4) -> GND; REF2 (3) -> GND; REF1 (7) -> +3V3 (bidirectional Vs/2 = 1.65V reference!)
    p_u4_gnd = u4.pin("2")
    p_u4_ref2 = u4.pin("3")
    p_u4_ref1 = u4.pin("7")
    sch.wire_poly([p_u4_gnd, (p_u4_gnd[0], 325.12), (p_u4_ref2[0], 325.12), p_u4_ref2])
    sch.add_power("power:GND", p_u4_gnd[0], 327.66, rot=0)
    sch.wire((p_u4_gnd[0], 325.12), (p_u4_gnd[0], 327.66))
    sch.wire_poly([p_u4_ref1, (p_u4_ref1[0], 335.28), (320.04, 335.28)])
    sch.label("+3V3", (320.04, 335.28), rot=0, justify="left bottom")

    # Output OUT (5) -> R18 (100R) + C28 (10nF) -> IOUT_SENSE (to STM32 PA1)
    p_u4_out = u4.pin("5")
    r_iout = sch.add_symbol("Device:R", "R18", "100", 327.66, p_u4_out[1], rot=90)
    sch.wire(p_u4_out, r_iout.pin("1"))
    sch.wire(r_iout.pin("2"), (358.14, p_u4_out[1]))
    sch.label("IOUT_SENSE", (358.14, p_u4_out[1]), rot=0, justify="left bottom")
    c28 = sch.add_symbol("Device:C", "C28", "10nF", 342.9, p_u4_out[1] + 10.16)
    sch.wire((342.9, p_u4_out[1]), c28.pin("1"))
    sch.pin_gnd(c28, "2", 2.54)

    # --- VOUT (100V) Voltage Sense Divider -> VOUT_SENSE (2.94V @ 100V -> STM32 PA0) ---
    r_vs1 = sch.add_symbol("Device:R", "R19", "330k", 419.1, 281.94)
    r_vs2 = sch.add_symbol("Device:R", "R20", "10k", 419.1, 307.34)
    c_vs = sch.add_symbol("Device:C", "C29", "100nF", 436.88, 307.34)
    sch.wire(r_vs1.pin("1"), (419.1, 271.78))
    sch.label("VOUT", (419.1, 271.78), rot=0, justify="left bottom")
    sch.wire(r_vs1.pin("2"), r_vs2.pin("1"))
    sch.pin_gnd(r_vs2, "2", 2.54)
    sch.wire((419.1, 294.64), (457.2, 294.64))
    sch.wire((436.88, 294.64), c_vs.pin("1"))
    sch.pin_gnd(c_vs, "2", 2.54)
    sch.label("VOUT_SENSE", (457.2, 294.64), rot=0, justify="left bottom")

    # --- VIN (200V) Voltage Sense Divider -> VIN_SENSE (2.90V @ 200V -> STM32 PA2) ---
    r_vin1 = sch.add_symbol("Device:R", "R21", "680k", 502.92, 281.94)
    r_vin2 = sch.add_symbol("Device:R", "R22", "10k", 502.92, 307.34)
    c_vins = sch.add_symbol("Device:C", "C30", "100nF", 520.7, 307.34)
    sch.wire(r_vin1.pin("1"), (502.92, 271.78))
    sch.label("VIN", (502.92, 271.78), rot=0, justify="left bottom")
    sch.wire(r_vin1.pin("2"), r_vin2.pin("1"))
    sch.pin_gnd(r_vin2, "2", 2.54)
    sch.wire((502.92, 294.64), (541.02, 294.64))
    sch.wire((520.7, 294.64), c_vins.pin("1"))
    sch.pin_gnd(c_vins, "2", 2.54)
    sch.label("VIN_SENSE", (541.02, 294.64), rot=0, justify="left bottom")

    sch.add_note(
        "Design Notes (Task 62 - MCU_SYNC_BUCK_SYSTEM):\n"
        "- 200V->100V Synchronous Buck: 2x IMZA65R015M2H (650V 15mR SiC) with dedicated Kelvin Source (KS) return to UCC5390E GND2/VEE2.\n"
        "- Separate turn-on (10R) and fast turn-off (3.3R + B340A Schottky) gate paths + 10k G-KS bleed on both HS and LS MOSFETs.\n"
        "- High-side UCC5390E powered via 600V bootstrap diode (US1J) + 4.7R + 1uF/100nF caps referenced to HS_KS.\n"
        "- INA240A1 monitors 5mR low-side shunt (CM=0V, within -4V..80V spec) with Vs/2 midpoint bias (REF1=+3V3, REF2=GND).",
        223.52,
        345.44,
        size=1.27,
    )

    return sch.write(out_dir, "t62_mcu_sync_buck.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t62(out)
    print("Wrote:", p)
