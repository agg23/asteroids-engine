use std::path::Path;

use mos6502::{cpu::CPU, instruction::Nmos6502};

use crate::{bus::Bus, dvg::DVG, rom::ROM};

mod bus;
mod dvg;
mod rom;

// 1_512_000 / 246.09
const NMI_PERIOD: usize = 6144;

struct Machine {
    cpu: CPU<Bus, Nmos6502>,

    nmi: bool,
}

impl Machine {
    fn new(rom: ROM) -> Self {
        let bus = Bus::new(rom);

        Self {
            cpu: CPU::new(bus, Nmos6502),
            nmi: false,
        }
    }

    fn cpu_step(&mut self) -> usize {
        let current_cycles = self.cpu.cycles;
        self.cpu.single_step();
        self.nmi = false;

        (self.cpu.cycles - current_cycles) as usize
    }

    fn run_steps(&mut self, step_count: usize) {
        self.cpu.memory.run_steps(step_count);
    }

    fn request_nmi(&mut self) {
        self.cpu.memory.request_nmi();
    }
}

fn main() {
    let rom = ROM::load_mame(Path::new("/Users/adam/code/mame/roms/asteroid.zip"));

    println!("Loaded ROM");

    let mut machine = Machine::new(rom);

    let mut nmi_counter = 0;

    loop {
        let new_cycles = machine.cpu_step();

        machine.run_steps(new_cycles);

        nmi_counter += new_cycles;

        if nmi_counter >= NMI_PERIOD {
            // Request NMI to be picked up by next CPU step. It will be cleared on next machine step
            machine.request_nmi();

            nmi_counter -= NMI_PERIOD;
        }
    }
}
