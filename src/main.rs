use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

use minifb::{Key, KeyRepeat, Window, WindowOptions};
use mos6502::{
    cpu::{CPU, WaitState},
    instruction::Nmos6502,
};

use crate::{
    bus::Bus, gpu::GpuRenderer, input::GamepadInputs, rom::ROM, shader::DISPLAY_RESOLUTION,
    types::BeamStep,
};

mod bus;
mod dvg;
mod dvg_simple;
mod gpu;
mod input;
mod rom;
mod shader;
mod types;

const CLOCK_SPEED: usize = 1_512_000;

// 1_512_000 / 246.09
const NMI_PERIOD: usize = 6144;

struct Machine {
    cpu: CPU<Bus, Nmos6502>,

    commands: Vec<BeamStep>,
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

    let mut window = Window::new(
        "Asteroids",
        DISPLAY_RESOLUTION,
        DISPLAY_RESOLUTION,
        WindowOptions::default(),
    )
    .unwrap();
    let mut buffer = vec![0u32; DISPLAY_RESOLUTION * DISPLAY_RESOLUTION];
    let mut renderer = GpuRenderer::new(DISPLAY_RESOLUTION as u32);

    let mut start_instant = Instant::now();

    let mut inputs = GamepadInputs::new();

    let mut prev_p_pressed = false;
    let mut pause: Option<Instant> = None;

    let mut prev_right_pressed = false;
    let mut stepping = false;

    while window.is_open() {
        let p_pressed = window.is_key_down(Key::P);
        let right_pressed = window.is_key_down(Key::Right);

        if !prev_p_pressed && p_pressed {
            match pause {
                Some(instant) => {
                    pause = None;
                    start_instant += Instant::now().duration_since(instant);
                }
                None => pause = Some(Instant::now()),
            }
        }

        if !prev_right_pressed && right_pressed {
            stepping = true;
            if pause.is_none() {
                pause = Some(Instant::now());
            }
        }

        prev_right_pressed = right_pressed;
        prev_p_pressed = p_pressed;

        if !stepping && pause.is_some() {
            sleep(Duration::from_millis(16));
            window.update();
            continue;
        }

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
                renderer.render(machine.commands.drain(..), &mut buffer);
                window
                    .update_with_buffer(&buffer, DISPLAY_RESOLUTION, DISPLAY_RESOLUTION)
                    .unwrap();

                if stepping {
                    stepping = false;
                }
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
