use std::path::Path;

use minifb::{Window, WindowOptions};
use mos6502::{
    cpu::{CPU, WaitState},
    instruction::Nmos6502,
};

use crate::{bus::Bus, dvg::VectorMove, rom::ROM};

mod bus;
mod dvg;
mod rom;

// 1_512_000 / 246.09
const NMI_PERIOD: usize = 6144;

struct Machine {
    cpu: CPU<Bus, Nmos6502>,

    moves: Vec<VectorMove>,
}

impl Machine {
    fn new(rom: ROM) -> Self {
        let bus = Bus::new(rom);
        let mut cpu = CPU::new(bus, Nmos6502);
        cpu.reset();

        Self {
            cpu,
            moves: Vec::with_capacity(10),
        }
    }

    fn cpu_step(&mut self) -> usize {
        let current_cycles = self.cpu.cycles;
        self.cpu.single_step();

        (self.cpu.cycles - current_cycles) as usize
    }

    fn run_steps(&mut self, step_count: usize) {
        if self.cpu.memory.run_steps(step_count, &mut self.moves) {
            // Watchdog fired
            println!("Firing watchdog");
            self.cpu.reset();
            self.cpu.memory.reset_dvg();
        }
    }

    fn request_nmi(&mut self) {
        self.cpu.memory.request_nmi();
    }

    fn log(&self) {
        println!("CPU: pc={:04X}", self.cpu.registers.program_counter);
    }
}

fn main() {
    let rom = ROM::load_mame(Path::new("/Users/adam/code/mame/roms/asteroid.zip"));

    println!("Loaded ROM");

    let mut machine = Machine::new(rom);

    let mut nmi_counter = 0;
    let mut nmi_count = 0;

    const SIZE: usize = 1024;
    let mut window = Window::new("Asteroids", SIZE, SIZE, WindowOptions::default()).unwrap();
    let mut buffer = vec![0u32; SIZE * SIZE];

    while window.is_open() {
        let new_cycles = machine.cpu_step();

        machine.run_steps(new_cycles);

        nmi_counter += new_cycles;

        if machine.cpu.wait_state() == WaitState::WaitingForReset {
            panic!(
                "CPU jammed at {:04X}",
                machine.cpu.registers.program_counter
            );
        }

        if machine.cpu.cycles % 50_000 == 0 {
            machine.log();
        }

        if nmi_counter >= NMI_PERIOD {
            // Request NMI to be picked up by next CPU step. It will be cleared on next machine step
            machine.request_nmi();

            nmi_counter -= NMI_PERIOD;
            nmi_count += 1;

            // Render every 4th NMI (~60Hz)
            if nmi_count % 4 == 0 {
                buffer.fill(0);
                for m in machine.moves.drain(..) {
                    draw_line(&mut buffer, SIZE, &m);
                }
                window.update_with_buffer(&buffer, SIZE, SIZE).unwrap();
            }
        }
    }
}

fn draw_line(buffer: &mut [u32], size: usize, m: &dvg::VectorMove) {
    if m.intensity.unwrap_or(0) == 0 {
        return;
    }

    let (x0, y0) = (m.source_x as f32, m.source_y as f32);
    let (x1, y1) = (m.dest_x as f32, m.dest_y as f32);
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1.0) as usize;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = ((x0 + (x1 - x0) * t) as usize / 2).min(size - 1);
        let y = ((y0 + (y1 - y0) * t) as usize / 2).min(size - 1);

        buffer[(size - 1 - y) * size + x] = 0xFFFFFF;
    }
}
