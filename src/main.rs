use std::{
    path::Path,
    sync::Arc,
    thread::sleep,
    time::{Duration, Instant},
};

use mos6502::{
    cpu::{CPU, WaitState},
    instruction::Nmos6502,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    bus::Bus,
    gpu::GpuRenderer,
    input::{GamepadInputs, KeyState},
    rom::ROM,
    shader::EMULATION_OUTPUT_RESOLUTION,
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
const NMI_PERIOD_TICKS: usize = 6144;

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

struct App {
    machine: Machine,
    // Owns the surface, so it can't exist until we have a window
    renderer: Option<GpuRenderer>,

    window: Option<Arc<Window>>,

    keys: KeyState,
    inputs: GamepadInputs,

    nmi_counter: usize,
    nmi_count: usize,
    last_frame_ticks: u64,

    start_instant: Instant,
    pause: Option<Instant>,
    stepping: bool,
}

impl App {
    fn new(machine: Machine) -> Self {
        Self {
            machine,
            renderer: None,

            window: None,

            keys: KeyState::new(),
            inputs: GamepadInputs::new(),

            nmi_counter: 0,
            nmi_count: 0,
            last_frame_ticks: 0,

            start_instant: Instant::now(),
            pause: None,
            stepping: false,
        }
    }

    fn toggle_pause(&mut self) {
        match self.pause {
            Some(instant) => {
                self.pause = None;
                // Don't count wall time spent paused against the CPU clock
                self.start_instant += Instant::now().duration_since(instant);
            }
            None => self.pause = Some(Instant::now()),
        }
    }

    fn step_frame(&mut self) {
        self.stepping = true;
        if self.pause.is_none() {
            self.pause = Some(Instant::now());
        }
    }

    fn draw(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        renderer.render(self.machine.commands.drain(..), self.last_frame_ticks);

        self.last_frame_ticks = self.machine.cpu.cycles;
    }

    fn run_until_frame(&mut self) {
        loop {
            let new_cycles = self.machine.cpu_step();
            self.machine.run_steps(new_cycles, self.inputs.clone());

            if self.machine.cpu.wait_state() == WaitState::WaitingForReset {
                panic!(
                    "CPU jammed at {:04X}",
                    self.machine.cpu.registers.program_counter
                );
            }

            self.nmi_counter += new_cycles;

            if self.nmi_counter < NMI_PERIOD_TICKS {
                continue;
            }

            // Request NMI to be picked up by next CPU step. It will be cleared on next machine step
            self.machine.request_nmi();

            self.nmi_counter -= NMI_PERIOD_TICKS;
            self.nmi_count += 1;

            // Render every 4th NMI (~60Hz)
            let did_render = self.nmi_count % 4 == 0;

            if did_render {
                self.draw();
                self.stepping = false;
            }

            self.inputs = self.keys.current_gamepad();

            // Get the timestamp of the CPU, and wait until real time catches up
            let current_cpu_time = Duration::from_nanos(
                (self.machine.cpu.cycles as u128 * 1_000_000_000 / CLOCK_SPEED as u128) as u64,
            );
            let target_time = self.start_instant + current_cpu_time;

            if let Some(delay) = target_time.checked_duration_since(Instant::now()) {
                sleep(delay);
            }

            if did_render {
                return;
            }
        }
    }
}

impl ApplicationHandler for App {
    // Don't create any graphics contexts/windows until we receive our first resumed
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Should respect DPI/pixel scale
        let size = PhysicalSize::new(
            EMULATION_OUTPUT_RESOLUTION as u32,
            EMULATION_OUTPUT_RESOLUTION as u32,
        );

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Asteroids")
                        .with_inner_size(size)
                        .with_resizable(false),
                )
                .unwrap(),
        );

        self.renderer = Some(GpuRenderer::new(Arc::clone(&window)));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                // Resized events can come regardless of whether or not we allow the window to resize, so do something in that scenario
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };

                let pressed = event.state == ElementState::Pressed;
                self.keys.set(code, pressed);

                // winit gives us real transitions, so no manual edge detection
                if pressed && !event.repeat {
                    match code {
                        KeyCode::KeyP => self.toggle_pause(),
                        KeyCode::ArrowRight => self.step_frame(),
                        KeyCode::Escape => event_loop.exit(),
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.present_last();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.pause.is_some() && !self.stepping {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        event_loop.set_control_flow(ControlFlow::Poll);
        self.run_until_frame();
    }
}

fn main() {
    let rom = ROM::load_mame(Path::new("/Users/adam/code/mame/roms/asteroid.zip"));

    println!("Loaded ROM");

    let machine = Machine::new(rom);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(machine);
    event_loop.run_app(&mut app).unwrap();
}
