use std::{path::Path, sync::Arc, time::Instant};

use asteroids::{GpuRenderer, Machine, ROM, shader::EMULATION_OUTPUT_RESOLUTION};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{keys::KeyState, surface::WindowTarget};

mod keys;
mod surface;

struct App {
    machine: Machine,

    // Neither can exist until we have a window
    target: Option<WindowTarget>,
    renderer: Option<GpuRenderer>,

    window: Option<Arc<Window>>,

    keys: KeyState,

    pause: Option<Instant>,
    stepping: bool,
}

impl App {
    fn new(machine: Machine) -> Self {
        Self {
            machine,

            target: None,
            renderer: None,

            window: None,

            keys: KeyState::new(),

            pause: None,
            stepping: false,
        }
    }

    fn toggle_pause(&mut self) {
        match self.pause {
            Some(instant) => {
                self.pause = None;
                // Don't count wall time spent paused against the CPU clock
                self.machine.ignore_elapsed(instant.elapsed());
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

    fn run_until_frame(&mut self) {
        let Self {
            machine: emulator,
            target,
            renderer,
            keys,
            ..
        } = self;

        let (Some(target), Some(renderer)) = (target.as_mut(), renderer.as_mut()) else {
            return;
        };

        let frame = emulator.run_until_frame(keys.current_gamepad());

        let Some(surface_texture) = target.acquire() else {
            return;
        };
        let view = surface_texture.texture.create_view(&Default::default());

        renderer.render(
            &view,
            frame.commands,
            frame.frame_tick_count,
            target.hdr_headroom(),
        );

        target.present(surface_texture);

        self.stepping = false;
    }

    fn present_last(&mut self) {
        let Self {
            target, renderer, ..
        } = self;

        let (Some(target), Some(renderer)) = (target.as_mut(), renderer.as_mut()) else {
            return;
        };

        let Some(surface_texture) = target.acquire() else {
            return;
        };
        let view = surface_texture.texture.create_view(&Default::default());

        renderer.present_last(&view);

        target.present(surface_texture);
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

        let target = WindowTarget::new(Arc::clone(&window));

        self.renderer = Some(GpuRenderer::new(
            target.device(),
            target.queue(),
            target.format(),
        ));
        self.target = Some(target);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                // Resized events can come regardless of whether or not we allow the window to resize, so do something in that scenario
                if let Some(target) = self.target.as_mut() {
                    target.resize(size.width, size.height);
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
            WindowEvent::RedrawRequested => self.present_last(),
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

    let emulator = Machine::new(rom);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(emulator);
    event_loop.run_app(&mut app).unwrap();
}
