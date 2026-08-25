use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

use minifb::{Window, WindowOptions};
use mos6502::{
    cpu::{CPU, WaitState},
    instruction::Nmos6502,
};

use crate::{bus::Bus, dvg::VectorMove, input::GamepadInputs, rom::ROM};

mod bus;
mod dvg;
mod input;
mod rom;

const CLOCK_SPEED: usize = 1_512_000;

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

    fn run_steps(&mut self, step_count: usize, inputs: GamepadInputs) {
        if self
            .cpu
            .memory
            .run_steps(step_count, inputs, &mut self.moves)
        {
            // Watchdog fired
            println!("Firing watchdog");
            self.cpu.reset();
            self.cpu.memory.reset_dvg();
        }
    }

    fn request_nmi(&mut self) {
        self.cpu.memory.request_nmi();
    }

    #[allow(dead_code)]
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

    const SIZE: usize = 512;
    let mut window = Window::new("Asteroids", SIZE, SIZE, WindowOptions::default()).unwrap();
    let mut buffer = vec![0u32; SIZE * SIZE];

    let start_instant = Instant::now();

    let mut inputs = GamepadInputs::new();

    while window.is_open() {
        let new_cycles = machine.cpu_step();
        machine.run_steps(new_cycles, inputs.clone());

        nmi_counter += new_cycles;

        if nmi_counter >= NMI_PERIOD {
            // Request NMI to be picked up by next CPU step. It will be cleared on next machine step
            machine.request_nmi();

            nmi_counter -= NMI_PERIOD;
            nmi_count += 1;

            // Render every 4th NMI (~60Hz)
            if nmi_count % 4 == 0 {
                buffer.fill(0);
                for vector_move in machine.moves.drain(..) {
                    draw_line(&mut buffer, SIZE, &vector_move);
                }
                window.update_with_buffer(&buffer, SIZE, SIZE).unwrap();
            }

            inputs = GamepadInputs::read_keyboard(&window);

            // Get the timestamp of the CPU, and wait until real time caches up
            let current_cpu_time = Duration::from_nanos(
                (machine.cpu.cycles as u128 * 1_000_000_000 / CLOCK_SPEED as u128) as u64,
            );
            let target_time = start_instant + current_cpu_time;

            if let Some(delay) = target_time.checked_duration_since(Instant::now()) {
                sleep(delay);
            }
        }

        if machine.cpu.wait_state() == WaitState::WaitingForReset {
            panic!(
                "CPU jammed at {:04X}",
                machine.cpu.registers.program_counter
            );
        }
    }
}

fn draw_line(buffer: &mut [u32], size: usize, vector_move: &VectorMove) {
    if vector_move.intensity.unwrap_or(0) == 0 {
        return;
    }

    let (x0, y0) = (vector_move.source_x as f32, vector_move.source_y as f32);
    let (x1, y1) = (vector_move.dest_x as f32, vector_move.dest_y as f32);
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1.0) as usize;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = ((x0 + (x1 - x0) * t) as usize / 2).min(size - 1);
        let y = ((y0 + (y1 - y0) * t) as usize / 2).min(size - 1);

        buffer[(size - 1 - y) * size + x] = 0xFFFFFF;
    }
}
