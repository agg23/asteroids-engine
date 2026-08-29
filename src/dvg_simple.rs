const MAX_SCREEN_SIZE: u16 = 1024;

pub struct DVG_Simple {
    // 12 bit PC
    pc: u16,

    pub rom: [u8; 0x800],
    pub ram: [u8; 0x1000],

    pub is_halted: bool,

    // Stack is a set of 4 registers
    stack: [u16; 4],

    current_x: u16,
    current_y: u16,

    global_scale: u8,
}

#[derive(Debug)]
pub struct VectorMove {
    // Positions are global, [0, 1024)
    pub source_x: u16,
    pub source_y: u16,

    pub dest_x: u16,
    pub dest_y: u16,

    // 4 bit intensity level. `None` implies no drawing
    pub intensity: Option<u8>,
}

enum StepResult {
    Move(VectorMove),
    Halt,
    None,
}

impl DVG_Simple {
    pub fn new(rom: [u8; 0x800]) -> Self {
        Self {
            pc: 0,
            rom,
            ram: [0; 0x1000],
            is_halted: true,
            stack: [0xFFFF; 4],
            current_x: 0,
            current_y: 0,
            global_scale: 0,
        }
    }

    pub fn resume(&mut self) {
        self.is_halted = false;
        self.pc = 0;
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.is_halted = true;
        self.stack = [0xFFFF; 4];
        self.current_x = 0;
        self.current_y = 0;
        self.global_scale = 0
    }

    pub fn step(&mut self) -> Option<VectorMove> {
        if self.is_halted {
            return None;
        }

        if let StepResult::Move(vector_move) = self.execute_instruction() {
            Some(vector_move)
        } else {
            None
        }
    }

    // pub fn run_frame(&mut self) -> Vec<VectorMove> {
    //     let mut moves = Vec::<VectorMove>::new();

    //     if self.is_halted {
    //         return moves;
    //     }

    //     let mut instruction_counter = 0;

    //     while self.execute_instruction() && instruction_counter < 100_000 {
    //         instruction_counter += 1;
    //     }

    //     moves
    // }

    /// Execute a single instruction. Returns `true` if the execution should halt
    fn execute_instruction(&mut self) -> StepResult {
        let instruction_word = self.read_next_word();
        let opcode = (instruction_word >> 12) & 0xF;

        // TODO: Do we need a cycle count for each of these? I kind of assume they are single/the same cycle length for all

        match opcode {
            // VCTR: Draw long vector
            0x0..0xA => {
                let read = self.read_xy(Some(opcode), instruction_word);

                return StepResult::Move(self.apply_delta_move(read, opcode as u8));
            }
            // LABS: Load absolute, resetting the current emitter position
            0xA => {
                let read = self.read_xy(None, instruction_word);

                self.global_scale = read.modifier;

                // let vector_move = VectorMove {
                //     source_x: self.current_x,
                //     source_y: self.current_y,
                //     dest_x: read.x as u16,
                //     dest_y: read.y as u16,
                //     intensity: None,
                // };

                self.current_x = read.x as u16;
                self.current_y = read.y as u16;

                // // Convert to "i4"
                // let global_scale = ((global_scale << 4) as i8) >> 4;

                // if global_scale < 0 {
                //     // Division
                //     self.global_scale_shift_left = false;
                //     self.global_scale_shift_count = global_scale.abs() as u8;
                // } else {
                //     // Multiplication
                //     self.global_scale_shift_left = true;
                //     self.global_scale_shift_count = global_scale as u8;
                // }
            }
            // HALT
            0xB => {
                self.is_halted = true;
                return StepResult::Halt;
            }
            // JSRL: Jump to subroutine
            0xC => {
                self.stack[3] = self.stack[2];
                self.stack[2] = self.stack[1];
                self.stack[1] = self.stack[0];
                self.stack[0] = self.pc;

                self.pc = instruction_word & 0xFFF;
            }
            // RTSL: Return from subroutine
            0xD => {
                self.pc = self.stack[0];
                self.stack[0] = self.stack[1];
                self.stack[1] = self.stack[2];
                self.stack[2] = self.stack[3];
                self.stack[3] = 0xFFFF;
            }
            // JMPL: Jump
            0xE => {
                self.pc = instruction_word & 0xFFF;
            }
            // SVEC: Draw short vector
            0xF => {
                let x = instruction_word & 0x3;
                let x_sign = (instruction_word & 0x4) != 0;

                let scale_factor_high = (instruction_word >> 3) & 0x1;

                let intensity = (instruction_word >> 4) & 0xF;

                let y = (instruction_word >> 8) & 0x3;
                let y_sign = (instruction_word & 0x400) != 0;

                let scale_factor_low = (instruction_word >> 11) & 0x1;

                let scale = (scale_factor_high << 1) | scale_factor_low;

                // let x = x << (scale + 1);
                // let y = y << (scale + 1);
                let x = x << 8;
                let y = y << 8;

                let read = XYRead::new(x, x_sign, y, y_sign, intensity);

                return StepResult::Move(self.apply_delta_move(read, (scale + 2) as u8));
            }
            _ => unreachable!(),
        }

        StepResult::None
    }

    fn read_xy(&mut self, opcode_shift: Option<u16>, instruction_word: u16) -> XYRead {
        let second_word = self.read_next_word();

        let mut y = instruction_word & 0x3FF;
        let y_sign = (instruction_word & 0x400) != 0;

        let mut x = second_word & 0x3FF;
        let x_sign = (second_word & 0x400) != 0;

        // Intensity/scale factor
        let modifier = (second_word >> 12) & 0xF;

        // Scale directions based on opcode
        // if let Some(opcode) = opcode_shift {
        //     let shift_scale = 9 - opcode;

        //     y = y >> shift_scale;
        //     x = x >> shift_scale;
        // }

        XYRead::new(x, x_sign, y, y_sign, modifier)
    }

    pub fn read_next_word(&mut self) -> u16 {
        // TODO: This is assuming word addressing
        let pc = self.pc as usize;
        self.pc = self.pc.wrapping_add(1);

        self.read_word(pc << 1)
    }

    /// `address` is a byte address
    pub fn read_word(&self, address: usize) -> u16 {
        let bytes = match address {
            0x0..0x1000 => [self.ram[address], self.ram[address + 1]],
            // TODO: This is half the size
            0x1000..0x1800 => {
                let address = address - 0x1000;
                [self.rom[address], self.rom[address + 1]]
            }
            _ => {
                println!("Out of bounds DVG read at {address:08X}");
                [0, 0]
            }
        };

        u16::from_le_bytes(bytes)
    }

    /// `address` is a byte address
    pub fn write_byte(&mut self, address: usize, value: u8) {
        match address {
            0x0..0x1000 => self.ram[address] = value,
            _ => {
                println!("Invalid DVG write to {address:08X}");
            }
        }
    }

    /// Computes the global scale bitshift as bits shifted left or right
    fn apply_scale(&self, value: i16, local_scale: u8) -> i16 {
        let scale = self.global_scale.wrapping_add(local_scale) & 0xF;

        if scale >= 10 {
            return 0;
        }

        let magnitude = (value.unsigned_abs() >> (9 - scale)) as i16;

        if value > 0 { magnitude } else { -magnitude }

        // if self.global_scale_shift_left {
        //     value << self.global_scale_shift_count
        // } else {
        //     value >> self.global_scale_shift_count
        // }
    }

    fn apply_delta_move(&mut self, read: XYRead, local_scale: u8) -> VectorMove {
        let source_x = self.current_x;
        let source_y = self.current_y;

        let delta_x = self.apply_scale(read.x, local_scale);
        let delta_y = self.apply_scale(read.y, local_scale);

        let x = bounded_u16_i16_add(source_x, delta_x, MAX_SCREEN_SIZE);
        let y = bounded_u16_i16_add(source_y, delta_y, MAX_SCREEN_SIZE);

        self.current_x = x;
        self.current_y = y;

        println!(
            "dvg: pc={:03X} gs={} local={} d=({},{}) -> ({}, {})",
            self.pc, self.global_scale, local_scale, delta_x, delta_y, x, y
        );

        VectorMove {
            source_x,
            source_y,

            dest_x: x,
            dest_y: y,

            intensity: Some(read.modifier),
        }
    }
}

struct XYRead {
    x: i16,
    y: i16,

    modifier: u8,
}

impl XYRead {
    fn new(x: u16, x_sign: bool, y: u16, y_sign: bool, modifier: u16) -> Self {
        let x = get_signed_int(x, x_sign);
        let y = get_signed_int(y, y_sign);

        Self {
            x,
            y,
            modifier: (modifier as u8),
        }
    }
}

fn get_signed_int(base: u16, sign: bool) -> i16 {
    let sign = if sign { -1 } else { 1 };
    (base as i16) * sign
}

/// Bounds an addition to be within [0, bound)
fn bounded_u16_i16_add(unsigned: u16, signed: i16, bound: u16) -> u16 {
    let sum = (unsigned as i16) + signed;

    let sum = if sum < 0 {
        // // Wrap against bound
        // sum + (bound as i16)
        0
    } else {
        sum
    };

    sum as u16
}
