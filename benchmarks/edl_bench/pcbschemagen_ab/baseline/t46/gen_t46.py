#!/usr/bin/env python3
"""Task 46: CAN_ISOLATED_FULL — Fully isolated CAN interface (ISO7741 + SN65HVD230 + NXE1S0303MC)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t46(out_dir: str) -> str:
    sch = Schematic(
        title="Task 46: Fully Isolated CAN Interface (CAN_ISOLATED_FULL)",
        project_name="t46_can_isolated",
        paper="A3",
        comment1="ISO7741 Digital Isolator + SN65HVD230 CAN Transceiver + NXE1S0303MC 3.3V-to-3.3V Isolated DC-DC",
        comment2="Primary: VDD (3.3V), GND, TXD, RXD | Isolated Secondary: VDD_ISO (3.3V), GND_ISO, CANH, CANL",
    )

    # Section visual boxes (A3 is 420 x 297 mm; keep above title block y<=246)
    sch.add_section_box("PRIMARY DOMAIN (HOST MCU - 3.3V / GND)", 25.4, 25.4, 177.8, 246.38)
    sch.add_section_box("ISOLATION BARRIER", 180.34, 25.4, 236.22, 246.38)
    sch.add_section_box("ISOLATED CAN DOMAIN (VDD_ISO / GND_ISO)", 238.76, 25.4, 393.7, 246.38)

    # =========================================================================
    # 0. HOST INTERFACE HEADER (J1: VDD, GND, TXD, RXD) & CAN BUS HEADER (J2: CANH, CANL, GND_ISO)
    # =========================================================================
    # Using standard connectors gives clean external I/O pins with 0 dangling labels!
    j1 = sch.add_symbol("Connector_Generic:Conn_01x04", "J1", "HOST_IF", 76.2, 165.1, mirror="y")
    # mirror="y" puts pins on the RIGHT in natural top-to-bottom order (1=VDD, 2=TXD, 3=RXD, 4=GND)
    sch.stub_label(j1, "1", "VDD", 10.16)
    sch.stub_label(j1, "2", "TXD", 10.16)
    sch.stub_label(j1, "3", "RXD", 10.16)
    sch.pin_gnd(j1, "4", 5.08)

    # CAN bus connector J2 (pins on left -> unrotated Conn_01x03)
    j2 = sch.add_symbol("Connector_Generic:Conn_01x03", "J2", "CAN_BUS", 381.0, 172.72)
    sch.stub_label(j2, "1", "CANH", 7.62)
    sch.stub_label(j2, "2", "CANL", 7.62)
    sch.stub_label(j2, "3", "GND_ISO", 7.62)

    # =========================================================================
    # 1. ISOLATED DC-DC POWER SUPPLY: NXE1S0303MC (U1)
    # =========================================================================
    u1 = sch.add_symbol("test:NXE1S0303MC", "U1", "NXE1S0303MC", 208.28, 76.2)
    sch.no_connect(u1.pin("14"))  # NC

    # Primary input filter: VDD -> L1 (4.7uH) -> U1.+VIN with C1 (10uF), C2 (100nF) to GND
    l1 = sch.add_symbol("Device:L", "L1", "4.7uH", 162.56, 71.12, rot=90)
    c1 = sch.add_symbol("Device:C", "C1", "10uF", 147.32, 81.28)
    c2 = sch.add_symbol("Device:C", "C2", "100nF", 175.26, 81.28)

    vdd_in = (132.08, 71.12)
    sch.label("VDD", vdd_in, rot=0, justify="right bottom")
    sch.wire(vdd_in, l1.pin("1"))
    sch.wire((147.32, 71.12), c1.pin("1"))
    sch.wire(l1.pin("2"), u1.pin("3"))  # +VIN
    sch.wire((175.26, 71.12), c2.pin("1"))

    # Primary GND bus under U1.-VIN
    gnd_y_u1 = 91.44
    sch.wire(c1.pin("2"), (147.32, gnd_y_u1))
    sch.wire(c2.pin("2"), (175.26, gnd_y_u1))
    sch.wire_ortho(u1.pin("1"), (187.96, gnd_y_u1), horiz_first=True)
    sch.wire((147.32, gnd_y_u1), (187.96, gnd_y_u1))
    sch.add_power("power:GND", 162.56, gnd_y_u1 + 2.54, rot=0)
    sch.wire((162.56, gnd_y_u1), (162.56, gnd_y_u1 + 2.54))
    sch.add_pwr_flag_on_net("GND", 116.84, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("VDD", 116.84, 40.64, is_gnd=False)

    # Secondary isolated output filter: U1.+VOUT -> VDD_ISO with C3 (10uF), C4 (100nF), R1 (3.3k min load)
    c3 = sch.add_symbol("Device:C", "C3", "10uF", 248.92, 81.28)
    c4 = sch.add_symbol("Device:C", "C4", "100nF", 261.62, 81.28)
    r_load = sch.add_symbol("Device:R", "R1", "3.3k", 274.32, 81.28)

    vdd_iso_y = 71.12
    gnd_iso_y = 91.44
    sch.wire(u1.pin("8"), (284.48, vdd_iso_y))
    sch.wire((248.92, vdd_iso_y), c3.pin("1"))
    sch.wire((261.62, vdd_iso_y), c4.pin("1"))
    sch.wire((274.32, vdd_iso_y), r_load.pin("1"))
    sch.label("VDD_ISO", (284.48, vdd_iso_y), rot=0, justify="left bottom")

    sch.wire_ortho(u1.pin("7"), (236.22, gnd_iso_y), horiz_first=True)
    sch.wire((236.22, gnd_iso_y), (284.48, gnd_iso_y))
    sch.wire(c3.pin("2"), (248.92, gnd_iso_y))
    sch.wire(c4.pin("2"), (261.62, gnd_iso_y))
    sch.wire(r_load.pin("2"), (274.32, gnd_iso_y))
    sch.label("GND_ISO", (284.48, gnd_iso_y), rot=0, justify="left bottom")

    # =========================================================================
    # 2. DIGITAL ISOLATOR: ISO7741 (U2)
    # =========================================================================
    u2 = sch.add_symbol("test:ISO7741", "U2", "ISO7741", 208.28, 172.72)

    # Side 1 Power (Vcc1 pin 1, GND1 pin 8) + decoupling C5 (100nF), C6 (1uF)
    p_vcc1 = u2.pin("1")
    sch.wire(p_vcc1, (p_vcc1[0], p_vcc1[1] - 10.16))
    sch.wire((p_vcc1[0], p_vcc1[1] - 10.16), (165.1, p_vcc1[1] - 10.16))
    sch.label("VDD", (165.1, p_vcc1[1] - 10.16), rot=0, justify="right bottom")

    c5 = sch.add_symbol("Device:C", "C5", "100nF", 182.88, p_vcc1[1] - 2.54)
    c6 = sch.add_symbol("Device:C", "C6", "1uF", 172.72, p_vcc1[1] - 2.54)
    sch.wire((182.88, p_vcc1[1] - 10.16), c5.pin("1"))
    sch.wire((172.72, p_vcc1[1] - 10.16), c6.pin("1"))
    sch.pin_gnd(c5, "2", 2.54)
    sch.pin_gnd(c6, "2", 2.54)
    sch.pin_gnd(u2, "8", 5.08)

    # Side 2 Power (Vcc2 pin 16, GND2 pin 9) + decoupling C7 (100nF), C8 (1uF)
    p_vcc2 = u2.pin("16")
    p_gnd2 = u2.pin("9")
    sch.wire(p_vcc2, (p_vcc2[0], p_vcc2[1] - 10.16))
    sch.wire((p_vcc2[0], p_vcc2[1] - 10.16), (256.54, p_vcc2[1] - 10.16))
    sch.label("VDD_ISO", (256.54, p_vcc2[1] - 10.16), rot=0, justify="left bottom")

    c7 = sch.add_symbol("Device:C", "C7", "100nF", 241.3, p_vcc2[1] - 2.54)
    c8 = sch.add_symbol("Device:C", "C8", "1uF", 251.46, p_vcc2[1] - 2.54)
    sch.wire((241.3, p_vcc2[1] - 10.16), c7.pin("1"))
    sch.wire((251.46, p_vcc2[1] - 10.16), c8.pin("1"))
    sch.wire(c7.pin("2"), (241.3, p_vcc2[1] + 5.08))
    sch.wire(c8.pin("2"), (251.46, p_vcc2[1] + 5.08))
    sch.wire((241.3, p_vcc2[1] + 5.08), (256.54, p_vcc2[1] + 5.08))
    sch.label("GND_ISO", (256.54, p_vcc2[1] + 5.08), rot=0, justify="left bottom")

    sch.wire(p_gnd2, (p_gnd2[0], p_gnd2[1] + 7.62))
    sch.wire((p_gnd2[0], p_gnd2[1] + 7.62), (241.3, p_gnd2[1] + 7.62))
    sch.label("GND_ISO", (241.3, p_gnd2[1] + 7.62), rot=0, justify="left bottom")

    # Side 1 Signal Channels:
    # INA (pin 2) <- TXD (with 33R series damping resistor R2)
    p_ina = u2.pin("2")
    r2 = sch.add_symbol("Device:R", "R2", "33", 172.72, p_ina[1], rot=90)
    sch.wire(r2.pin("2"), p_ina)
    sch.wire((132.08, p_ina[1]), r2.pin("1"))
    sch.label("TXD", (132.08, p_ina[1]), rot=0, justify="right bottom")

    # IND (pin 5) -> RXD (reverse channel D on ISO7741, with 33R series resistor R3)
    p_ind = u2.pin("5")
    r3 = sch.add_symbol("Device:R", "R3", "33", 172.72, p_ind[1], rot=90)
    sch.wire(r3.pin("2"), p_ind)
    sch.wire((132.08, p_ind[1]), r3.pin("1"))
    sch.label("RXD", (132.08, p_ind[1]), rot=0, justify="right bottom")

    # Unused Side 1 inputs INB (3), INC (4), INE (6), INF (7) tied to GND via 10k pull-down R4
    r4 = sch.add_symbol("Device:R", "R4", "10k", 185.42, 193.04)
    sch.pin_gnd(r4, "2", 2.54)
    sch.wire(u2.pin("3"), (193.04, u2.pin("3")[1]))
    sch.wire(u2.pin("4"), (193.04, u2.pin("4")[1]))
    sch.wire((193.04, u2.pin("3")[1]), (193.04, u2.pin("4")[1]))
    sch.label("ISO_TIE_GND", (193.04, u2.pin("3")[1]), rot=0, justify="right bottom")

    sch.wire(u2.pin("6"), (185.42, u2.pin("6")[1]))
    sch.wire(u2.pin("7"), (185.42, u2.pin("7")[1]))
    sch.wire((185.42, u2.pin("6")[1]), (185.42, r4.pin("1")[1]))
    sch.label("ISO_TIE_GND", (185.42, r4.pin("1")[1]), rot=0, justify="right bottom")

    # Unused Side 2 outputs OUTB (14), OUTC (13), OUTE (11), OUTF (10) -> no_connect
    for pin_num in ("14", "13", "11", "10"):
        sch.no_connect(u2.pin(pin_num))

    # =========================================================================
    # 3. CAN TRANSCEIVER: SN65HVD230 (U3) ON ISOLATED SIDE
    # =========================================================================
    u3 = sch.add_symbol("test:SN65HVD230", "U3", "SN65HVD230", 299.72, 172.72)

    # Connect ISO7741.OUTA (15) -> CAN_TXD_ISO -> U3.D (1) with clean jog away from pin
    p_outa = u2.pin("15")
    p_d = u3.pin("1")
    sch.wire_poly([p_outa, (281.94, p_outa[1]), (281.94, p_d[1]), p_d])
    sch.label("CAN_TXD_ISO", (248.92, p_outa[1]), rot=0, justify="left bottom")

    # Connect ISO7741.OUTD (12) <- CAN_RXD_ISO <- U3.R (4) — jog RIGHT by 15.24mm first so we don't touch OUTE!
    p_outd = u2.pin("12")
    p_r = u3.pin("4")
    sch.wire_poly([p_outd, (233.68, p_outd[1]), (233.68, p_r[1]), p_r])
    sch.label("CAN_RXD_ISO", (248.92, p_r[1]), rot=0, justify="left bottom")

    # U3.VCC (pin 3) -> VDD_ISO with C9 (100nF) & C10 (10uF) to GND_ISO
    p_u3_vcc = u3.pin("3")
    p_u3_gnd = u3.pin("2")
    sch.wire_poly([p_u3_vcc, (281.94, p_u3_vcc[1]), (281.94, 198.12)])
    sch.label("VDD_ISO", (281.94, 198.12), rot=0, justify="right bottom")

    c9 = sch.add_symbol("Device:C", "C9", "100nF", 292.1, 205.74)
    c10 = sch.add_symbol("Device:C", "C10", "10uF", 302.26, 205.74)
    sch.wire((281.94, 198.12), (302.26, 198.12))
    sch.wire((292.1, 198.12), c9.pin("1"))
    sch.wire((302.26, 198.12), c10.pin("1"))
    sch.wire(c9.pin("2"), (292.1, 215.9))
    sch.wire(c10.pin("2"), (302.26, 215.9))
    sch.wire((281.94, 215.9), (302.26, 215.9))
    sch.label("GND_ISO", (281.94, 215.9), rot=0, justify="right bottom")

    # U3.GND (pin 2) -> GND_ISO
    sch.wire(p_u3_gnd, (279.4, p_u3_gnd[1]))
    sch.label("GND_ISO", (279.4, p_u3_gnd[1]), rot=0, justify="right bottom")

    # U3.RS (pin 8) -> R5 (10k slope-control resistor) to GND_ISO
    p_rs = u3.pin("8")
    r5 = sch.add_symbol("Device:R", "R5", "10k", 322.58, 152.4)
    sch.wire_ortho(p_rs, r5.pin("2"), horiz_first=True)
    sch.wire(r5.pin("1"), (322.58, 142.24))
    sch.label("GND_ISO", (322.58, 142.24), rot=0, justify="left bottom")

    # CANH (pin 7) and CANL (pin 6) with ISO 11898-2 split 120R termination + TVS ESD
    p_canh = u3.pin("7")
    p_canl = u3.pin("6")
    p_vref = u3.pin("5")

    canh_y = 165.10
    canl_y = 180.34
    sch.wire_poly([p_canh, (320.04, p_canh[1]), (320.04, canh_y), (368.3, canh_y)])
    sch.wire_poly([p_canl, (320.04, p_canl[1]), (320.04, canl_y), (368.3, canl_y)])
    sch.label("CANH", (368.3, canh_y), rot=0, justify="right bottom")
    sch.label("CANL", (368.3, canl_y), rot=0, justify="right bottom")

    # Split termination: R6 (60R) + R7 (60R) + C11 (4.7nF) tied to VREF
    r6 = sch.add_symbol("Device:R", "R6", "60", 340.36, 168.91)
    r7 = sch.add_symbol("Device:R", "R7", "60", 340.36, 176.53)
    sch.wire((340.36, canh_y), r6.pin("1"))
    sch.wire(r6.pin("2"), r7.pin("1"))
    sch.wire(r7.pin("2"), (340.36, canl_y))
    mid_pt = r6.pin("2")
    sch.wire_poly([p_vref, (327.66, p_vref[1]), (327.66, 172.72), mid_pt])
    c11 = sch.add_symbol("Device:C", "C11", "4.7nF", 347.98, 195.58)
    sch.wire_poly([mid_pt, (347.98, 172.72), c11.pin("1")])
    sch.wire(c11.pin("2"), (347.98, 205.74))
    sch.label("GND_ISO", (347.98, 205.74), rot=0, justify="left bottom")

    # Bidirectional TVS ESD protection diodes on CANH and CANL to GND_ISO
    d1 = sch.add_symbol("Device:D_TVS", "D1", "PESD1CAN", 360.68, 152.4, rot=90)
    d2 = sch.add_symbol("Device:D_TVS", "D2", "PESD1CAN", 360.68, 195.58, rot=90)
    sch.wire(d1.pin("1"), (360.68, canh_y))
    sch.wire(d1.pin("2"), (360.68, 142.24))
    sch.label("GND_ISO", (360.68, 142.24), rot=0, justify="left bottom")

    sch.wire(d2.pin("2"), (360.68, canl_y))
    sch.wire(d2.pin("1"), (360.68, 205.74))
    sch.label("GND_ISO", (360.68, 205.74), rot=0, justify="left bottom")

    sch.add_note(
        "Design Notes (Task 46 - CAN_ISOLATED_FULL):\n"
        "- Pure galvanic isolation between primary GND and secondary GND_ISO.\n"
        "- ISO7741: Ch A (INA->OUTA) TXD forward; Ch D (OUTD->IND) RXD reverse.\n"
        "- SN65HVD230: RS=10k slope control; ISO 11898-2 split 120R termination via VREF + 4.7nF.\n"
        "- NXE1S0303MC: 4.7uH + 10uF/100nF EMI input filter, 10uF/100nF + 3.3k min load on output.",
        33.02,
        226.06,
        size=1.524,
    )

    return sch.write(out_dir, "t46_can_isolated.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t46(out)
    print("Wrote:", p)
