use fast_srgb8::srgb8_to_f32;
#[cfg(target_os = "windows")]
use winit::{
	event::*,
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

use crate::{
	render::{CardInstance, Renderer},
	stroke::Stroke,
	wintab::*,
};

// Current state of our app.
pub struct App {
	window: winit::window::Window,
	pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
	should_redraw: bool,
	renderer: Renderer,
	cursor_x: f64,
	cursor_y: f64,
	is_cursor_relevant: bool,
	is_cursor_pressed: bool,
	tablet_context: Option<TabletContext>,
	pressure: Option<f64>,
	strokes: Vec<Stroke>,
	is_mid_stroke: bool,
}

impl App {
	// Sets up the logger and renderer.
	pub fn new(event_loop: &EventLoop<()>) -> Self {
		let window = WindowBuilder::new().with_title("Monoscribe").build(event_loop).unwrap();

		// Attempt to establish a tablet context.
		let tablet_context = TabletContext::new(&window);

		// Set up the renderer.
		let size = window.inner_size();
		let renderer = Renderer::new(&window, size.width, size.height);

		// Return a new instance of the app state.
		Self {
			window,
			pending_resize: None,
			should_redraw: false,
			renderer,
			cursor_x: 0.,
			cursor_y: 0.,
			is_cursor_relevant: false,
			is_cursor_pressed: false,
			tablet_context,
			pressure: None,
			strokes: Vec::new(),
			is_mid_stroke: false,
		}
	}

	// Runs the event loop with the event handler.
	pub fn run(mut self, event_loop: EventLoop<()>) {
		// Run the event loop.
		event_loop.run(move |event, _, control_flow| self.handle_event(event, control_flow));
	}

	// Handles a single event.
	fn handle_event(&mut self, event: Event<()>, control_flow: &mut ControlFlow) {
		match event {
			// Emitted when the event loop resumes.
			Event::NewEvents(_) => {
				self.should_redraw = false;
			},
			// Check if a window event has occurred.
			Event::WindowEvent { ref event, window_id } if window_id == self.window.id() => {
				// FIXME: This is a little wasteful. Blender, for example, only updates if something actually does change on-screen.
				self.should_redraw = true;
				match event {
					// If the titlebar close button is clicked  or the escape key is pressed, exit the loop.
					WindowEvent::CloseRequested
					| WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Escape),
							..
						},
						..
					} => *control_flow = ControlFlow::Exit,
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Z),
							..
						},
						..
					} => {
						self.strokes.pop();
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Delete),
							..
						},
						..
					} => self.strokes.clear(),

					// Experiment with capturing cursor movements (currently changing clear color.)
					WindowEvent::CursorMoved { position, .. } => {
						self.cursor_x = position.x;
						self.cursor_y = position.y;
					},
					WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
						self.is_cursor_pressed = *state == ElementState::Pressed;
					},
					WindowEvent::CursorEntered { .. } => {
						self.is_cursor_relevant = true;
						self.tablet_context.as_mut().map(|c| c.enable(true).unwrap());
					},
					WindowEvent::CursorLeft { .. } => {
						self.is_cursor_relevant = false;
						self.tablet_context.as_mut().map(|c| c.enable(false).unwrap());
					},

					// Resize the window if requested to.
					WindowEvent::Resized(physical_size) => {
						self.pending_resize = Some(*physical_size);
					},
					WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
						self.pending_resize = Some(**new_inner_size);
					},

					// Ignore all other window events.
					_ => {},
				}
			},

			// If all other main events have been cleared, poll for tablet events, then reqeust a window redraw.
			Event::MainEventsCleared => {
				self.poll_tablet();
				if self.should_redraw {
					self.window.request_redraw();
				}
			},

			// If a window redraw is requested, have the renderer update and render.
			Event::RedrawRequested(window_id) if window_id == self.window.id() => {
				self.update_renderer();
				/*let card_instances = vec![
					CardInstance {
						position: [10., 10.],
						dimensions: [202., 102.],
						color: [srgb8_to_f32(0x2a), srgb8_to_f32(0xda), srgb8_to_f32(0xfa), 1.],
						depth: 0.,
						radius: 5.,
					},
					CardInstance {
						position: [11.0, 11.0],
						dimensions: [200.0, 100.0],
						color: [srgb8_to_f32(0x1e), srgb8_to_f32(0x1e), srgb8_to_f32(0x1e), 1.],
						depth: 0.,
						radius: 4.,
					},
				];
				*/
				match self.renderer.render(&vec![], &self.strokes) {
					Ok(_) => {},
					Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.width, self.renderer.height),
					Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
					Err(e) => eprintln!("{:?}", e),
				}
			},

			// If all redraw events have been cleared, suspend until a new event arrives.
			Event::RedrawEventsCleared => {
				*control_flow = ControlFlow::Wait;
			},

			// Ignore all other events.
			_ => {},
		}
	}

	fn poll_tablet(&mut self) {
		if !self.is_cursor_pressed {
			self.pressure = None;
		}

		if let Some(buf) = self.tablet_context.as_mut().map(|c| c.get_packets(50)) {
			if let Some(packet) = buf.last() {
				self.pressure = Some(f64::from(packet.normal_pressure));
			}
		}
	}

	fn update_renderer(&mut self) {
		self.renderer.clear_color = if self.is_cursor_relevant {
			wgpu::Color {
				r: self.cursor_x / f64::from(self.renderer.width),
				g: self.cursor_y / f64::from(self.renderer.height),
				b: if self.is_cursor_pressed { self.pressure.map_or(1., |x| x / 32767.) } else { 0. },
				a: 1.0,
			}
		} else {
			wgpu::Color::BLACK
		};

		if self.is_cursor_pressed {
			if !self.is_mid_stroke {
				self.strokes.push(Stroke::new());
				self.is_mid_stroke = true;
			}

			if let Some(current_stroke) = self.strokes.last_mut() {
				current_stroke.add_point(self.cursor_x as f32, self.cursor_y as f32, self.pressure.map_or(1., |pressure| (pressure / 32767.) as f32))
			}
		} else {
			self.is_mid_stroke = false;
		}

		// Apply a resize if necessary; resizes are time-intensive.
		if let Some(size) = self.pending_resize.take() {
			self.renderer.resize(size.width, size.height);
		}

		self.renderer.update();
	}
}
