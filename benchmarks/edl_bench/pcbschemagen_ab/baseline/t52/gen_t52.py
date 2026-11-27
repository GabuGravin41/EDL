#!/usr/bin/env python3
"""Task 52: IOT_SENSOR_NODE — STM32F103C8T6 + SN65HVD230 CAN + TPS54302 12V->5V Buck + TLV1117-33 5V->3.3V LDO."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from kicad_sch_builder import Schematic


def build_t52(out_dir: str) -> str:
    sch = Schematic(
        title="Task 52: IoT Sensor Node (IOT_SENSOR_NODE)",
        project_name="t52_iot_node",
        paper="A2",
        comment1="TPS54302 (12V->5V Buck) + TLV1117-33 (5V->3.3V LDO) + STM32F103C8T6 MCU + SN65HVD230 CAN + NTC Sensor",
        comment2="Inputs: VIN12 (12V), GND | Outputs: CANH, CANL, TEMP_ADC",
    )

    # A2: 594 x 420 mm
    sch.add_section_box("POWER STAGE: 12V -> 5V SYNC BUCK (TPS54302) -> 3.3V LDO (TLV1117-33)", 25.4, 25.4, 568.96, 134.62)
    sch.add_section_box("MCU CORE: STM32F103C8T6 + CLOCKS + RESET + SWD + DECOUPLING", 25.4, 137.16, 355.6, 368.3)
    sch.add_section_box("TEMPERATURE SENSOR (TEMP_ADC)", 358.14, 137.16, 568.96, 226.06)
    sch.add_section_box("CAN BUS TRANSCEIVER (SN65HVD230 + SPLIT TERM + TVS)", 358.14, 228.6, 568.96, 368.3)

    # =========================================================================
    # 1. POWER INPUT (J1: VIN12, GND) + TPS54302 12V->5.0V BUCK (U1)
    # =========================================================================
    j1 = sch.add_symbol("Connector_Generic:Conn_01x02", "J1", "PWR_IN_12V", 45.72, 76.2, mirror="y")
    sch.stub_label(j1, "1", "VIN12", 10.16)
    sch.pin_gnd(j1, "2", 5.08)

    sch.add_pwr_flag_on_net("GND", 43.18, 48.26, is_gnd=True)
    sch.add_pwr_flag_on_net("VIN12", 43.18, 40.64, is_gnd=False)
    sch.add_pwr_flag_on_net("+5V", 81.28, 40.64, is_gnd=False)
    sch.add_pwr_flag_on_net("+3V3", 119.38, 40.64, is_gnd=False)

    # Input TVS + bulk decoupling on VIN12
    vin_y = 83.82
    sch.wire((86.36, vin_y), (172.72, vin_y))
    sch.label("VIN12", (86.36, vin_y), rot=0, justify="right bottom")
    d_in = sch.add_symbol("Device:D_TVS", "D1", "SMBJ15A", 101.6, 96.52, rot=90)
    c_in1 = sch.add_symbol("Device:C", "C1", "10uF/25V", 116.84, 96.52)
    c_in2 = sch.add_symbol("Device:C", "C2", "10uF/25V", 134.62, 96.52)
    c_in3 = sch.add_symbol("Device:C", "C3", "100nF", 152.4, 96.52)
    sch.wire((101.6, vin_y), d_in.pin("2"))
    sch.pin_gnd(d_in, "1", 2.54)
    for c_sym, cx in ((c_in1, 116.84), (c_in2, 134.62), (c_in3, 152.4)):
        sch.wire((cx, vin_y), c_sym.pin("1"))
        sch.pin_gnd(c_sym, "2", 2.54)

    # TPS54302 (U1): pin1=GND(0,0), pin2=SW(0,-2.54), pin3=VIN(0,-5.08),
    #                pin6=BOOT(27.94,0), pin5=EN(27.94,-2.54), pin4=FB(27.94,-5.08)
    # Place U1 at (172.72, 78.74) -> VIN (pin 3) is at (172.72, 83.82) == vin_y!
    u1 = sch.add_symbol("test:TPS54302", "U1", "TPS54302", 172.72, 78.74)

    # Pin 1 GND -> route left then up to GND (or left then down)
    p_u1_gnd = u1.pin("1")  # (172.72, 78.74)
    sch.wire_poly([p_u1_gnd, (162.56, p_u1_gnd[1]), (162.56, 66.04)])
    sch.add_power("power:GND", 162.56, 63.5, rot=180)
    sch.wire((162.56, 66.04), (162.56, 63.5))

    # Pin 2 SW -> route left then UP over U1 to inductor L1 (10uH) -> +5V!
    p_u1_sw = u1.pin("2")   # (172.72, 81.28)
    p_u1_boot = u1.pin("6") # (200.66, 78.74)
    sw_bus_y = 53.34
    sch.wire_poly([p_u1_sw, (167.64, p_u1_sw[1]), (167.64, sw_bus_y), (236.22, sw_bus_y)])

    # Pin 6 BOOT -> C4 (100nF) bootstrap cap -> SW bus!
    c_boot = sch.add_symbol("Device:C", "C4", "100nF", 215.9, 66.04)
    sch.wire_poly([p_u1_boot, (215.9, p_u1_boot[1]), c_boot.pin("2")])
    sch.wire(c_boot.pin("1"), (215.9, sw_bus_y))

    # Buck Inductor L1 (10uH) -> +5V rail with output caps C5 (22uF), C6 (22uF), C7 (100nF)
    l1 = sch.add_symbol("Device:L", "L1", "10uH", 240.03, sw_bus_y, rot=90)
    sch.wire(l1.pin("2"), (309.88, sw_bus_y))
    sch.label("+5V", (309.88, sw_bus_y), rot=0, justify="left bottom")
    c5 = sch.add_symbol("Device:C", "C5", "22uF", 261.62, 66.04)
    c6 = sch.add_symbol("Device:C", "C6", "22uF", 279.4, 66.04)
    c7 = sch.add_symbol("Device:C", "C7", "100nF", 297.18, 66.04)
    for c_sym, cx in ((c5, 261.62), (c6, 279.4), (c7, 297.18)):
        sch.wire((cx, sw_bus_y), c_sym.pin("1"))
        sch.pin_gnd(c_sym, "2", 2.54)

    # Pin 5 EN -> VIN12 UVLO resistor divider R1 (100k) top / R2 (22k) bottom -> V_EN = 2.16V > 1.30V
    p_u1_en = u1.pin("5")  # (200.66, 81.28)
    r_en1 = sch.add_symbol("Device:R", "R1", "100k", 231.14, 96.52)
    r_en2 = sch.add_symbol("Device:R", "R2", "22k", 231.14, 114.3)
    sch.wire_poly([p_u1_en, (223.52, p_u1_en[1]), (223.52, 104.14), (231.14, 104.14)])
    sch.wire(r_en1.pin("2"), (231.14, 104.14))
    sch.wire((231.14, 104.14), r_en2.pin("1"))
    sch.wire(r_en1.pin("1"), (231.14, 88.9))
    sch.label("VIN12", (231.14, 88.9), rot=0, justify="left bottom")
    sch.pin_gnd(r_en2, "2", 2.54)

    # Pin 4 FB -> 5.0V feedback divider: R3 (100k) || C8 (75pF C_FF) to +5V, R4 (13.5k) to GND
    # Vout = 0.596 * (1 + 100k / 13.5k) = 5.01V
    p_u1_fb = u1.pin("4")  # (200.66, 83.82)
    r_fb1 = sch.add_symbol("Device:R", "R3", "100k", 261.62, 96.52)
    c_ff = sch.add_symbol("Device:C", "C8", "75pF", 279.4, 96.52)
    r_fb2 = sch.add_symbol("Device:R", "R4", "13.5k", 261.62, 114.3)
    fb_node_y = 104.14
    sch.wire_poly([p_u1_fb, (213.36, p_u1_fb[1]), (213.36, 121.92), (248.92, 121.92), (248.92, fb_node_y), (279.4, fb_node_y)])
    sch.wire(r_fb1.pin("2"), (261.62, fb_node_y))
    sch.wire(c_ff.pin("2"), (279.4, fb_node_y))
    sch.wire((261.62, fb_node_y), r_fb2.pin("1"))
    sch.wire(r_fb1.pin("1"), (261.62, 88.9))
    sch.wire(c_ff.pin("1"), (279.4, 88.9))
    sch.wire((261.62, 88.9), (279.4, 88.9))
    sch.label("+5V", (279.4, 88.9), rot=0, justify="left bottom")
    sch.pin_gnd(r_fb2, "2", 2.54)

    # =========================================================================
    # 2. TLV1117-33 5V -> 3.3V LDO (U2)
    # =========================================================================
    # TLV1117-33 pins: pin1=ADJ/GND(0,0), pin2=OUTPUT_1(0,-2.54), pin3=INPUT(0,-5.08), pin4=OUTPUT_2(38.1,0)
    u2 = sch.add_symbol("test:TLV1117-33", "U2", "TLV1117-33", 419.1, 78.74)

    # Pin 3 INPUT -> +5V with C9 (10uF) + C10 (100nF) to GND
    p_ldo_in = u2.pin("3")  # (419.1, 83.82)
    sch.wire((368.3, p_ldo_in[1]), p_ldo_in)
    sch.label("+5V", (368.3, p_ldo_in[1]), rot=0, justify="right bottom")
    c9 = sch.add_symbol("Device:C", "C9", "10uF", 386.08, 96.52)
    c10 = sch.add_symbol("Device:C", "C10", "100nF", 401.32, 96.52)
    sch.wire((386.08, p_ldo_in[1]), c9.pin("1"))
    sch.wire((401.32, p_ldo_in[1]), c10.pin("1"))
    sch.pin_gnd(c9, "2", 2.54)
    sch.pin_gnd(c10, "2", 2.54)

    # Pin 1 ADJ/GND -> GND
    p_ldo_gnd = u2.pin("1")  # (419.1, 78.74)
    sch.wire_poly([p_ldo_gnd, (408.94, p_ldo_gnd[1]), (408.94, 66.04)])
    sch.add_power("power:GND", 408.94, 63.5, rot=180)
    sch.wire((408.94, 66.04), (408.94, 63.5))

    # Pin 2 OUTPUT_1 AND Pin 4 OUTPUT_2 (tab) BOTH connected to +3V3 with C11 (10uF) + C12 (100nF)
    p_ldo_out1 = u2.pin("2")  # (419.1, 81.28)
    p_ldo_out2 = u2.pin("4")  # (457.2, 78.74)
    v33_y = 78.74
    sch.wire_poly([p_ldo_out1, (414.02, p_ldo_out1[1]), (414.02, 53.34), (472.44, 53.34), (472.44, v33_y)])
    sch.wire(p_ldo_out2, (525.78, v33_y))
    sch.label("+3V3", (525.78, v33_y), rot=0, justify="left bottom")
    c11 = sch.add_symbol("Device:C", "C11", "10uF", 487.68, 91.44)
    c12 = sch.add_symbol("Device:C", "C12", "100nF", 505.46, 91.44)
    sch.wire((487.68, v33_y), c11.pin("1"))
    sch.wire((505.46, v33_y), c12.pin("1"))
    sch.pin_gnd(c11, "2", 2.54)
    sch.pin_gnd(c12, "2", 2.54)

    # =========================================================================
    # 3. STM32F103C8T6 MCU (U3) + CLOCKS + RESET + BOOT + ANALOG FILTER + SWD
    # =========================================================================
    u3 = sch.add_symbol("test:STM32F103C8T6", "U3", "STM32F103C8T6", 198.12, 238.76)

    # Mark unused GPIOs with no_connect cleanly
    unused_mcu_pins = (
        "11", "12", "13", "14", "15", "16", "17", "18", "19", "21", "22",
        "25", "26", "27", "28", "29", "30", "31",
        "38", "39", "40", "41", "42", "43", "45", "46",
    )
    for pn in unused_mcu_pins:
        sch.no_connect(u3.pin(pn))

    # --- Power pins on STM32 ---
    # VBAT (1) -> +3V3
    sch.stub_label(u3, "1", "+3V3", 10.16)
    # VDD_1 (24), VDD_2 (36), VDD_3 (48) -> +3V3
    sch.stub_label(u3, "24", "+3V3", 10.16)
    sch.stub_label(u3, "36", "+3V3", 10.16)
    sch.stub_label(u3, "48", "+3V3", 10.16)
    # VSS_1 (23), VSS_2 (35), VSS_3 (47), VSSA (8) -> GND
    sch.wire(u3.pin("23"), (u3.pin("23")[0] - 12.7, u3.pin("23")[1]))
    sch.add_power("power:GND", u3.pin("23")[0] - 12.7, u3.pin("23")[1] + 2.54, rot=0)
    sch.wire((u3.pin("23")[0] - 12.7, u3.pin("23")[1]), (u3.pin("23")[0] - 12.7, u3.pin("23")[1] + 2.54))

    sch.wire(u3.pin("35"), (u3.pin("35")[0] + 22.86, u3.pin("35")[1]))
    sch.add_power("power:GND", u3.pin("35")[0] + 22.86, u3.pin("35")[1] + 2.54, rot=0)
    sch.wire((u3.pin("35")[0] + 22.86, u3.pin("35")[1]), (u3.pin("35")[0] + 22.86, u3.pin("35")[1] + 2.54))

    sch.wire(u3.pin("47"), (u3.pin("47")[0] + 17.78, u3.pin("47")[1]))
    sch.add_power("power:GND", u3.pin("47")[0] + 17.78, u3.pin("47")[1] + 2.54, rot=0)
    sch.wire((u3.pin("47")[0] + 17.78, u3.pin("47")[1]), (u3.pin("47")[0] + 17.78, u3.pin("47")[1] + 2.54))

    sch.wire(u3.pin("8"), (u3.pin("8")[0] - 17.78, u3.pin("8")[1]))
    sch.add_power("power:GND", u3.pin("8")[0] - 17.78, u3.pin("8")[1] + 2.54, rot=0)
    sch.wire((u3.pin("8")[0] - 17.78, u3.pin("8")[1]), (u3.pin("8")[0] - 17.78, u3.pin("8")[1] + 2.54))

    # VDDA (9) -> +3V3_A (ferrite-filtered analog supply)
    sch.stub_label(u3, "9", "+3V3_A", 12.7)

    # Dedicated MCU Decoupling Bank (4x 100nF + 1x 4.7uF on +3V3, plus FB1 + 1uF + 100nF for +3V3_A)
    dec_y = 322.58
    sch.wire((45.72, dec_y), (167.64, dec_y))
    sch.label("+3V3", (45.72, dec_y), rot=0, justify="right bottom")
    dec_vals = [("C13", "4.7uF"), ("C14", "100nF"), ("C15", "100nF"), ("C16", "100nF"), ("C17", "100nF")]
    for idx, (cref, cval) in enumerate(dec_vals):
        cx = 58.42 + idx * 17.78
        cs = sch.add_symbol("Device:C", cref, cval, cx, dec_y + 10.16)
        sch.wire((cx, dec_y), cs.pin("1"))
        sch.pin_gnd(cs, "2", 2.54)

    # Ferrite bead FB1 (600R@100MHz) -> +3V3_A with C18 (1uF) + C19 (100nF)
    fb1 = sch.add_symbol("Device:FerriteBead", "FB1", "600R@100MHz", 171.45, dec_y, rot=90)
    sch.wire(fb1.pin("2"), (228.6, dec_y))
    sch.label("+3V3_A", (228.6, dec_y), rot=0, justify="left bottom")
    c18 = sch.add_symbol("Device:C", "C18", "1uF", 195.58, dec_y + 10.16)
    c19 = sch.add_symbol("Device:C", "C19", "100nF", 213.36, dec_y + 10.16)
    sch.wire((195.58, dec_y), c18.pin("1"))
    sch.wire((213.36, dec_y), c19.pin("1"))
    sch.pin_gnd(c18, "2", 2.54)
    sch.pin_gnd(c19, "2", 2.54)

    # --- Status LED on PC13 (pin 2) ---
    sch.stub_label(u3, "2", "LED_STAT", 12.7)
    r_led = sch.add_symbol("Device:R", "R5", "1k", 281.94, 330.2, rot=90)
    led1 = sch.add_symbol("Device:LED", "D2", "STATUS_LED", 302.26, 330.2)
    sch.wire((266.7, 330.2), r_led.pin("1"))
    sch.label("LED_STAT", (266.7, 330.2), rot=0, justify="right bottom")
    sch.wire(r_led.pin("2"), led1.pin("1"))
    sch.wire(led1.pin("2"), (317.5, 330.2))
    sch.label("+3V3", (317.5, 330.2), rot=0, justify="left bottom")

    # --- LSE 32.768 kHz Crystal (Y2) on PC14 (3) / PC15 (4) ---
    sch.stub_label(u3, "3", "OSC32_IN", 15.24)
    sch.stub_label(u3, "4", "OSC32_OUT", 15.24)
    y2 = sch.add_symbol("Device:Crystal", "Y2", "32.768kHz", 81.28, 165.1)
    c20 = sch.add_symbol("Device:C", "C20", "12pF", 68.58, 177.8)
    c21 = sch.add_symbol("Device:C", "C21", "12pF", 93.98, 177.8)
    sch.wire((60.96, 165.1), y2.pin("1"))
    sch.wire((68.58, 165.1), c20.pin("1"))
    sch.label("OSC32_IN", (60.96, 165.1), rot=0, justify="right bottom")
    sch.wire(y2.pin("2"), (101.6, 165.1))
    sch.wire((93.98, 165.1), c21.pin("1"))
    sch.label("OSC32_OUT", (101.6, 165.1), rot=0, justify="left bottom")
    sch.pin_gnd(c20, "2", 2.54)
    sch.pin_gnd(c21, "2", 2.54)

    # --- HSE 8 MHz Crystal (Y1) on PD0 (5) / PD1 (6) ---
    sch.stub_label(u3, "5", "OSC_IN", 15.24)
    sch.stub_label(u3, "6", "OSC_OUT", 15.24)
    y1 = sch.add_symbol("Device:Crystal", "Y1", "8MHz", 81.28, 208.28)
    c22 = sch.add_symbol("Device:C", "C22", "20pF", 68.58, 220.98)
    c23 = sch.add_symbol("Device:C", "C23", "20pF", 93.98, 220.98)
    sch.wire((60.96, 208.28), y1.pin("1"))
    sch.wire((68.58, 208.28), c22.pin("1"))
    sch.label("OSC_IN", (60.96, 208.28), rot=0, justify="right bottom")
    sch.wire(y1.pin("2"), (101.6, 208.28))
    sch.wire((93.98, 208.28), c23.pin("1"))
    sch.label("OSC_OUT", (101.6, 208.28), rot=0, justify="left bottom")
    sch.pin_gnd(c22, "2", 2.54)
    sch.pin_gnd(c23, "2", 2.54)

    # --- NRST (pin 7) RC reset circuit: 10k pull-up to +3V3 + 100nF to GND ---
    sch.stub_label(u3, "7", "NRST", 15.24)
    r_rst = sch.add_symbol("Device:R", "R6", "10k", 76.2, 256.54)
    c_rst = sch.add_symbol("Device:C", "C24", "100nF", 76.2, 274.32)
    sch.wire(r_rst.pin("1"), (76.2, 246.38))
    sch.label("+3V3", (76.2, 246.38), rot=0, justify="left bottom")
    sch.wire(r_rst.pin("2"), c_rst.pin("1"))
    sch.wire((76.2, 265.43), (93.98, 265.43))  # wait: snap midpoint to 1.27mm grid!
    # r_rst at 256.54 -> pin2 at 260.35; c_rst at 274.32 -> pin1 at 270.51; tap at 264.16!
    sch.wires.pop()
    sch.wire((76.2, 264.16), (93.98, 264.16))
    sch.label("NRST", (93.98, 264.16), rot=0, justify="left bottom")
    sch.pin_gnd(c_rst, "2", 2.54)

    # --- BOOT0 (44) & BOOT1/PB2 (20): 10k pull-downs to GND ---
    sch.stub_label(u3, "20", "BOOT1", 10.16)
    r_b1 = sch.add_symbol("Device:R", "R7", "10k", 116.84, 274.32)
    sch.wire(r_b1.pin("1"), (116.84, 266.7))
    sch.label("BOOT1", (116.84, 266.7), rot=0, justify="left bottom")
    sch.pin_gnd(r_b1, "2", 2.54)

    sch.stub_label(u3, "44", "BOOT0", 10.16)
    r_b0 = sch.add_symbol("Device:R", "R8", "10k", 281.94, 226.06)
    sch.wire(r_b0.pin("1"), (281.94, 218.44))
    sch.label("BOOT0", (281.94, 218.44), rot=0, justify="left bottom")
    sch.pin_gnd(r_b0, "2", 2.54)

    # --- SWD Debug Header J2 (+3V3, SWDIO, SWCLK, NRST, GND) ---
    sch.stub_label(u3, "34", "SWDIO", 12.7)
    sch.stub_label(u3, "37", "SWCLK", 12.7)
    j2 = sch.add_symbol("Connector_Generic:Conn_01x05", "J2", "SWD_DBG", 330.2, 241.3)
    sch.stub_label(j2, "1", "+3V3", 10.16)
    sch.stub_label(j2, "2", "SWDIO", 10.16)
    sch.stub_label(j2, "3", "SWCLK", 10.16)
    sch.stub_label(j2, "4", "NRST", 10.16)
    sch.pin_gnd(j2, "5", 5.08)

    # --- CAN1 & ADC Connections on STM32 ---
    sch.stub_label(u3, "10", "TEMP_ADC", 15.24)  # PA0 / ADC1_IN0
    sch.stub_label(u3, "32", "CAN_RX", 12.7)     # PA11 / CAN_RX
    sch.stub_label(u3, "33", "CAN_TX", 12.7)     # PA12 / CAN_TX

    # =========================================================================
    # 4. TEMPERATURE SENSOR CIRCUIT (TEMP_ADC)
    # =========================================================================
    # +3V3_A -> R9 (10k 1%) -> TEMP_ADC -> TH1 (10k NTC) -> GND, with C25 (100nF) RC filter + J3 monitor header
    r_ntc = sch.add_symbol("Device:R", "R9", "10k", 419.1, 165.1)
    th1 = sch.add_symbol("Device:Thermistor_NTC", "TH1", "10k_NTC", 419.1, 193.04)
    c_ntc = sch.add_symbol("Device:C", "C25", "100nF", 447.04, 193.04)
    sch.wire(r_ntc.pin("1"), (419.1, 154.94))
    sch.label("+3V3_A", (419.1, 154.94), rot=0, justify="left bottom")
    sch.wire(r_ntc.pin("2"), th1.pin("1"))
    sch.pin_gnd(th1, "2", 2.54)
    t_mid_y = 177.8
    sch.wire((419.1, t_mid_y), (474.98, t_mid_y))
    sch.wire((447.04, t_mid_y), c_ntc.pin("1"))
    sch.pin_gnd(c_ntc, "2", 2.54)
    sch.label("TEMP_ADC", (474.98, t_mid_y), rot=0, justify="left bottom")

    j3 = sch.add_symbol("Connector_Generic:Conn_01x02", "J3", "TEMP_OUT", 543.56, 177.8)
    sch.stub_label(j3, "1", "TEMP_ADC", 10.16)
    sch.pin_gnd(j3, "2", 5.08)

    # =========================================================================
    # 5. SN65HVD230 CAN TRANSCEIVER (U4)
    # =========================================================================
    u4 = sch.add_symbol("test:SN65HVD230", "U4", "SN65HVD230", 436.88, 294.64)

    sch.wire(u4.pin("1"), (396.24, u4.pin("1")[1]))
    sch.label("CAN_TX", (396.24, u4.pin("1")[1]), rot=0, justify="right bottom")
    sch.wire(u4.pin("4"), (396.24, u4.pin("4")[1]))
    sch.label("CAN_RX", (396.24, u4.pin("4")[1]), rot=0, justify="right bottom")

    # GND (2) & VCC (3) + decoupling C26 (100nF), C27 (10uF)
    sch.wire(u4.pin("2"), (414.02, u4.pin("2")[1]))
    sch.wire((414.02, u4.pin("2")[1]), (414.02, u4.pin("2")[1] + 2.54))
    sch.add_power("power:GND", 414.02, u4.pin("2")[1] + 2.54, rot=0)

    p_u4_vcc = u4.pin("3")
    sch.wire_poly([p_u4_vcc, (419.1, p_u4_vcc[1]), (419.1, 322.58), (441.96, 322.58)])
    sch.label("+3V3", (419.1, 322.58), rot=0, justify="right bottom")
    c26 = sch.add_symbol("Device:C", "C26", "100nF", 429.26, 330.2)
    c27 = sch.add_symbol("Device:C", "C27", "10uF", 441.96, 330.2)
    sch.wire((429.26, 322.58), c26.pin("1"))
    sch.wire((441.96, 322.58), c27.pin("1"))
    sch.pin_gnd(c26, "2", 2.54)
    sch.pin_gnd(c27, "2", 2.54)

    # RS (8) -> 10k slope control to GND
    r_rs = sch.add_symbol("Device:R", "R10", "10k", 459.74, 274.32)
    sch.wire_ortho(u4.pin("8"), r_rs.pin("2"), horiz_first=True)
    sch.wire(r_rs.pin("1"), (459.74, 264.16))
    sch.add_power("power:GND", 459.74, 261.62, rot=180)
    sch.wire((459.74, 264.16), (459.74, 261.62))

    # CANH (7), CANL (6), VREF (5) split 120R termination + TVS ESD + J4 CAN bus header
    canh_y = 287.02
    canl_y = 302.26
    sch.wire_poly([u4.pin("7"), (457.2, u4.pin("7")[1]), (457.2, canh_y), (513.08, canh_y)])
    sch.wire_poly([u4.pin("6"), (457.2, u4.pin("6")[1]), (457.2, canl_y), (513.08, canl_y)])
    sch.label("CANH", (513.08, canh_y), rot=0, justify="right bottom")
    sch.label("CANL", (513.08, canl_y), rot=0, justify="right bottom")

    r_t1 = sch.add_symbol("Device:R", "R11", "60", 477.52, 290.83)
    r_t2 = sch.add_symbol("Device:R", "R12", "60", 477.52, 298.45)
    sch.wire((477.52, canh_y), r_t1.pin("1"))
    sch.wire(r_t1.pin("2"), r_t2.pin("1"))
    sch.wire(r_t2.pin("2"), (477.52, canl_y))
    mid = r_t1.pin("2")
    sch.wire_poly([u4.pin("5"), (464.82, u4.pin("5")[1]), (464.82, mid[1]), mid])
    c_sp = sch.add_symbol("Device:C", "C28", "4.7nF", 485.14, 317.5)
    sch.wire_poly([mid, (485.14, mid[1]), c_sp.pin("1")])
    sch.pin_gnd(c_sp, "2", 2.54)

    d3 = sch.add_symbol("Device:D_TVS", "D3", "PESD1CAN", 497.84, 274.32, rot=90)
    d4 = sch.add_symbol("Device:D_TVS", "D4", "PESD1CAN", 497.84, 317.5, rot=90)
    sch.wire(d3.pin("1"), (497.84, canh_y))
    sch.wire(d3.pin("2"), (497.84, 264.16))
    sch.add_power("power:GND", 497.84, 261.62, rot=180)
    sch.wire((497.84, 264.16), (497.84, 261.62))
    sch.wire(d4.pin("2"), (497.84, canl_y))
    sch.pin_gnd(d4, "1", 2.54)

    j4 = sch.add_symbol("Connector_Generic:Conn_01x03", "J4", "CAN_BUS", 548.64, 294.64)
    sch.stub_label(j4, "1", "CANH", 7.62)
    sch.stub_label(j4, "2", "CANL", 7.62)
    sch.pin_gnd(j4, "3", 5.08)

    sch.add_note(
        "Design Notes (Task 52 - IOT_SENSOR_NODE):\n"
        "- TPS54302: 12V->5.0V (R3=100k||75pF, R4=13.5k -> 5.01V), EN UVLO divider 100k/22k (2.16V>1.30V), 100nF BOOT, 10uH inductor.\n"
        "- TLV1117-33: 5V->3.3V clean LDO with both OUTPUT_1 & OUTPUT_2 tied to +3V3, 10uF+100nF input/output caps.\n"
        "- STM32F103C8T6: 8MHz HSE + 32.768kHz LSE crystals, NRST RC, BOOT0/1 10k pull-downs, ferrite-filtered VDDA, PA11/PA12 CAN1, PA0 TEMP_ADC.",
        35.56,
        350.52,
        size=1.27,
    )

    return sch.write(out_dir, "t52_iot_node.kicad_sch")


if __name__ == "__main__":
    out = os.path.dirname(os.path.abspath(__file__))
    p = build_t52(out)
    print("Wrote:", p)
