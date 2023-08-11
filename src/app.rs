// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::{Duration, Instant};

use enumset::EnumSet;
use fast_srgb8::srgb8_to_f32;
use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::*,
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

#[cfg(target_os = "linux")]
use crate::linux::*;
#[cfg(target_os = "windows")]
use crate::wintab::*;
use crate::{
	input::{Button, InputMonitor, Key},
	pixel::{Lx, Px, Scale, Vex, Vx, Zero, Zoom},
	render::{DrawCommand, Renderer},
	stroke::{Canvas, Stroke},
	tools::*,
};

fn hsv_to_srgb(h: f32, s: f32, v: f32) -> [f32; 3] {
	fn hue(h: f32) -> [f32; 3] {
		[(h * 6. - 3.).abs() - 1., 2. - (h * 6. - 2.).abs(), 2. - (h * 6. - 4.).abs()].map(|n| n.clamp(0., 1.))
	}
	hue(h).map(|n: f32| ((n - 1.) * s + 1.) * v)
}

fn hsv_to_srgba8(hsv: [f32; 3]) -> [u8; 4] {
	let [h, s, v] = hsv;
	let [r, g, b] = hsv_to_srgb(h, s, v).map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 });
	[r, g, b, 0xff]
}

// TODO: Move this somewhere saner.
// Color selector constants in logical pixels/points.
const TRIGON_RADIUS: Lx = Lx(68.);
const HOLE_RADIUS: Lx = Lx(80.);
const RING_WIDTH: Lx = Lx(28.);
const OUTLINE_WIDTH: Lx = Lx(2.);
const SATURATION_VALUE_WINDOW_DIAMETER: Lx = Lx(8.);

pub enum ClipboardContents {
	Subcanvas(Vec<Stroke>),
}

// Current state of our app.
pub struct App {
	window: winit::window::Window,
	pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
	should_redraw: bool,
	renderer: Renderer,
	cursor_physical_position: Vex<2, Px>,
	position: Vex<2, Vx>,
	zoom: Zoom,
	scale: Scale,
	tilt: f32,
	is_cursor_relevant: bool,
	tablet_context: Option<TabletContext>,
	pressure: Option<f64>,
	canvas: Canvas,
	mode_stack: ModeStack,
	last_frame_instant: std::time::Instant,
	input_monitor: InputMonitor,
	current_color: [f32; 3],
	clipboard_contents: Option<ClipboardContents>,
	is_fullscreen: bool,
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
		let position = Vex::ZERO;
		let size = window.inner_size();
		let zoom = 1.;
		let tilt = 0.;
		let scale_factor = window.scale_factor() as f32;
		let mut renderer = Renderer::new(&window, position, size.width, size.height, zoom, tilt, scale_factor);

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
			scale: Scale(scale_factor),
			cursor_physical_position: Vex::ZERO,
			position,
			zoom: Zoom(zoom),
			tilt,
			is_cursor_relevant: false,
			tablet_context,
			pressure: None,
			canvas: Canvas::new(),
			mode_stack: ModeStack::new(Tool::Draw { current_stroke: None }),
			last_frame_instant: Instant::now() - Duration::new(1, 0),
			input_monitor: InputMonitor::new(),
			current_color: [2. / 3., 0.016, 1.],
			clipboard_contents: None,
			is_fullscreen: false,
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
						self.position = self.position + Vex([*lines, *rows].map(Lx)).z(self.zoom) * -32.;
						self.renderer.reposition(self.position);
					},
					WindowEvent::CursorMoved { position, .. } => {
						self.cursor_physical_position = Vex([position.x as _, position.y as _].map(Px));
					},
					WindowEvent::CursorEntered { .. } => {
						self.is_cursor_relevant = true;
						self.tablet_context.as_mut().map(|c: &mut TabletContext| c.enable(true).unwrap());
					},
					WindowEvent::CursorLeft { .. } => {
						self.is_cursor_relevant = false;
						self.tablet_context.as_mut().map(|c| c.enable(false).unwrap());
					},

					// Resize the window if requested to.
					WindowEvent::Resized(physical_size) => {
						self.pending_resize = Some(*physical_size);
					},
					WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
						self.scale = Scale(*scale_factor as f32);
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
						Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.width, self.renderer.height, self.renderer.scale_factor),
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
		let mut draw_commands: Vec<DrawCommand> = vec![];

		let semidimensions = Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px)).s(self.scale).z(self.zoom);
		let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(self.zoom) - semidimensions).rotate(-self.tilt);

		// Draw brushstrokes.
		let selection_offset = if let Tool::Move { origin: Some(origin) } = &self.mode_stack.base_mode {
			Some(self.position + cursor_virtual_position - *origin)
		} else {
			None
		};
		let (strokes_vertices, strokes_indices) = self.canvas.bake(self.mode_stack.current_stroke(), selection_offset);
		draw_commands.push(DrawCommand::Trimesh {
			vertices: strokes_vertices,
			indices: strokes_indices,
		});

		match &self.mode_stack.get() {
			Tool::Select { origin: Some(origin) } => {
				let current = (cursor_virtual_position.rotate(self.tilt) + semidimensions).z(self.zoom).s(self.scale);
				let origin = ((origin - self.position).rotate(self.tilt) + semidimensions).z(self.zoom).s(self.scale);
				let topleft = Vex([current[0].min(origin[0]), current[1].min(origin[1])]);
				draw_commands.push(DrawCommand::Card {
					position: topleft,
					dimensions: (current - origin).map(|n| n.abs()),
					color: [0x22, 0xae, 0xd1, 0x33],
					radius: Px(0.),
				});
			},
			Tool::Orbit { .. } => {
				let center = Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px));
				let hue_outline_width = (SATURATION_VALUE_WINDOW_DIAMETER + 4. * OUTLINE_WIDTH).s(self.scale);
				let hue_frame_width = (SATURATION_VALUE_WINDOW_DIAMETER + 2. * OUTLINE_WIDTH).s(self.scale);
				let hue_window_width = SATURATION_VALUE_WINDOW_DIAMETER.s(self.scale);
				draw_commands.push(DrawCommand::Card {
					position: center.map(|x| x - hue_outline_width / 2.),
					dimensions: Vex([hue_outline_width; 2]),
					color: [0xff; 4],
					radius: hue_outline_width / 2.,
				});
				draw_commands.push(DrawCommand::Card {
					position: center.map(|x| x - hue_frame_width / 2.),
					dimensions: Vex([hue_frame_width; 2]),
					color: [0x00, 0x00, 0x00, 0xff],
					radius: hue_frame_width / 2.,
				});
				let srgba8 = hsv_to_srgba8(self.current_color);
				draw_commands.push(DrawCommand::Card {
					position: center.map(|x| x - hue_window_width / 2.),
					dimensions: Vex([hue_window_width; 2]),
					color: srgba8,
					radius: hue_window_width / 2.,
				});
			},
			Tool::PickColor { cursor_physical_origin: cursor_origin, .. } => {
				draw_commands.push(DrawCommand::ColorSelector {
					position: cursor_origin.map(|x| x - (HOLE_RADIUS + RING_WIDTH).s(self.scale)),
					hsv: self.current_color,
					trigon_radius: TRIGON_RADIUS.s(self.scale),
					hole_radius: HOLE_RADIUS.s(self.scale),
					ring_width: RING_WIDTH.s(self.scale),
				});

				let srgba8 = hsv_to_srgba8(self.current_color);

				let ring_position = cursor_origin
					+ Vex([
						(HOLE_RADIUS + RING_WIDTH / 2.).s(self.scale) * -(self.current_color[0] * 2. * core::f32::consts::PI).cos(),
						(HOLE_RADIUS + RING_WIDTH / 2.).s(self.scale) * -(self.current_color[0] * 2. * core::f32::consts::PI).sin(),
					]);

				let hue_outline_width = (RING_WIDTH + 4. * OUTLINE_WIDTH).s(self.scale);
				let hue_frame_width = (RING_WIDTH + 2. * OUTLINE_WIDTH).s(self.scale);
				let hue_window_width = RING_WIDTH.s(self.scale);
				draw_commands.push(DrawCommand::Card {
					position: ring_position.map(|x| x - hue_outline_width / 2.),
					dimensions: Vex([hue_outline_width; 2]),
					color: [0xff; 4],
					radius: hue_outline_width / 2.,
				});
				draw_commands.push(DrawCommand::Card {
					position: ring_position.map(|x| x - hue_frame_width / 2.),
					dimensions: Vex([hue_frame_width; 2]),
					color: [0x00, 0x00, 0x00, 0xff],
					radius: hue_frame_width / 2.,
				});
				draw_commands.push(DrawCommand::Card {
					position: ring_position.map(|x| x - hue_window_width / 2.),
					dimensions: Vex([hue_window_width; 2]),
					color: srgba8,
					radius: hue_window_width / 2.,
				});

				let trigon_position = cursor_origin
					+ Vex([
						3.0f32.sqrt() * (self.current_color[2] - 0.5 * (self.current_color[1] * self.current_color[2] + 1.)),
						0.5 * (1. - 3. * self.current_color[1] * self.current_color[2]),
					]) * TRIGON_RADIUS.s(self.scale);

				let sv_outline_width = (SATURATION_VALUE_WINDOW_DIAMETER + (4. * OUTLINE_WIDTH)).s(self.scale);
				let sv_frame_width = (SATURATION_VALUE_WINDOW_DIAMETER + (2. * OUTLINE_WIDTH)).s(self.scale);
				let sv_window_width = SATURATION_VALUE_WINDOW_DIAMETER.s(self.scale);
				draw_commands.push(DrawCommand::Card {
					position: trigon_position.map(|x| x - sv_outline_width / 2.),
					dimensions: Vex([sv_outline_width; 2]),
					color: [0xff; 4],
					radius: sv_outline_width / 2.,
				});
				draw_commands.push(DrawCommand::Card {
					position: trigon_position.map(|x| x - sv_frame_width / 2.),
					dimensions: Vex([sv_frame_width; 2]),
					color: [0x00, 0x00, 0x00, 0xff],
					radius: sv_frame_width / 2.,
				});
				draw_commands.push(DrawCommand::Card {
					position: trigon_position.map(|x| x - sv_window_width / 2.),
					dimensions: Vex([sv_window_width; 2]),
					color: srgba8,
					radius: sv_window_width / 2.,
				});
			},
			_ => {},
		}

		self.renderer.render(draw_commands)
	}

	fn poll_tablet(&mut self) {
		use Button::*;
		if !self.input_monitor.active_buttons.contains(Left) {
			self.pressure = None;
		}

		if let Some(buf) = self.tablet_context.as_mut().map(|c| c.get_packets(50)) {
			if let Some(packet) = buf.last() {
				self.pressure = Some(f64::from(packet.normal_pressure));
			}
		}
	}

	fn process_input(&mut self) {
		use Button::*;
		use Key::*;

		let semidimensions = Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px)).s(self.scale).z(self.zoom);
		let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(self.zoom) - semidimensions).rotate(-self.tilt);

		if self.input_monitor.is_fresh {
			self.should_redraw = true;

			use crate::input::Key::*;
			if self.input_monitor.should_trigger(EnumSet::EMPTY, B) {
				self.mode_stack.switch_draw();
			}
			if self.input_monitor.should_trigger(EnumSet::EMPTY, Backspace) {
				for _ in self.canvas.strokes.extract_if(|x| x.is_selected) {}
			}
			if self.input_monitor.should_trigger(LControl | LShift, F) {
				if self.is_fullscreen {
					self.window.set_fullscreen(None);
				} else {
					self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(self.window.current_monitor())));
				}
				self.is_fullscreen = !self.is_fullscreen;
			}
			if self.input_monitor.should_trigger(LControl, F) {
				self.window.set_maximized(!self.window.is_maximized());
			}
			if self.input_monitor.should_trigger(LControl, X) {
				let offset = cursor_virtual_position + self.position;
				self.clipboard_contents = Some(ClipboardContents::Subcanvas(
					self.canvas
						.strokes
						.extract_if(|x| {
							if x.is_selected {
								x.origin = x.origin - offset;
								x.is_selected = false;
								true
							} else {
								false
							}
						})
						.collect(),
				));
			}
			if self.input_monitor.should_trigger(LControl, C) {
				let offset = cursor_virtual_position + self.position;
				self.clipboard_contents = Some(ClipboardContents::Subcanvas(
					self.canvas
						.strokes
						.iter()
						.filter(|x| x.is_selected)
						.map(|stroke| Stroke {
							origin: stroke.origin - offset,
							color: stroke.color,
							points: stroke.points.clone(),
							is_selected: false,
						})
						.collect::<Vec<_>>(),
				))
			}
			if self.input_monitor.should_trigger(LControl, V) {
				if let Some(ClipboardContents::Subcanvas(strokes)) = self.clipboard_contents.as_ref() {
					for stroke in self.canvas.strokes.iter_mut() {
						stroke.is_selected = false;
					}
					let offset = cursor_virtual_position + self.position;
					self.canvas.strokes.extend(strokes.iter().map(|stroke| Stroke {
						origin: stroke.origin + offset,
						color: stroke.color,
						points: stroke.points.clone(),
						is_selected: true,
					}));
				}
			}
			if self.input_monitor.should_trigger(EnumSet::EMPTY, A) {
				for stroke in self.canvas.strokes.iter_mut() {
					stroke.is_selected = true;
				}
			}
			if self.input_monitor.should_trigger(LShift, A) {
				for stroke in self.canvas.strokes.iter_mut() {
					stroke.is_selected = false;
				}
			}
			if self.input_monitor.should_trigger(Tab, R) {
				for stroke in self.canvas.strokes.iter_mut().filter(|stroke| stroke.is_selected) {
					stroke.color = hsv_to_srgba8(self.current_color);
				}
			}
			if self.input_monitor.should_trigger(EnumSet::EMPTY, S) {
				self.mode_stack.switch_select();
			}
			if self.input_monitor.should_trigger(EnumSet::EMPTY, T) {
				self.mode_stack.switch_move();
			}
			if self.input_monitor.should_retrigger(EnumSet::EMPTY, Z) {
				if self.mode_stack.is_drafting() {
					self.mode_stack.discard_draft();
				} else {
					self.canvas.strokes.pop();
				}
			}
			if self.input_monitor.should_trigger(EnumSet::EMPTY, Escape) {
				self.mode_stack.discard_draft();
			}
			if self.input_monitor.was_discovered(EnumSet::EMPTY, Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Pan { should_pan: true });
			} else if self.input_monitor.was_undiscovered(EnumSet::EMPTY, Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Pan { should_pan: false });
			}
			if self.input_monitor.was_discovered(EnumSet::EMPTY, LControl | Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Zoom { should_zoom: true });
			} else if self.input_monitor.was_undiscovered(EnumSet::EMPTY, LControl | Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Zoom { should_zoom: false });
			}
			if self.input_monitor.was_discovered(EnumSet::EMPTY, LShift | Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Orbit { should_orbit: true });
			} else if self.input_monitor.was_undiscovered(EnumSet::EMPTY, LShift | Space) {
				self.mode_stack.switch_transient(TransientModeSwitch::Orbit { should_orbit: false });
			}
			if self.input_monitor.was_discovered(EnumSet::EMPTY, Tab) {
				self.mode_stack.switch_transient(TransientModeSwitch::Color {
					center: Some(if self.is_cursor_relevant {
						self.cursor_physical_position
					} else {
						Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px))
					}),
				});
			} else if self.input_monitor.was_undiscovered(EnumSet::EMPTY, Tab) {
				self.mode_stack.switch_transient(TransientModeSwitch::Color { center: None });
			}
		}

		match self.mode_stack.get_mut() {
			Tool::Draw { current_stroke } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Arrow);
				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && current_stroke.is_none() {
						let srgba8 = hsv_to_srgba8(self.current_color);
						*current_stroke = Some(Stroke::new(self.position, srgba8));
					}

					if let Some(current_stroke) = current_stroke {
						let offset = self.position + cursor_virtual_position;
						current_stroke.add_point(offset, self.pressure.map_or(1., |pressure| (pressure / 32767.) as f32))
					}
				} else {
					self.canvas.strokes.extend(current_stroke.take());
				}
			},
			Tool::Select { origin } => {
				let offset = cursor_virtual_position + self.position;
				self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						*origin = Some(offset);
					}
				} else {
					if let Some(origin) = origin.take() {
						let offset = cursor_virtual_position.rotate(self.tilt);
						let origin = (origin - self.position).rotate(self.tilt);
						let min = Vex([offset[0].min(origin[0]), offset[1].min(origin[1])]);
						let max = Vex([offset[0].max(origin[0]), offset[1].max(origin[1])]);
						self.canvas.select(min, max, self.tilt, self.position, self.input_monitor.active_keys.contains(LShift));
					}
				}
			},
			Tool::Pan { origin } => {
				if self.input_monitor.active_buttons.contains(Left) {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					if origin.is_none() {
						*origin = Some(PanOrigin {
							cursor: cursor_virtual_position,
							position: self.position,
						});
					}
				} else {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					if origin.is_some() {
						*origin = None;
					}
				}

				if let Some(origin) = origin {
					self.position = origin.position - (cursor_virtual_position - origin.cursor);
					self.renderer.reposition(self.position);
				}
			},
			Tool::Zoom { origin } => {
				if self.input_monitor.active_buttons.contains(Left) {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					if origin.is_none() {
						let window_height = Px(self.renderer.height as f32);
						*origin = Some(ZoomOrigin {
							initial_zoom: self.zoom.0,
							initial_y_ratio: self.cursor_physical_position[1] / window_height,
						});
					}
				} else {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					if origin.is_some() {
						*origin = None;
					}
				}

				if let Some(origin) = origin {
					let window_height = Px(self.renderer.height as f32);
					let y_ratio = self.cursor_physical_position[1] / window_height;
					let zoom_ratio = f32::powf(8., origin.initial_y_ratio - y_ratio);
					self.zoom = Zoom(origin.initial_zoom * zoom_ratio);
					self.renderer.rezoom(self.zoom.0);
				}
			},
			Tool::Orbit { initial } => {
				if self.input_monitor.active_buttons.contains(Left) {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					if initial.is_none() {
						let semidimensions = Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px));
						let vector = self.cursor_physical_position - semidimensions;
						let angle = vector.angle();
						*initial = Some(OrbitInitial { tilt: self.tilt, cursor_angle: angle });
					}
				} else {
					self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					if initial.is_some() {
						*initial = None;
					}
				}

				if let Some(OrbitInitial { tilt, cursor_angle }) = initial {
					let semidimensions = Vex([self.renderer.width as f32 / 2., self.renderer.height as f32 / 2.].map(Px));
					let vector = self.cursor_physical_position - semidimensions;
					let angle = vector.angle();
					self.tilt = *tilt + angle - *cursor_angle;
					self.renderer.retilt(self.tilt);
				}
			},
			Tool::Move { origin } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Move);

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						*origin = Some(self.position + cursor_virtual_position);
					}
				} else {
					if let Some(origin) = origin.take() {
						let selection_offset = self.position + cursor_virtual_position - origin;

						for stroke in self.canvas.strokes.iter_mut().filter(|x| x.is_selected) {
							stroke.origin = stroke.origin + selection_offset;
						}
					}
				}
			},
			Tool::PickColor { cursor_physical_origin, part } => {
				self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);

				if self.input_monitor.active_buttons.contains(Left) {
					let cursor = self.cursor_physical_position;
					let vector = cursor - *cursor_physical_origin;
					if part.is_none() && self.input_monitor.different_buttons.contains(Left) {
						let magnitude = vector.norm();
						if magnitude >= HOLE_RADIUS.s(self.scale) && magnitude <= (HOLE_RADIUS + RING_WIDTH).s(self.scale) {
							*part = Some(ColorSelectionPart::Hue);
						} else if 2. * vector[1] < TRIGON_RADIUS.s(self.scale) && -(3.0f32.sqrt()) * vector[0] - vector[1] < TRIGON_RADIUS.s(self.scale) && (3.0f32.sqrt()) * vector[0] - vector[1] < TRIGON_RADIUS.s(self.scale) {
							*part = Some(ColorSelectionPart::SaturationValue);
						}
					}

					match part {
						Some(ColorSelectionPart::Hue) => {
							self.current_color[0] = vector.angle() / (2.0 * std::f32::consts::PI) + 0.5;
						},
						Some(ColorSelectionPart::SaturationValue) => {
							let scaled_vector = vector / TRIGON_RADIUS.s(self.scale);
							let other = Vex([-(3.0f32.sqrt()) / 2., -1. / 2.]);
							let dot = other.dot(scaled_vector);
							let scaled_vector = scaled_vector + -other * (dot - dot.min(0.5));
							let scaled_vector = Vex([scaled_vector[0].max(-3.0f32.sqrt() / 2.), scaled_vector[1].min(0.5)]);
							let s = (1. - 2. * scaled_vector[1]) / (2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]);
							self.current_color[1] = if s.is_nan() { 0. } else { s.clamp(0., 1.) };
							self.current_color[2] = ((2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]) / 3.).clamp(0., 1.);
						},
						None => {},
					}
				} else {
					*part = None;
				}
			},
		}

		// Reset inputs.
		self.input_monitor.defresh();
	}

	fn update_renderer(&mut self) {
		// Apply a resize if necessary; resizes are time-intensive.
		if let Some(size) = self.pending_resize.take() {
			self.renderer.resize(size.width, size.height, self.scale.0);
		}

		self.renderer.update();
	}
}
