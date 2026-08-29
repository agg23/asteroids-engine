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

use crate::{bus::Bus, input::GamepadInputs, rom::ROM, types::DrawCommand};

mod bus;
mod dvg;
mod dvg_simple;
mod input;
mod rom;
mod types;

const CLOCK_SPEED: usize = 1_512_000;

// 1_512_000 / 246.09
const NMI_PERIOD: usize = 6144;

struct Machine {
    cpu: CPU<Bus, Nmos6502>,

    commands: Vec<DrawCommand>,
}

impl Machine {
    fn new(rom: ROM) -> Self {
        let bus = Bus::new(rom);
        let mut cpu = CPU::new(bus, Nmos6502);
        cpu.reset();

        Self {
            cpu,
            commands: Vec::with_capacity(10),
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
            .run_steps(step_count, inputs, &mut self.commands)
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
                for vector_move in machine.commands.drain(..) {
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

fn draw_line(buffer: &mut [u32], size: usize, command: &DrawCommand) {
    if command.intensity == 0 {
        return;
    }

    let (x0, y0) = (command.start_x as f32, command.start_y as f32);

    let dx = wrapped_signed_delta(command.start_x, command.dest_x);
    let dy = wrapped_signed_delta(command.start_y, command.dest_y);

    let steps = dx.abs().max(dy.abs()).max(1.0) as usize;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (x0 + dx * t).rem_euclid(4096.0) as usize;
        let y = (y0 + dy * t).rem_euclid(4096.0) as usize;

        if (x | y) & 0x400 != 0 {
            // Traveled outside of display bounds. Ignore
            continue;
        }

        buffer[(size - 1 - y / 2) * size + x / 2] = 0xFFFFFF;
    }
}

fn wrapped_signed_delta(from: u16, to: u16) -> f32 {
    let forward = (to.wrapping_sub(from) & 0xFFF) as f32;

    if forward <= 2048.0 {
        forward
    } else {
        forward - 4096.0
    }
}
