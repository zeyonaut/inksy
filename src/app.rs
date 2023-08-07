use std::time::{Duration, Instant};

use fast_srgb8::srgb8_to_f32;
use vek::{Vec2, Vec3};
use winit::dpi::{PhysicalPosition, PhysicalSize};
#[cfg(target_os = "windows")]
use winit::{
	event::*,
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

use crate::{
	input::{Input, InputMonitor},
	render::{CardInstance, ColorWheelInstance, Renderer},
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
	ChoosingColor { cursor_origin: Vec2<f32>, is_selecting_hue: bool },
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

	pub fn temp_choose(&mut self, cursor_origin: Option<Vec2<f32>>) {
		if let Some(cursor_origin) = cursor_origin {
			if !matches!(self.transient_mode, Some(Mode::ChoosingColor { .. })) {
				self.transient_mode = Some(Mode::ChoosingColor { cursor_origin, is_selecting_hue: false });
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

	pub fn is_drafting(&mut self) -> bool {
		match self.get_mut() {
			Mode::Drawing { current_stroke } => current_stroke.is_some(),
			Mode::Selecting { origin } => origin.is_some(),
			Mode::Moving { origin } => origin.is_some(),
			_ => false,
		}
	}

	pub fn discard_draft(&mut self) {
		match self.get_mut() {
			Mode::Drawing { current_stroke } => *current_stroke = None,
			Mode::Selecting { origin } => *origin = None,
			Mode::Moving { origin } => *origin = None,
			_ => {},
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
	tablet_context: Option<TabletContext>,
	pressure: Option<f64>,
	canvas: Canvas,
	mode_stack: ModeStack,
	last_frame_instant: std::time::Instant,
	input_monitor: InputMonitor,
	current_color: [u8; 4],
}

impl App {
	// Sets up the logger and renderer.
	pub fn new(event_loop: &EventLoop<()>) -> Self {
		let window = WindowBuilder::new().with_title("Inkslate").with_visible(false).build(event_loop).unwrap();

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
			r: srgb8_to_f32(0x12) as f64,
			g: srgb8_to_f32(0x12) as f64,
			b: srgb8_to_f32(0x12) as f64,
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
			tablet_context,
			pressure: None,
			canvas: Canvas::new(),
			mode_stack: ModeStack::new(Mode::Drawing { current_stroke: None }),
			last_frame_instant: Instant::now() - Duration::new(1, 0),
			input_monitor: InputMonitor::new(),
			current_color: [0xfb, 0xfb, 0xff, 0xff],
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
			Event::NewEvents(_) => {},
			// Check if a window event has occurred.
			Event::WindowEvent { ref event, window_id } if window_id == self.window.id() => {
				match event {
					// If the titlebar close button is clicked  or the escape key is pressed, exit the loop.
					WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
					WindowEvent::KeyboardInput { input, .. } => {
						self.input_monitor.process_keyboard_input(input);
					},
					WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
						self.input_monitor.process_mouse_input(state);
					},
					WindowEvent::MouseWheel {
						delta: MouseScrollDelta::LineDelta(lines, rows), ..
					} => {
						// Negative multiplier = reverse scrolling; positive multiplier = natural scrolling.
						self.position[0] = self.position[0] + lines * -48.;
						self.position[1] = self.position[1] + rows * -48.;
						self.renderer.reposition(self.position);
					},
					WindowEvent::CursorMoved { position, .. } => {
						self.cursor_x = position.x;
						self.cursor_y = position.y;
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
				self.process_input();
				self.window.request_redraw();
			},

			// If a window redraw is requested, have the renderer update and render.
			Event::RedrawRequested(window_id) if window_id == self.window.id() => {
				self.update_renderer();

				// Only render if it's been too long since the last render.
				if self.should_redraw || (Instant::now() - self.last_frame_instant) >= Duration::new(1, 0) / 90 {
					self.last_frame_instant = Instant::now();

					match self.repaint() {
						Ok(_) => {},
						Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.width, self.renderer.height),
						Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
						Err(e) => eprintln!("{:?}", e),
					}

					self.should_redraw = false;
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

		let colorwheel_instances = if let Mode::ChoosingColor { cursor_origin, .. } = self.mode_stack.get() {
			vec![ColorWheelInstance {
				position: (cursor_origin - 160.).into_array(),
				radius_major: 160.,
				radius_minor: 120.,
				depth: 0.,
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

		self.renderer.render(&card_instances, strokes_vertices, strokes_indices, colorwheel_instances)
	}

	fn poll_tablet(&mut self) {
		use Input::*;
		if !self.input_monitor.inputs[LMouse].is_active {
			self.pressure = None;
		}

		if let Some(buf) = self.tablet_context.as_mut().map(|c| c.get_packets(50)) {
			if let Some(packet) = buf.last() {
				self.pressure = Some(f64::from(packet.normal_pressure));
			}
		}
	}

	fn process_input(&mut self) {
		use Input::*;

		if self.input_monitor.is_fresh {
			self.should_redraw = true;

			use crate::input::Input::*;
			if self.input_monitor.inputs[B].was_pressed() {
				self.mode_stack.switch_draw();
			}
			if self.input_monitor.inputs[Q].was_pressed() {
				for stroke in self.canvas.strokes.iter_mut().filter(|x| x.is_selected) {
					stroke.is_selected = false;
				}
			}
			if self.input_monitor.inputs[S].was_pressed() {
				self.mode_stack.switch_select();
			}
			if self.input_monitor.inputs[T].was_pressed() {
				self.mode_stack.switch_move();
			}
			if self.input_monitor.inputs[X].was_pressed() {
				for _ in self.canvas.strokes.drain_filter(|x| x.is_selected) {}
			}
			if self.input_monitor.inputs[Z].was_emitted() {
				if self.mode_stack.is_drafting() {
					self.mode_stack.discard_draft();
				} else {
					self.canvas.strokes.pop();
				}
			}
			if self.input_monitor.inputs[Escape].was_pressed() {
				self.mode_stack.discard_draft();
			}
			if self.input_monitor.inputs[Space].is_different {
				if self.input_monitor.inputs[Space].is_active {
					self.mode_stack.temp_pan(true);
				} else {
					self.mode_stack.temp_pan(false);
				}
			}
			if self.input_monitor.inputs[Tab].is_different {
				if self.input_monitor.inputs[Tab].is_active {
					self.mode_stack.temp_choose(Some(Vec2::new(self.cursor_x as f32, self.cursor_y as f32)));
				} else {
					self.mode_stack.temp_choose(None);
				}
			}
		}

		match self.mode_stack.get_mut() {
			Mode::Drawing { current_stroke } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Arrow);
				if self.input_monitor.inputs[LMouse].is_active {
					if self.input_monitor.inputs[LMouse].is_different && current_stroke.is_none() {
						*current_stroke = Some(Stroke::new(self.current_color));
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

				if self.input_monitor.inputs[LMouse].is_active {
					if self.input_monitor.inputs[LMouse].is_different && origin.is_none() {
						*origin = Some(Vec2::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>());
					}
				} else {
					if let Some(origin) = origin.take() {
						let current = Vec2::<f32>::from(self.position) + Vec2::new(self.cursor_x, self.cursor_y).as_::<f32>();
						let min = Vec2::new(current.x.min(origin.x), current.y.min(origin.y));
						let max = Vec2::new(current.x.max(origin.x), current.y.max(origin.y));
						self.canvas.select(min, max, self.input_monitor.inputs[LShift].is_active);
					}
				}
			},
			Mode::Panning { origin } => {
				if self.input_monitor.inputs[LMouse].is_active {
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

				if self.input_monitor.inputs[LMouse].is_active {
					if self.input_monitor.inputs[LMouse].is_different && origin.is_none() {
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
			Mode::ChoosingColor { cursor_origin, is_selecting_hue } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);

				if self.input_monitor.inputs[LMouse].is_active && (self.input_monitor.inputs[LMouse].is_different || *is_selecting_hue) {
					let cursor = Vec2::new(self.cursor_x as f32, self.cursor_y as f32);
					let center = *cursor_origin;
					let vector = cursor - center;
					let magnitude = vector.magnitude();
					if magnitude >= 120. && magnitude <= 160. {
						*is_selecting_hue = true;
						let normalized_angle = vector.y.atan2(vector.x) / (2.0 * std::f32::consts::PI) + 0.5;

						fn hsv_to_srgb(h: f32, s: f32, v: f32) -> [f32; 3] {
							fn hue(h: f32) -> [f32; 3] {
								[(h * 6. - 3.).abs() - 1., 2. - (h * 6. - 2.).abs(), 2. - (h * 6. - 4.).abs()].map(|n| n.clamp(0., 1.))
							}
							hue(h).map(|n: f32| ((n - 1.) * s + 1.) * v)
						}

						let srgb = hsv_to_srgb(normalized_angle, 1., 1.).map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 });

						self.current_color = [srgb[0], srgb[1], srgb[2], 0xff];
					}
				}
			},
		}

		// FIXME: This is a little wasteful. Is it feasible to only update if something actually does change on-screen?
		self.should_redraw = true;

		// Reset inputs.
		self.input_monitor.defresh();
	}

	fn update_renderer(&mut self) {
		// Apply a resize if necessary; resizes are time-intensive.
		if let Some(size) = self.pending_resize.take() {
			self.renderer.resize(size.width, size.height);
		}

		self.renderer.update();
	}
}
