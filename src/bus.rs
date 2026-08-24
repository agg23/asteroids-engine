use crate::{dvg::DVG, rom::ROM};

const CLOCK_PERIOD: usize = 512;

pub struct Bus {
    /// 3kHz clock
    cycle_count: u64,

    nmi: bool,

    ram: [u8; 0x400],
    rom: [u8; 0x1800],

    dvg: DVG,
}

impl Bus {
    pub fn new(rom: ROM) -> Self {
        let dvg = DVG::new(rom.vector);

        Self {
            cycle_count: 0,
            nmi: false,
            ram: [0; 0x400],
            rom: rom.program,
            dvg,
        }
    }

    pub fn run_steps(&mut self, step_count: usize) {
        self.nmi = false;

        for _ in 0..step_count {
            // TODO: Find cycle time
            self.dvg.step();
        }

        self.cycle_count += step_count as u64;
    }

    pub fn request_nmi(&mut self) {
        self.nmi = true;
    }
}

impl mos6502::memory::Bus for Bus {
    fn get_byte(&mut self, address: u16) -> u8 {
        // Upper bit is ignored
        let address = address & 0x7FFF;

        match address {
            0..0x400 => self.ram[(address & 0x3FF) as usize],
            // Clock
            0x2001 => {
                if (self.cycle_count & 0x100) != 0 {
                    1
                } else {
                    0
                }
            }
            // DVG Halt
            0x2002 => {
                if self.dvg.is_halted {
                    0x7F
                } else {
                    0x80
                }
            }
            // TODO: Hyperspace button
            0x2003 => 0,
            // TODO: Fire button
            0x2004 => 0,
            // TODO: Diagnostic step button
            0x2005 => 0,
            // TODO: Slam button
            0x2006 => 0,
            // TODO: Self test button
            0x2007 => 0,
            // TODO: Left coin button
            0x2400 => 0,
            // TODO: Center coin button
            0x2401 => 0,
            // TODO: Right coin button
            0x2402 => 0,
            // TODO: P1 start button
            0x2403 => 0,
            // TODO: P2 start button
            0x2404 => 0,
            // TODO: Thrust button
            0x2405 => 0,
            // TODO: Rotate right switch
            0x2406 => 0,
            // TODO: Rotate left switch
            0x2407 => 0,
            0x4000..0x6000 => {
                let word = self.dvg.read_word(address as usize);

                if address & 0x1 != 0 {
                    ((word >> 8) & 0xFF) as u8
                } else {
                    (word & 0xFF) as u8
                }
            }
            // 0x4000..0x5000 => self.dvg.ram[(address - 0x4000) as usize],
            // 0x5000..0x5800 => self.dvg.rom[(address - 0x5000) as usize],
            0x6800..=0xFFFF => self.rom[((address - 0x6800) & 0x17FF) as usize],
            _ => {
                println!("Out of bounds read {address:04X}");
                0
            }
        }
    }

    fn set_byte(&mut self, address: u16, value: u8) {
        // Upper bit is ignored
        let address = address & 0x7FFF;

        match address {
            0x0..0x400 => self.ram[(address & 0x3FF) as usize] = value,
            // GODVG
            0x3000 => {
                self.dvg.is_halted = false;
            }
            0x4000..0x6000 => self.dvg.write_byte((address - 0x4000) as usize, value),
            _ => {
                println!("Out of bounds write {address:0X}");
            }
        }
    }

    fn nmi_pending(&mut self) -> bool {
        self.nmi
    }
}
