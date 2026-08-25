Random thoughts about how the beam simulation needs to work.

## Individual Contstraints/Formulas

### Beam Movement over Time

Function of:

- DVG state machine
  - This is backed by the currently unused PROM
  - Runs at 1.5MHz per step
  - Unsure how exactly it counts (1 step in X/Y per cycle?)

### Beam Deflection

Function of:

- Commanded position from the timeline
- Yoke and gun
  - Massless, but there may still be momentum in the coils/caps
  - Per display, and maybe different for X vs Y
  - Likely has overshoot and possibly some oscillation
- DAC change speed
  - DVG steps one bit at a time

### Energy Deposition

How much energy lands on each bit of phosphor

Function of:

- Beam spot shape
- Movement speed/time over a spot
  - Energy per unit = Input current / velocity?
- Beam current

### Beam Spot Size

Function of:

- Focus
  - Specific to the display
- Beam current

### Phosphor Decay

Energy imparted on a unit of phosphor will emit light for a limited period

Function of:

- Energy stored in the cell
- Time
- Phosphor type
  - Specific to the display

Maybe encode these in some sort of "energy over time" metric?

This may also impart a color change on multi-gun displays

### Glass Scattering

Light coming out of the glass faceplate of the display. Will spread out phosphor light

Function of:

- Phosphor grain
  - Specific to the display
- Internal reflection
  - Specific to the display

These two functions work on very different scales, so they may need to be modeled differently.

### Output

Function of:

- Above constraints
- Exposure
- Tonemap
  - For example has to cover brighter colors appearing white
