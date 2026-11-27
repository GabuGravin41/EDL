#!/usr/bin/env python3
"""Task 51: ISOLATED_DAQ_4CH — 4-ch isolated DAQ (4x ADS7042 + ISO7741 + NXE1S0303MC + REF3033)."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t51(out_dir: str) -> str:
    sch = Schematic(
        title="Task 51: 4-Channel Isolated Data Acquisition System (ISOLATED_DAQ_4CH)",
        project_name="t51_daq_4ch",
        paper="A2",
        comment1="4x ADS7042 12-bit 1-MSPS SAR ADCs + ISO7741 SPI Isolator + NXE1S0303MC Iso DC-DC + REF3033 3.3V Ref",
        comment2="Host: VDD (3.3V), GND, SCLK, MOSI, CS, MISO | Isolated DAQ: VDD_ISO, GND_ISO, VREF_3V3, AIN1..AIN4",
    )

    # A2: 594 x 420 mm
    sch.add_section_box("PRIMARY HOST DOMAIN (3.3V / GND)", 25.4, 25.4, 165.1, 368.3)
    sch.add_section_box("ISOLATION BARRIER", 167.64, 25.4, 223.52, 368.3)
    sch.add_section_box("ISOLATED POWER & 3.3V REF (VDD_ISO / VREF_3V3)", 226.06, 25.4, 568.96, 121.92)
    sch.add_section_box("4-CHANNEL ISOLATED SAR ADC ARRAY (4x ADS7042 + RC ANTI-ALIASING)", 226.06, 124.46, 568.96, 368.3)

    # =========================================================================
    # 1. HOST SPI & POWER CONNECTOR (J1: VDD, GND, SCLK, MOSI, CS, MISO)
    # =========================================================================
    j1 = sch.add_symbol("Connector_Generic:Conn_01x06", "J1", "HOST_SPI_IF", 53.34, 198.12, mirror="y")
    sch.stub_label(j1, "1", "VDD", 12.7)
    sch.stub_label(j1, "2", "SCLK", 12.7)
    sch.stub_label(j1, "3", "CS", 12.7)
    sch.stub_label(j1, "4", "MOSI", 12.7)
    sch.stub_label(j1, "5", "MISO", 12.7)
    sch.pin_gnd(j1, "6", 5.08)

    sch.add_pwr_flag_on_net("GND", 45.72, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("VDD", 45.72, 40.64, is_gnd=False)

    # =========================================================================
    # 2. ISOLATED DC-DC POWER SUPPLY: NXE1S0303MC (U6) + REF3033 (U7)
    # =========================================================================
    u6 = sch.add_symbol("test:NXE1S0303MC", "U6", "NXE1S0303MC", 195.58, 76.2)
    sch.no_connect(u6.pin("14"))  # NC

    # Primary EMI filter: VDD -> L1 (4.7uH) -> U6.+VIN with C1 (10uF), C2 (100nF) to GND
    l1 = sch.add_symbol("Device:L", "L1", "4.7uH", 147.32, 71.12, rot=90)
    c1 = sch.add_symbol("Device:C", "C1", "10uF", 132.08, 81.28)
    c2 = sch.add_symbol("Device:C", "C2", "100nF", 160.02, 81.28)

    sch.wire((116.84, 71.12), l1.pin("1"))
    sch.label("VDD", (116.84, 71.12), rot=0, justify="right bottom")
    sch.wire((132.08, 71.12), c1.pin("1"))
    sch.wire(l1.pin("2"), u6.pin("3"))
    sch.wire((160.02, 71.12), c2.pin("1"))

    gnd_y = 91.44
    sch.wire(c1.pin("2"), (132.08, gnd_y))
    sch.wire(c2.pin("2"), (160.02, gnd_y))
    sch.wire_ortho(u6.pin("1"), (172.72, gnd_y), horiz_first=True)
    sch.wire((132.08, gnd_y), (172.72, gnd_y))
    sch.add_power("power:GND", 147.32, gnd_y + 2.54, rot=0)
    sch.wire((147.32, gnd_y), (147.32, gnd_y + 2.54))

    # Secondary VDD_ISO filter: C3 (10uF), C4 (100nF), R1 (3.3k min load)
    c3 = sch.add_symbol("Device:C", "C3", "10uF", 241.3, 81.28)
    c4 = sch.add_symbol("Device:C", "C4", "100nF", 256.54, 81.28)
    r1 = sch.add_symbol("Device:R", "R1", "3.3k", 271.78, 81.28)
    sch.wire(u6.pin("8"), (304.8, 71.12))
    sch.wire((241.3, 71.12), c3.pin("1"))
    sch.wire((256.54, 71.12), c4.pin("1"))
    sch.wire((271.78, 71.12), r1.pin("1"))
    sch.label("VDD_ISO", (287.02, 71.12), rot=0, justify="left bottom")

    sch.wire_ortho(u6.pin("7"), (228.6, gnd_y), horiz_first=True)
    sch.wire((228.6, gnd_y), (304.8, gnd_y))
    sch.wire(c3.pin("2"), (241.3, gnd_y))
    sch.wire(c4.pin("2"), (256.54, gnd_y))
    sch.wire(r1.pin("2"), (271.78, gnd_y))
    sch.label("GND_ISO", (287.02, gnd_y), rot=0, justify="left bottom")

    # Precision Voltage Reference: REF3033 (U7) — VDD_ISO -> VREF_3V3 (powers AVDD of all 4 ADCs)
    # REF3033 pins: IN=(-2.54, 7.62) top, GND=(-2.54, -7.62) bottom, OUT=(7.62, 0) right
    u7 = sch.add_symbol("test:REF3033", "U7", "REF3033", 355.6, 81.28)
    p_ref_in = u7.pin("1")   # (353.06, 73.66)
    p_ref_gnd = u7.pin("3")  # (353.06, 88.90)
    p_ref_out = u7.pin("2")  # (363.22, 81.28)

    # Connect REF3033.IN to VDD_ISO with C5 (1uF) + C6 (100nF)
    sch.wire_poly([(304.8, 71.12), (353.06, 71.12), p_ref_in])
    c5 = sch.add_symbol("Device:C", "C5", "1uF", 322.58, 81.28)
    c6 = sch.add_symbol("Device:C", "C6", "100nF", 335.28, 81.28)
    sch.wire((322.58, 71.12), c5.pin("1"))
    sch.wire((335.28, 71.12), c6.pin("1"))
    sch.wire(c5.pin("2"), (322.58, gnd_y))
    sch.wire(c6.pin("2"), (335.28, gnd_y))
    sch.wire_poly([p_ref_gnd, (353.06, gnd_y), (304.8, gnd_y)])

    # Connect REF3033.OUT -> VREF_3V3 with C7 (1uF) + C8 (100nF) to GND_ISO
    sch.wire(p_ref_out, (421.64, p_ref_out[1]))
    sch.label("VREF_3V3", (421.64, p_ref_out[1]), rot=0, justify="left bottom")
    c7 = sch.add_symbol("Device:C", "C7", "1uF", 386.08, 91.44)
    c8 = sch.add_symbol("Device:C", "C8", "100nF", 401.32, 91.44)
    sch.wire((386.08, p_ref_out[1]), c7.pin("1"))
    sch.wire((401.32, p_ref_out[1]), c8.pin("1"))
    sch.wire(c7.pin("2"), (386.08, 104.14))
    sch.wire(c8.pin("2"), (401.32, 104.14))
    sch.wire((386.08, 104.14), (416.56, 104.14))
    sch.label("GND_ISO", (416.56, 104.14), rot=0, justify="left bottom")

    # =========================================================================
    # 3. SPI DIGITAL ISOLATOR: ISO7741 (U5)
    # =========================================================================
    u5 = sch.add_symbol("test:ISO7741", "U5", "ISO7741", 195.58, 228.6)

    # Side 1 Power (Vcc1=1, GND1=8) + decoupling C9 (100nF), C10 (1uF)
    p_v1 = u5.pin("1")
    sch.wire_poly([p_v1, (p_v1[0], p_v1[1] - 10.16), (149.86, p_v1[1] - 10.16)])
    sch.label("VDD", (149.86, p_v1[1] - 10.16), rot=0, justify="right bottom")
    c9 = sch.add_symbol("Device:C", "C9", "100nF", 172.72, p_v1[1] - 2.54)
    c10 = sch.add_symbol("Device:C", "C10", "1uF", 160.02, p_v1[1] - 2.54)
    sch.wire((172.72, p_v1[1] - 10.16), c9.pin("1"))
    sch.wire((160.02, p_v1[1] - 10.16), c10.pin("1"))
    sch.pin_gnd(c9, "2", 2.54)
    sch.pin_gnd(c10, "2", 2.54)
    sch.pin_gnd(u5, "8", 5.08)

    # Side 2 Power (Vcc2=16, GND2=9) + decoupling C11 (100nF), C12 (1uF)
    p_v2 = u5.pin("16")
    p_g2 = u5.pin("9")
    sch.wire_poly([p_v2, (p_v2[0], p_v2[1] - 10.16), (254.0, p_v2[1] - 10.16)])
    sch.label("VDD_ISO", (254.0, p_v2[1] - 10.16), rot=0, justify="left bottom")
    c11 = sch.add_symbol("Device:C", "C11", "100nF", 228.6, p_v2[1] - 2.54)
    c12 = sch.add_symbol("Device:C", "C12", "1uF", 241.3, p_v2[1] - 2.54)
    sch.wire((228.6, p_v2[1] - 10.16), c11.pin("1"))
    sch.wire((241.3, p_v2[1] - 10.16), c12.pin("1"))
    sch.wire(c11.pin("2"), (228.6, p_v2[1] + 5.08))
    sch.wire(c12.pin("2"), (241.3, p_v2[1] + 5.08))
    sch.wire((228.6, p_v2[1] + 5.08), (254.0, p_v2[1] + 5.08))
    sch.label("GND_ISO", (254.0, p_v2[1] + 5.08), rot=0, justify="left bottom")

    sch.wire_poly([p_g2, (p_g2[0], p_g2[1] + 7.62), (228.6, p_g2[1] + 7.62)])
    sch.label("GND_ISO", (228.6, p_g2[1] + 7.62), rot=0, justify="left bottom")

    # SPI Channels across ISO7741:
    # Ch A: SCLK -> INA (2) -> OUTA (15) -> SCLK_ISO
    # Ch B: CS   -> INB (3) -> OUTB (14) -> CS_ISO (with 10k pull-up to VDD_ISO)
    # Ch C: MOSI -> INC (4) -> OUTC (13) -> MOSI_ISO (with 10k pull-down to GND_ISO)
    # Ch D: MISO <- IND (5) <- OUTD (12) <- MISO_ISO (reverse channel)
    # Unused: INE (6), INF (7) pulled down via 10k R_TIE to GND; OUTE (11), OUTF (10) no_connect
    spi_left = [("2", "SCLK", "R_SCLK"), ("3", "CS", "R_CS"), ("4", "MOSI", "R_MOSI"), ("5", "MISO", "R_MISO")]
    for idx, (pnum, sig, rref) in enumerate(spi_left):
        pt = u5.pin(pnum)
        # Fan vertically with 10.16mm spacing so horizontal 33R damping resistors have plenty of breathing room
        ty = 213.36 + idx * 10.16
        jx = 177.8 - idx * 2.54
        r_ser = sch.add_symbol("Device:R", rref, "33", 149.86, ty, rot=90)
        sch.wire_poly([pt, (jx, pt[1]), (jx, ty), r_ser.pin("2")])
        sch.wire(r_ser.pin("1"), (127.0, ty))
        sch.label(sig, (127.0, ty), rot=0, justify="right bottom")

    # Unused INE (6), INF (7) -> 10k pull-down R_TIE to GND
    r_tie = sch.add_symbol("Device:R", "R_TIE", "10k", 172.72, 261.62)
    sch.wire_poly([u5.pin("6"), (172.72, u5.pin("6")[1]), r_tie.pin("1")])
    sch.wire(u5.pin("7"), (172.72, u5.pin("7")[1]))
    sch.pin_gnd(r_tie, "2", 2.54)

    # Isolated SPI outputs on right of ISO7741
    spi_right = [("15", "SCLK_ISO"), ("14", "CS_ISO"), ("13", "MOSI_ISO"), ("12", "MISO_ISO")]
    for idx, (pnum, sig) in enumerate(spi_right):
        pt = u5.pin(pnum)
        ty = 213.36 + idx * 10.16
        jx = 213.36 + idx * 2.54
        sch.wire_poly([pt, (jx, pt[1]), (jx, ty), (248.92, ty)])
        sch.label(sig, (248.92, ty), rot=0, justify="left bottom")

    # Pull-up on CS_ISO (10k to VDD_ISO) and pull-down on MOSI_ISO (10k to GND_ISO)
    r_cs_pu = sch.add_symbol("Device:R", "R_CSPU", "10k", 238.76, 200.66)
    sch.wire_poly([(238.76, 223.52), r_cs_pu.pin("2")])
    sch.wire(r_cs_pu.pin("1"), (238.76, 190.5))
    sch.label("VDD_ISO", (238.76, 190.5), rot=0, justify="left bottom")

    r_mosi_pd = sch.add_symbol("Device:R", "R_MOSIPD", "10k", 241.3, 256.54)
    sch.wire_poly([(241.3, 233.68), r_mosi_pd.pin("1")])
    sch.wire(r_mosi_pd.pin("2"), (241.3, 266.7))
    sch.label("GND_ISO", (241.3, 266.7), rot=0, justify="left bottom")

    sch.no_connect(u5.pin("11"))  # OUTE
    sch.no_connect(u5.pin("10"))  # OUTF

    # =========================================================================
    # 4. FOUR ADS7042 12-BIT SAR ADCs (U1..U4) IN A CLEAN 2x2 GRID
    # =========================================================================
    # Analog input connector J2 (AIN1..AIN4, GND_ISO) on right side of isolated section
    j2 = sch.add_symbol("Connector_Generic:Conn_01x05", "J2", "ANALOG_IN", 551.18, 76.2)
    sch.stub_label(j2, "1", "AIN1", 7.62)
    sch.stub_label(j2, "2", "AIN2", 7.62)
    sch.stub_label(j2, "3", "AIN3", 7.62)
    sch.stub_label(j2, "4", "AIN4", 7.62)
    sch.stub_label(j2, "5", "GND_ISO", 7.62)

    adc_positions = [
        ("U1", 1, 330.2, 185.42),
        ("U2", 2, 482.6, 185.42),
        ("U3", 3, 330.2, 297.18),
        ("U4", 4, 482.6, 297.18),
    ]

    for uref, ch, ax, ay in adc_positions:
        adc = sch.add_symbol("test:ADS7042", uref, "ADS7042", ax, ay)

        # Analog input RC anti-aliasing / kickback filter: AIN<ch> -> R_IN<ch> (100R) -> AINP (pin 6) + C_IN<ch> (1nF) to GND_ISO
        p_ainp = adc.pin("6")
        p_ainm = adc.pin("5")
        r_in = sch.add_symbol("Device:R", f"R_IN{ch}", "100", p_ainp[0] - 20.32, p_ainp[1], rot=90)
        sch.wire(r_in.pin("2"), p_ainp)
        sch.wire(r_in.pin("1"), (r_in.pin("1")[0] - 12.7, p_ainp[1]))
        sch.label(f"AIN{ch}", (r_in.pin("1")[0] - 12.7, p_ainp[1]), rot=0, justify="right bottom")

        c_in = sch.add_symbol("Device:C", f"C_IN{ch}", "1nF", p_ainp[0] - 7.62, p_ainp[1] + 15.24)
        sch.wire((p_ainp[0] - 7.62, p_ainp[1]), c_in.pin("1"))
        sch.wire(c_in.pin("2"), (p_ainp[0] - 7.62, p_ainp[1] + 25.4))
        sch.label("GND_ISO", (p_ainp[0] - 7.62, p_ainp[1] + 25.4), rot=0, justify="right bottom")

        # AINM (pin 5) -> GND_ISO (single-ended mode per datasheet)
        sch.wire_poly([p_ainm, (p_ainm[0] - 15.24, p_ainm[1]), (p_ainm[0] - 15.24, p_ainm[1] + 5.08)])
        sch.label("GND_ISO", (p_ainm[0] - 15.24, p_ainm[1] + 5.08), rot=0, justify="right bottom")

        # AVDD (pin 7) -> VREF_3V3 with local 1uF + 100nF caps to GND_ISO
        p_avdd = adc.pin("7")  # (ax - 2.54, ay - 10.16)
        p_dvdd = adc.pin("1")  # (ax, ay - 10.16)
        p_agnd = adc.pin("8")  # (ax, ay + 10.16)

        sch.wire_poly([p_avdd, (p_avdd[0], p_avdd[1] - 17.78), (p_avdd[0] - 33.02, p_avdd[1] - 17.78)])
        sch.label("VREF_3V3", (p_avdd[0] - 33.02, p_avdd[1] - 17.78), rot=0, justify="right bottom")
        c_av1 = sch.add_symbol("Device:C", f"C_AV{ch}A", "1uF", p_avdd[0] - 12.7, p_avdd[1] - 10.16)
        c_av2 = sch.add_symbol("Device:C", f"C_AV{ch}B", "100nF", p_avdd[0] - 25.4, p_avdd[1] - 10.16)
        sch.wire((p_avdd[0] - 12.7, p_avdd[1] - 17.78), c_av1.pin("1"))
        sch.wire((p_avdd[0] - 25.4, p_avdd[1] - 17.78), c_av2.pin("1"))
        sch.wire(c_av1.pin("2"), (p_avdd[0] - 12.7, p_avdd[1] - 2.54))
        sch.wire(c_av2.pin("2"), (p_avdd[0] - 25.4, p_avdd[1] - 2.54))
        sch.wire((p_avdd[0] - 25.4, p_avdd[1] - 2.54), (p_avdd[0] - 12.7, p_avdd[1] - 2.54))
        sch.label("GND_ISO", (p_avdd[0] - 25.4, p_avdd[1] - 2.54), rot=0, justify="right bottom")

        # DVDD (pin 1) -> VDD_ISO with local 100nF cap to GND_ISO
        sch.wire_poly([p_dvdd, (p_dvdd[0], p_dvdd[1] - 17.78), (p_dvdd[0] + 22.86, p_dvdd[1] - 17.78)])
        sch.label("VDD_ISO", (p_dvdd[0] + 22.86, p_dvdd[1] - 17.78), rot=0, justify="left bottom")
        c_dv = sch.add_symbol("Device:C", f"C_DV{ch}", "100nF", p_dvdd[0] + 12.7, p_dvdd[1] - 10.16)
        sch.wire((p_dvdd[0] + 12.7, p_dvdd[1] - 17.78), c_dv.pin("1"))
        sch.wire(c_dv.pin("2"), (p_dvdd[0] + 12.7, p_dvdd[1] - 2.54))
        sch.label("GND_ISO", (p_dvdd[0] + 12.7, p_dvdd[1] - 2.54), rot=0, justify="left bottom")

        # GND (pin 8) -> GND_ISO
        sch.wire(p_agnd, (p_agnd[0], p_agnd[1] + 7.62))
        sch.label("GND_ISO", (p_agnd[0], p_agnd[1] + 7.62), rot=0, justify="left bottom")

        # SPI Pins on right of ADS7042: SDO (3), SCLK (2), ~{CS} (4)
        p_sdo = adc.pin("3")
        p_sclk = adc.pin("2")
        p_cs = adc.pin("4")

        # SDO -> 100R series resistor -> MISO_ISO
        r_sdo = sch.add_symbol("Device:R", f"R_SDO{ch}", "100", p_sdo[0] + 15.24, p_sdo[1] - 5.08, rot=90)
        sch.wire_poly([p_sdo, (p_sdo[0] + 5.08, p_sdo[1]), (p_sdo[0] + 5.08, p_sdo[1] - 5.08), r_sdo.pin("1")])
        sch.wire(r_sdo.pin("2"), (r_sdo.pin("2")[0] + 10.16, p_sdo[1] - 5.08))
        sch.label("MISO_ISO", (r_sdo.pin("2")[0] + 10.16, p_sdo[1] - 5.08), rot=0, justify="left bottom")

        # SCLK -> 33R series resistor -> SCLK_ISO
        r_clk = sch.add_symbol("Device:R", f"R_CLK{ch}", "33", p_sclk[0] + 15.24, p_sclk[1] + 2.54, rot=90)
        sch.wire_poly([p_sclk, (p_sclk[0] + 7.62, p_sclk[1]), (p_sclk[0] + 7.62, p_sclk[1] + 2.54), r_clk.pin("1")])
        sch.wire(r_clk.pin("2"), (r_clk.pin("2")[0] + 10.16, p_sclk[1] + 2.54))
        sch.label("SCLK_ISO", (r_clk.pin("2")[0] + 10.16, p_sclk[1] + 2.54), rot=0, justify="left bottom")

        # ~{CS} -> CS_ISO
        sch.wire_poly([p_cs, (p_cs[0] + 5.08, p_cs[1]), (p_cs[0] + 5.08, p_cs[1] + 7.62), (p_cs[0] + 27.94, p_cs[1] + 7.62)])
        sch.label("CS_ISO", (p_cs[0] + 27.94, p_cs[1] + 7.62), rot=0, justify="left bottom")

    sch.add_note(
        "Design Notes (Task 51 - ISOLATED_DAQ_4CH):\n"
        "- Full galvanic isolation between host domain (VDD/GND) and DAQ domain (VDD_ISO/GND_ISO).\n"
        "- REF3033 generates precision 3.3V VREF_3V3 powering AVDD (ADC reference) of all four ADS7042 ADCs.\n"
        "- Each ADS7042 has a 100R / 1nF C0G anti-aliasing + charge-kickback filter, local AVDD/DVDD decoupling, and SPI damping.\n"
        "- ISO7741 isolates SCLK, CS, MOSI (forward Ch A-C) and MISO (reverse Ch D).",
        33.02,
        335.28,
        size=1.524,
    )

    return sch.write(out_dir, "t51_daq_4ch.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t51(out)
    print("Wrote:", p)
