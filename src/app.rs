use std::time::{Duration, Instant};

use fast_srgb8::srgb8_to_f32;
use vek::Vec2;
use winit::dpi::{PhysicalPosition, PhysicalSize};
#[cfg(target_os = "windows")]
use winit::{
	event::*,
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

use crate::{
	render::{CardInstance, Renderer},
	stroke::{Canvas, Stroke},
	wintab::*,
};

struct PanOrigin {
	cursor: Vec2<f64>,
	position: Vec2<f32>,
}

enum Mode {
	Drawing { current_stroke: Option<Stroke> },
	Selecting { origin: Option<Vec2<f32>> },
	Panning { origin: Option<PanOrigin> },
	Moving { origin: Option<Vec2<f32>> },
}

struct ModeStack {
	base_mode: Mode,
	transient_mode: Option<Mode>,
}

impl ModeStack {
	pub fn new(mode: Mode) -> Self {
		Self { base_mode: mode, transient_mode: None }
	}

	pub fn get(&self) -> &Mode {
		self.transient_mode.as_ref().unwrap_or(&self.base_mode)
	}

	pub fn get_mut(&mut self) -> &mut Mode {
		self.transient_mode.as_mut().unwrap_or(&mut self.base_mode)
	}

	pub fn temp_pan(&mut self, should_pan: bool) {
		if should_pan {
			if !matches!(self.transient_mode, Some(Mode::Panning { .. })) {
				self.transient_mode = Some(Mode::Panning { origin: None });
			}
		} else {
			self.transient_mode = None;
		}
	}

	pub fn switch_select(&mut self) {
		if !matches!(self.base_mode, Mode::Selecting { .. }) {
			self.base_mode = Mode::Selecting { origin: None }
		}
	}

	pub fn switch_draw(&mut self) {
		if !matches!(self.base_mode, Mode::Drawing { .. }) {
			self.base_mode = Mode::Drawing { current_stroke: None }
		}
	}

	pub fn switch_move(&mut self) {
		if !matches!(self.base_mode, Mode::Moving { .. }) {
			self.base_mode = Mode::Moving { origin: None }
		}
	}

	pub fn discard_draft(&mut self) {
		match self.get_mut() {
			Mode::Drawing { current_stroke } => *current_stroke = None,
			Mode::Selecting { origin } => *origin = None,
			Mode::Panning { .. } => {},
			Mode::Moving { origin } => *origin = None,
		}
	}

	pub fn current_stroke(&self) -> Option<&Stroke> {
		if let Mode::Drawing { current_stroke } = &self.base_mode {
			current_stroke.as_ref()
		} else {
			None
		}
	}
}

// Current state of our app.
pub struct App {
	window: winit::window::Window,
	pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
	should_redraw: bool,
	renderer: Renderer,
	cursor_x: f64,
	cursor_y: f64,
	position: [f32; 2],
	is_cursor_relevant: bool,
	is_cursor_pressed: bool,
	was_cursor_changed_this_frame: bool,
	is_shift_pressed: bool,
	tablet_context: Option<TabletContext>,
	pressure: Option<f64>,
	canvas: Canvas,
	mode_stack: ModeStack,
	last_frame_instant: std::time::Instant,
}

impl App {
	// Sets up the logger and renderer.
	pub fn new(event_loop: &EventLoop<()>) -> Self {
		let window = WindowBuilder::new().with_title("Inskriva").with_visible(false).build(event_loop).unwrap();

		// Resize the window to a reasonable size.
		let monitor_size = window.current_monitor().unwrap().size();
		window.set_inner_size(PhysicalSize::new(monitor_size.width as f64 / 1.5, monitor_size.height as f64 / 1.5));
		window.set_outer_position(PhysicalPosition::new(monitor_size.width as f64 / 6., monitor_size.height as f64 / 6.));

		// Attempt to establish a tablet context.
		let tablet_context = TabletContext::new(&window);

		// Set up the renderer.
		let position = [0.; 2];
		let size = window.inner_size();
		let mut renderer = Renderer::new(&window, position, size.width, size.height);

		// Make the window visible and immediately clear color to prevent a flash.
		renderer.clear_color = wgpu::Color {
			r: srgb8_to_f32(0x04) as f64,
			g: srgb8_to_f32(0x0f) as f64,
			b: srgb8_to_f32(0x16) as f64,
			a: srgb8_to_f32(0xff) as f64,
		};
		let output = renderer.clear().unwrap();
		window.set_visible(true);
		// FIXME: This sometimes flashes, and sometimes doesn't.
		output.present();

		// Return a new instance of the app state.
		Self {
			window,
			pending_resize: None,
			should_redraw: false,
			renderer,
			cursor_x: 0.,
			cursor_y: 0.,
			position,
			is_cursor_relevant: false,
			is_cursor_pressed: false,
			was_cursor_changed_this_frame: false,
			is_shift_pressed: false,
			tablet_context,
			pressure: None,
			canvas: Canvas::new(),
			mode_stack: ModeStack::new(Mode::Drawing { current_stroke: None }),
			last_frame_instant: Instant::now() - Duration::new(1, 0),
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
				// FIXME: This is a little wasteful. Is it feasible to only update if something actually does change on-screen?
				self.should_redraw = true;
				match event {
					// If the titlebar close button is clicked  or the escape key is pressed, exit the loop.
					WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Escape),
							..
						},
						..
					} => {
						self.mode_stack.discard_draft();
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Q),
							..
						},
						..
					} => {
						for stroke in self.canvas.strokes.iter_mut().filter(|x| x.is_selected) {
							stroke.is_selected = false;
						}
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::Z),
							..
						},
						..
					} => {
						if !self.is_cursor_pressed {
							self.canvas.strokes.pop();
						}
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::S),
							..
						},
						..
					} => {
						self.mode_stack.switch_select();
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::B),
							..
						},
						..
					} => {
						self.mode_stack.switch_draw();
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::T),
							..
						},
						..
					} => {
						self.mode_stack.switch_move();
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state,
							virtual_keycode: Some(VirtualKeyCode::Space),
							..
						},
						..
					} => match state {
						ElementState::Pressed => {
							if self.mode_stack.transient_mode.is_none() {
								self.mode_stack.temp_pan(true);
							}
						},
						ElementState::Released => {
							self.mode_stack.temp_pan(false);
						},
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state,
							virtual_keycode: Some(VirtualKeyCode::LShift),
							..
						},
						..
					} => match state {
						ElementState::Pressed => {
							self.is_shift_pressed = true;
						},
						ElementState::Released => {
							self.is_shift_pressed = false;
						},
					},
					WindowEvent::KeyboardInput {
						input: KeyboardInput {
							state: ElementState::Pressed,
							virtual_keycode: Some(VirtualKeyCode::X),
							..
						},
						..
					} => for _ in self.canvas.strokes.drain_filter(|x| x.is_selected) {},

					WindowEvent::CursorMoved { position, .. } => {
						self.cursor_x = position.x;
						self.cursor_y = position.y;
					},
					WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
						self.was_cursor_changed_this_frame = self.is_cursor_pressed != (*state == ElementState::Pressed);
						self.is_cursor_pressed = self.was_cursor_changed_this_frame ^ self.is_cursor_pressed;
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

				// Only render if it's been too long since the last render.
				if (Instant::now() - self.last_frame_instant) >= Duration::new(1, 0) / 90 {
					self.last_frame_instant = Instant::now();

					match self.repaint() {
						Ok(_) => {},
						Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.width, self.renderer.height),
						Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
						Err(e) => eprintln!("{:?}", e),
					}
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

	fn repaint(&mut self) -> Result<(), wgpu::SurfaceError> {
		// FIXME: Check if selecting in transient mode first.
		let card_instances = if let Mode::Selecting { origin: Some(origin) } = &self.mode_stack.base_mode {
			let current = Vec2::<f32>::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>();
			vec![CardInstance {
				position: [current.x.min(origin.x), current.y.min(origin.y)],
				dimensions: [(current.x - origin.x).abs(), (current.y - origin.y).abs()],
				color: [0x22, 0xae, 0xd1, 0x33].map(srgb8_to_f32),
				depth: 0.,
				radius: 0.,
			}]
		} else {
			vec![]
		};

		let selection_offset = if let Mode::Moving { origin: Some(origin) } = &self.mode_stack.base_mode {
			Some(Vec2::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>() - *origin)
		} else {
			None
		};

		let (strokes_vertices, strokes_indices) = self.canvas.bake(self.mode_stack.current_stroke(), selection_offset);

		self.renderer.render(&card_instances, strokes_vertices, strokes_indices)
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
		match self.mode_stack.get_mut() {
			Mode::Drawing { current_stroke } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Arrow);
				if self.is_cursor_pressed {
					if self.was_cursor_changed_this_frame && current_stroke.is_none() {
						*current_stroke = Some(Stroke::new());
					}

					if let Some(current_stroke) = current_stroke {
						current_stroke.add_point(self.position[0] + self.cursor_x as f32, self.position[1] + self.cursor_y as f32, self.pressure.map_or(1., |pressure| (pressure / 32767.) as f32))
					}
				} else {
					self.canvas.strokes.extend(current_stroke.take());
				}
			},
			Mode::Selecting { origin } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);

				if self.is_cursor_pressed {
					if self.was_cursor_changed_this_frame && origin.is_none() {
						*origin = Some(Vec2::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>());
					}
				} else {
					if let Some(origin) = origin.take() {
						let current = Vec2::<f32>::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>();
						let min = Vec2::new(current.x.min(origin.x), current.y.min(origin.y));
						let max = Vec2::new(current.x.max(origin.x), current.y.max(origin.y));
						self.canvas.select(min, max, self.is_shift_pressed);
					}
				}
			},
			Mode::Panning { origin } => {
				if self.is_cursor_pressed {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					// TODO: We ignore was_cursor_changed_this_frame; figure out a consistent, intuitive standard across modes.
					if origin.is_none() {
						*origin = Some(PanOrigin {
							cursor: Vec2::new(self.cursor_x, self.cursor_y),
							position: Vec2::from(self.position),
						});
					}
				} else {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					if origin.is_some() {
						*origin = None;
					}
				}

				if let Some(origin) = origin {
					self.position = (origin.position - (Vec2::new(self.cursor_x, self.cursor_y) - origin.cursor).as_::<f32>()).into_array();
					self.renderer.reposition(self.position);
				}
			},
			Mode::Moving { origin } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Move);

				if self.is_cursor_pressed {
					if self.was_cursor_changed_this_frame && origin.is_none() {
						*origin = Some(Vec2::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>());
					}
				} else {
					if let Some(origin) = origin.take() {
						let selection_offset = Vec2::<f32>::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>() - origin;

						for stroke in self.canvas.strokes.iter_mut().filter(|x| x.is_selected) {
							stroke.origin = stroke.origin + selection_offset;
						}
					}
				}
			},
		}

		// Apply a resize if necessary; resizes are time-intensive.
		if let Some(size) = self.pending_resize.take() {
			self.renderer.resize(size.width, size.height);
		}

		self.renderer.update();

		// Reset inputs.
		self.was_cursor_changed_this_frame = false;
	}
}
