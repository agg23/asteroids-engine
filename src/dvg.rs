use crate::types::DrawCommand;

pub struct DVG {
    // 12 bit PC
    pc: u16,

    rom: [u8; 0x800],
    ram: [u8; 0x1000],
    state_prom: [u8; 0x100],

    pub is_halted: bool,

    // Stack is a set of 4, 12 bit registers
    stack: [u16; 4],

    // 12 bits. In real hardware, top 4 bits are intensity (split out here though)
    dvx: u16,
    // 12 bits. In real hardware, top 4 bits are opcode (split out here though)
    dvy: u16,

    // 12 bits each
    current_x: u16,
    current_y: u16,

    global_scale: u8,

    // 4 bits
    opcode: u8,
    intensity: u8,

    // 4 bits
    state: u8,
}

impl DVG {
    pub fn new(rom: [u8; 0x800], state_prom: [u8; 0x100]) -> Self {
        Self {
            pc: 0,
            rom,
            ram: [0; 0x1000],
            state_prom,
            is_halted: true,
            stack: [0xFFFF; 4],
            dvx: 0,
            dvy: 0,
            current_x: 0,
            current_y: 0,
            global_scale: 0,
            opcode: 0,
            intensity: 0,
            state: 0,
        }
    }

    pub fn step(&mut self, tick_counter: u64, commands: &mut Vec<DrawCommand>) -> u16 {
        let latched_opcode = self.opcode;
        let latched_is_halted = self.is_halted;

        let opcode_low = latched_opcode & 0x1 != 0;

        let tick_count =
            self.perform_microinstruction(latched_opcode, opcode_low, tick_counter, commands);

        let prom_opcode = if (latched_opcode & 0x8) != 0 {
            // Only use opcode if high bit is set
            latched_opcode & 0x7
        } else {
            0
        };

        let not_halt_int = if latched_is_halted { 0 } else { 1 };

        // Address is {~halt, opcode[2:0], current_state[3:0]}
        let mut prom_address = self.state;
        prom_address |= prom_opcode << 4;
        prom_address |= not_halt_int << 7;

        self.state = self.state_prom[prom_address as usize] & 0xF;

        tick_count
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.dvy = 0;
        self.opcode = 0;
    }

    pub fn godvg(&mut self) {
        self.is_halted = false;
        self.dvy = 0;
        self.opcode = 0;
    }

    fn perform_microinstruction(
        &mut self,
        latched_opcode: u8,
        opcode_low: bool,
        tick_counter: u64,
        commands: &mut Vec<DrawCommand>,
    ) -> u16 {
        // state[3]: Enable (~halt)
        // state[2]: If set, data latch. If unset, DVG control
        // DVG Control:
        //   state[1]: If set, bit 0 controls halt DVG/go draw vector. If unset, bit 0 controls load/store
        // Data Latch:
        //   state[1]: If set, write to DVX. If unset, write to DVY
        //   state[0]: If set, write to high byte. If unset, write to low byte
        match self.state {
            // NOP
            0x0..0x8 => {}
            // DMA Push
            // state[2] => DVG Control, state[1] => Load/Store, state[0] => Store
            // Push PC to stack
            0x8 => {
                if !opcode_low {
                    // Prevent pushing to stack on startup
                    self.stack[3] = self.stack[2];
                    self.stack[2] = self.stack[1];
                    self.stack[1] = self.stack[0];
                    self.stack[0] = self.pc;
                }
            }
            // DMA Load
            // state[2] => DVG Control, state[1] => Load/Store, state[0] => Load
            // Pop PC from stack/dvy
            0x9 => {
                if opcode_low {
                    // Low bit is high, pop PC
                    self.pc = self.stack[0];
                    self.stack[0] = self.stack[1];
                    self.stack[1] = self.stack[2];
                    self.stack[2] = self.stack[3];
                    self.stack[3] = 0xFFFF;
                } else {
                    self.pc = self.dvy;
                }
            }
            // Go Strobe: Start drawing vector
            // state[2] => DVG Control, state[1] => Halt/Go, state[0] => Go
            0xA => {
                // This places the DVG into drawing mode, and it will hold there until that vector is complete
                // For the next 2^(scale + 1) ~1.5MHz ticks, it will tick the 7497 6-bit rate multipliers, which outputs sporadic pulses based on the configured value (value / 64 * ~1.5MHz).
                // The goal is to step the CRT emitter from its current position to vector destination in a "smooth" line. The 7497 does not uniformly distribute the output,
                // which results in the line being less smooth than it could be.

                // --- Scale ---
                let local_scale = if latched_opcode == 0xF {
                    // SVEC
                    // Extract X/Y high bits as described in Pemberton, just from the latched dvx/dvy values
                    let dvx_bit = (self.dvx >> 11) & 1;
                    let dvy_bit = (self.dvy >> 11) & 1;

                    (2 + (dvx_bit << 1) + dvy_bit) as u8
                } else {
                    latched_opcode
                };

                // In the general case, the scale is the 0x0-0x9 VCTR opcode value, plus the global scale
                let scale = (self.global_scale + local_scale) & 0xF;

                // --- Duration ---
                let tick_count = if scale > 9 {
                    // Hardware bug collapses this to a point vector
                    0
                } else {
                    (2 as u16).pow(scale as u32 + 1)
                };

                let moving_negative_x = (self.dvx & 0x400) != 0;
                let moving_negative_y = (self.dvy & 0x400) != 0;

                let tick_rate_x = (self.dvx << 2) & 0xFFF;
                let tick_rate_y = (self.dvy << 2) & 0xFFF;

                let start_x = self.current_x;
                let start_y = self.current_y;

                // 12 bit counter, separate from tick_count
                let mut counter = 0;

                for _ in 0..tick_count {
                    let mut step_x = false;
                    let mut step_y = false;

                    // Each step, we strobe the two 7497s
                    // 2x 6-bit counter, so 12 bits. They are designed to only have one bit active at a time, so we OR all bits to see if we pulse
                    // Bit 0 fires every second tick, bit 1 every fourth tick, etc...
                    // This is the same as saying it pulses for a given bit X when there are X set bits at or below that slot
                    for active_bit in 0..12 {
                        let period = (2 as usize).pow(active_bit + 1);
                        let phase = period / 2 - 1;

                        if (counter % period) != phase {
                            // Nothing to do
                            continue;
                        }

                        // Pulse
                        // Match the bit in dvx/y with the bit representing the same frequency in the 7497s
                        // Bit 0 of 7497s fires 1/2 of the time. Bit 11 of dvx/y represents 2^11 / 4096 = 2048 / 4096 = 1/2
                        // When those do match up, pulse the direction corresponding to that
                        let pulse_rate_mask = (2 as u16).pow(11 - active_bit);

                        if (tick_rate_x & pulse_rate_mask) != 0 {
                            step_x = true;
                        }

                        if (tick_rate_y & pulse_rate_mask) != 0 {
                            step_y = true;
                        }
                    }

                    // We know that only one bit at most could have fired for each of X and Y, so we don't have to track each bit's step bool separately
                    if step_x {
                        self.current_x = step_position(self.current_x, moving_negative_x);
                    }

                    if step_y {
                        self.current_y = step_position(self.current_y, moving_negative_y);
                    }

                    counter = (counter + 1) & 0xFFF;
                }

                // Finished tracking from previous position to endpoint of the latest vector. Ready to return to the DVG state machine
                commands.push(DrawCommand {
                    start_tick: tick_counter,
                    duration_ticks: tick_count,

                    start_x,
                    start_y,

                    dest_x: self.current_x,
                    dest_y: self.current_y,

                    moving_negative_x,
                    moving_negative_y,

                    intensity: self.intensity,
                });

                // 1 cycle for this execution
                return tick_count + 1;
            }
            // Halt Strobe: Enter halt conditionally
            // state[2] => DVG Control, state[1] => Halt/Go, state[0] => Halt
            0xB => {
                self.is_halted = opcode_low;

                if !opcode_low {
                    // LABS
                    // Needs to emit blank move in addition to just setting the position
                    let start_x = self.current_x;
                    let start_y = self.current_y;

                    self.current_x = self.dvx;
                    self.current_y = self.dvy;

                    // This snaps directly to the location, but it will have physical latency as the coils/caps can't just jump straight there
                    commands.push(DrawCommand {
                        start_tick: tick_counter,
                        duration_ticks: 0,

                        start_x,
                        start_y,

                        dest_x: self.current_x,
                        dest_y: self.current_y,

                        moving_negative_x: false,
                        moving_negative_y: false,

                        intensity: 0,
                    });
                }
            }
            // Latch0: Latch low byte into DVY
            0xC => {
                if latched_opcode == 0xF {
                    // SVEC
                    // This fetches the low byte, unlike what Latch3 would normally do
                    self.perform_latch3();
                    self.dvy &= 0xF00;
                } else {
                    let byte = self.fetch_byte() as u16;
                    self.dvy = (self.dvy & 0xF00) | byte;
                }

                self.increment_pc();
            }
            // Latch1: Latch high byte into DVY
            0xD => {
                let byte = self.fetch_byte();
                let low_nibble = (byte & 0xF) as u16;

                self.dvy = (self.dvy & 0xFF) | (low_nibble << 8);
                self.opcode = (byte >> 4) & 0xF;

                if self.opcode == 0xF {
                    // SVEC
                    self.dvx &= 0xF00;
                    self.dvy &= 0xF00;
                }
            }
            // Latch2: Latch low byte into DVX
            0xE => {
                let byte = self.fetch_byte() as u16;
                self.dvx = (self.dvx & 0xF00) | byte;
                self.increment_pc();

                if (latched_opcode & 0xA) == 0xA {
                    // LABS
                    self.global_scale = self.intensity;
                }
            }
            // Latch3: Latch high byte into DVX
            0xF => {
                self.perform_latch3();
            }
            _ => unreachable!(),
        }

        // Everything except Gostrobe takes 1 tick
        1
    }

    pub fn read_byte(&self, address: usize) -> u8 {
        match address {
            0x0..0x1000 => self.ram[address],
            // TODO: This is half the size
            0x1000..0x1800 => self.rom[address - 0x1000],
            _ => {
                println!("Out of bounds DVG read at {address:08X}");
                0
            }
        }
    }

    pub fn write_byte(&mut self, address: usize, value: u8) {
        match address {
            0x0..0x1000 => self.ram[address] = value,
            _ => {
                println!("Invalid DVG write to {address:08X}");
            }
        }
    }

    fn fetch_byte(&self) -> u8 {
        let high_byte = (self.state & 0x1) as usize;
        // PC is a word address
        let address = ((self.pc as usize) << 1) + high_byte;

        self.read_byte(address)
    }

    fn increment_pc(&mut self) {
        self.pc = (self.pc + 1) & 0xFFF;
    }

    fn perform_latch3(&mut self) {
        let byte = self.fetch_byte();
        let low_nibble = (byte & 0xF) as u16;

        self.dvx = (self.dvx & 0xFF) | (low_nibble << 8);
        self.intensity = (byte >> 4) & 0xF;
    }
}

fn step_position(current: u16, negative: bool) -> u16 {
    if negative {
        if current == 0 { 4095 } else { current - 1 }
    } else {
        (current + 1) & 0xFFF
    }
}
