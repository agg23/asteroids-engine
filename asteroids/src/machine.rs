use std::{
    thread::sleep,
    time::{Duration, Instant},
    vec::Drain,
};

use mos6502::{
    cpu::{CPU, WaitState},
    instruction::Nmos6502,
};

use crate::{bus::Bus, input::GamepadInputs, rom::ROM, types::BeamStep};

pub const CLOCK_SPEED: usize = 1_512_000;

// 1_512_000 / 246.09
pub const NMI_PERIOD_TICKS: usize = 6144;

/// The display is driven every fourth NMI (~60Hz)
const NMIS_PER_FRAME: usize = 4;

pub struct Frame<'a> {
    pub commands: Drain<'a, BeamStep>,
    pub frame_tick_count: u64,
}

pub struct Machine {
    cpu: CPU<Bus, Nmos6502>,

    commands: Vec<BeamStep>,

    nmi_counter: usize,
    nmi_count: usize,
    last_frame_ticks: u64,

    start_instant: Instant,
    /// Wall time that has elapsed, but should be ignored by the emulation, updating internal counters
    pending_catch_up: Duration,
}

impl Machine {
    pub fn new(rom: ROM) -> Self {
        let bus = Bus::new(rom);
        let mut cpu = CPU::new(bus, Nmos6502);
        cpu.reset();

        Self {
            cpu,
            commands: Vec::with_capacity(10),

            nmi_counter: 0,
            nmi_count: 0,
            last_frame_ticks: 0,

            start_instant: Instant::now(),
            pending_catch_up: Duration::ZERO,
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

    /// Discard wall time that passed while the emulator wasn't running, so it doesn't fast-forward the CPU to catch up
    pub fn ignore_elapsed(&mut self, duration: Duration) {
        self.pending_catch_up += duration;
    }

    /// Run the machine until the next display frame is complete, based on wall timing
    pub fn run_until_frame(&mut self, inputs: GamepadInputs) -> Frame<'_> {
        self.step_until_frame(inputs, true)
    }

    /// Run the next display frame as quickly as possible, not sleeping for frame/CPU timing
    pub fn run_until_frame_now(&mut self, inputs: GamepadInputs) -> Frame<'_> {
        self.step_until_frame(inputs, false)
    }

    fn step_until_frame(&mut self, inputs: GamepadInputs, paced: bool) -> Frame<'_> {
        if paced {
            self.sleep_until_cpu_time();
        }

        loop {
            let new_cycles = self.cpu_step();
            self.run_steps(new_cycles, inputs.clone());

            if self.cpu.wait_state() == WaitState::WaitingForReset {
                panic!("CPU jammed at {:04X}", self.cpu.registers.program_counter);
            }

            self.nmi_counter += new_cycles;

            if self.nmi_counter < NMI_PERIOD_TICKS {
                continue;
            }

            // Request NMI to be picked up by next CPU step. It will be cleared on next machine step
            self.request_nmi();

            self.nmi_counter -= NMI_PERIOD_TICKS;
            self.nmi_count += 1;

            let did_render = self.nmi_count % NMIS_PER_FRAME == 0;

            if did_render {
                let frame_tick_count = self.last_frame_ticks;
                self.last_frame_ticks = self.cpu.cycles;

                return Frame {
                    commands: self.commands.drain(..),
                    frame_tick_count,
                };
            }

            if paced {
                self.sleep_until_cpu_time();
            }
        }
    }

    fn sleep_until_cpu_time(&mut self) {
        self.start_instant += std::mem::take(&mut self.pending_catch_up);

        let current_cpu_time = Duration::from_nanos(
            (self.cpu.cycles as u128 * 1_000_000_000 / CLOCK_SPEED as u128) as u64,
        );
        let target_time = self.start_instant + current_cpu_time;

        if let Some(delay) = target_time.checked_duration_since(Instant::now()) {
            sleep(delay);
        }
    }
}
