# Bake-Off Raw Output: Track C (Python Control Baseline) — Pro

- **Task 1 (`Apartment_B`):** 237 lines of Python (~7,500 chars). Manually computed all 7 wall rectangles (`W_Mid_Horiz`, `W_Hall_Living`, `W_Bath_Bedroom`, etc.) and `bed_living_x0`/`x1`. Wrote ~140 lines of custom sweep-line quarter-disk intersection math to check door swings. Valid Python, accurate numbers, massive boilerplate.
- **Task 2 (`Studio_A` explanation):** 3/3 exact. Explicitly noted that the Python script hardcoded `4500` in `D_Entry` and `W_Living`, creating manual sync risk when `living_w` changes.
- **Task 3 (Surgical modification):** Required updating 3 separate locations (`living_w`, `D_Entry.seg_len`, and `W_Living.seg_len` + `offset` + `width`).
- **Task 4 (Diagnosis & repair):** Exact (`D_Bath` start jamb `40mm < 100mm`, `D_Kitchen` end jamb `50mm < 100mm`, no 90° overlap / `210mm` gap). Minimal repair preserved `hinge = "end"` and `swing = "Living"`.
