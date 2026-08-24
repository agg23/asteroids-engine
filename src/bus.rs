use crate::{
    dvg::{DVG, VectorMove},
    rom::ROM,
};

// 3 kHz
const CLOCK_PERIOD: u64 = 0x200;
const CLOCK_INTERVAL: u64 = CLOCK_PERIOD / 2;
// Watchdog counter D5 counts to 128 before triggering reset
const WATCHDOG_PERIOD: usize = (CLOCK_PERIOD as usize) * 0x80;

pub struct Bus {
    /// 3kHz clock
    cycle_count: u64,
    watchdog_cycle_count: usize,

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
            watchdog_cycle_count: 0,
            nmi: false,
            ram: [0; 0x400],
            rom: rom.program,
            dvg,
        }
    }

    /// Returns true if watchdog fired
    pub fn run_steps(&mut self, step_count: usize, moves: &mut Vec<VectorMove>) -> bool {
        self.nmi = false;

        for _ in 0..step_count {
            // TODO: Find cycle time
            moves.extend(self.dvg.step());
        }

        self.cycle_count += step_count as u64;
        self.watchdog_cycle_count += step_count;

        if self.watchdog_cycle_count >= WATCHDOG_PERIOD {
            // Don't need precise timing. Just completely reset it
            self.watchdog_cycle_count = 0;

            true
        } else {
            false
        }
    }

    pub fn request_nmi(&mut self) {
        self.nmi = true;
    }

    pub fn reset_dvg(&mut self) {
        self.dvg.reset();
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
                if (self.cycle_count & CLOCK_INTERVAL) != 0 {
                    0x80
                } else {
                    0x7F
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
                let word = self.dvg.read_word((address - 0x4000) as usize);

                // Since we're using a byte address, the low byte is always the one we want
                (word & 0xFF) as u8
            }
            0x6800..=0xFFFF => self.rom[(address - 0x6800) as usize],
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
                self.dvg.resume();
            }
            // Watchdog reset
            0x3400 => {
                self.watchdog_cycle_count = 0;
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
