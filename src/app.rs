// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::{Duration, Instant};

use fast_srgb8::srgb8_to_f32;
use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::*,
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

#[cfg(target_os = "linux")]
use crate::input::linux::*;
#[cfg(target_os = "windows")]
use crate::input::wintab::*;
use crate::{
	actions::default_keymap,
	canvas::{Canvas, Image, IncompleteStroke, Object, Operation, Stroke},
	clipboard::Clipboard,
	input::{
		keymap::{execute_keymap, Keymap},
		Button, InputMonitor, Key,
	},
	pixel::{Lx, Px, Scale, Vex, Zero, Zoom},
	render::{DrawCommand, Renderer},
	tools::*,
	utility::*,
	APP_NAME_CAPITALIZED,
};

// TODO: Move this somewhere saner.
// Color selector constants in logical pixels/points.
const TRIGON_RADIUS: Lx = Lx(68.);
const HOLE_RADIUS: Lx = Lx(80.);
const RING_WIDTH: Lx = Lx(28.);
const OUTLINE_WIDTH: Lx = Lx(2.);
const SATURATION_VALUE_WINDOW_DIAMETER: Lx = Lx(8.);

pub enum ClipboardContents {
	Subcanvas(Vec<Object<Image>>, Vec<Object<Stroke>>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreFullscreenState {
	Normal(PhysicalPosition<i32>, PhysicalSize<u32>),
	Maximized,
}

// Current state of our app.
pub struct App {
	pub window: winit::window::Window,
	pub clipboard: Clipboard,
	pub pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
	pub should_redraw: bool,
	pub renderer: Renderer,
	pub cursor_physical_position: Vex<2, Px>,
	pub scale: Scale,
	pub is_cursor_relevant: bool,
	pub tablet_context: Option<TabletContext>,
	pub pressure: Option<f64>,
	pub canvas: Canvas,
	pub was_canvas_saved: bool,
	pub mode_stack: ModeStack,
	pub last_frame_instant: std::time::Instant,
	pub input_monitor: InputMonitor,
	pub keymap: Keymap,
	pub current_color: HSV,
	pub clipboard_contents: Option<ClipboardContents>,
	pub pre_fullscreen_state: Option<PreFullscreenState>,
}

impl App {
	// Sets up the logger and renderer.
	pub fn new(event_loop: &EventLoop<()>) -> Self {
		let keymap = default_keymap();

		// Create a window.
		let window = WindowBuilder::new().with_title(crate::APP_NAME_CAPITALIZED).with_visible(false).build(event_loop).unwrap();

		// Set the icon (on Windows).
		#[cfg(target_os = "windows")]
		{
			use winit::platform::windows::WindowExtWindows;
			crate::windows::set_window_icon(window.hwnd());
		}

		// Resize the window to a reasonable size.
		let monitor_size = window.current_monitor().unwrap().size();
		window.set_inner_size(PhysicalSize::new(monitor_size.width as f64 / 1.5, monitor_size.height as f64 / 1.5));
		window.set_outer_position(PhysicalPosition::new(monitor_size.width as f64 / 6., monitor_size.height as f64 / 6.));

		// Attempt to establish a tablet context.
		let tablet_context = TabletContext::new(&window);

		// Set up the renderer.
		let size = window.inner_size();
		let scale_factor = window.scale_factor() as f32;
		let renderer = Renderer::new(&window, size.width, size.height, scale_factor);

		// Make the window visible and immediately clear color to prevent a flash.
		let output = renderer
			.clear(wgpu::Color {
				r: srgb8_to_f32(0x12) as f64,
				g: srgb8_to_f32(0x12) as f64,
				b: srgb8_to_f32(0x12) as f64,
				a: srgb8_to_f32(0xff) as f64,
			})
			.unwrap();
		window.set_visible(true);
		// FIXME: This sometimes flashes, and sometimes doesn't.
		output.present();

		// Return a new instance of the app state.
		Self {
			window,
			clipboard: Clipboard::new().unwrap(),
			pending_resize: None,
			should_redraw: false,
			renderer,
			scale: Scale(scale_factor),
			cursor_physical_position: Vex::ZERO,
			is_cursor_relevant: false,
			tablet_context,
			pressure: None,
			canvas: Canvas::new(HSV([0., 0., 0.07])),
			was_canvas_saved: false,
			mode_stack: ModeStack::new(Tool::Draw { current_stroke: None }),
			last_frame_instant: Instant::now() - Duration::new(1, 0),
			input_monitor: InputMonitor::new(),
			keymap,
			current_color: HSV([2. / 3., 0.016, 1.]),
			clipboard_contents: None,
			pre_fullscreen_state: None,
		}
	}

	// Runs the event loop with the event handler.
	pub fn run(mut self, event_loop: EventLoop<()>) {
		// Update the window title.
		self.update_window_title();

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
						if !self.input_monitor.active_keys.contains(Key::Control) {
							// Negative multiplier = reverse scrolling; positive multiplier = natural scrolling.
							self.canvas.view.position = self.canvas.view.position + Vex([*lines, *rows].map(Lx)).z(self.canvas.view.zoom).rotate(self.canvas.view.tilt) * -32.;
						} else {
							self.canvas.view.zoom = Zoom(self.canvas.view.zoom.0 * f32::powf(2., *rows / 32.));
						}
						self.canvas.is_view_dirty = true;
						self.should_redraw = true;
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
						self.pending_resize = Some(physical_size.clone());
						self.should_redraw = true;
					},
					WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
						self.scale = Scale(*scale_factor as f32);
						self.pending_resize = Some(**new_inner_size);
						self.should_redraw = true;
					},
					// Ignore all other window events.
					_ => {},
				}
			},

			Event::MainEventsCleared => {
				self.poll_tablet();
				self.process_input();
				self.window.request_redraw();
			},

			// If a window redraw is requested, have the renderer update and render.
			Event::RedrawRequested(window_id) if window_id == self.window.id() => {
				self.update_renderer();
				if self.should_redraw || (Instant::now() - self.last_frame_instant) >= Duration::new(1, 0) / 90 {
					self.last_frame_instant = Instant::now();
					match self.repaint() {
						Ok(_) => {},
						Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.config.width, self.renderer.config.height, self.renderer.scale_factor),
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
			_ => return,
		}
	}

	fn repaint(&mut self) -> Result<(), wgpu::SurfaceError> {
		let mut draw_commands: Vec<DrawCommand> = vec![];

		let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px)).s(self.scale).z(self.canvas.view.zoom);
		let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(self.canvas.view.zoom) - semidimensions).rotate(self.canvas.view.tilt);

		// Draw brushstrokes and images.
		let selection_offset = if let Tool::Move { origin: Some(origin) } = &self.mode_stack.base_mode {
			Some(self.canvas.view.position + cursor_virtual_position - *origin)
		} else {
			None
		};

		let selection_angle = if let Tool::Rotate {
			origin: Some(RotateDraft { center, initial_position }),
		} = &self.mode_stack.base_mode
		{
			let selection_offset = self.canvas.view.position + cursor_virtual_position - center;
			let angle = initial_position.angle_to(selection_offset);
			Some((center.clone(), angle))
		} else {
			None
		};

		let selection_dilation = if let Tool::Resize {
			origin: Some(ResizeDraft { center, initial_distance }),
		} = &self.mode_stack.base_mode
		{
			let selection_distance = (self.canvas.view.position + cursor_virtual_position - center).norm();
			let dilation = selection_distance / initial_distance;
			Some((center.clone(), dilation))
		} else {
			None
		};

		self.canvas.bake(&mut draw_commands, self.mode_stack.current_stroke(), selection_offset, selection_angle, selection_dilation);

		match &self.mode_stack.get() {
			Tool::Select { origin: Some(origin) } => {
				let current = (cursor_virtual_position.rotate(-self.canvas.view.tilt) + semidimensions).z(self.canvas.view.zoom).s(self.scale);
				let origin = ((origin - self.canvas.view.position).rotate(-self.canvas.view.tilt) + semidimensions).z(self.canvas.view.zoom).s(self.scale);
				let topleft = Vex([current[0].min(origin[0]), current[1].min(origin[1])]);
				draw_commands.push(DrawCommand::Card {
					position: topleft,
					dimensions: (current - origin).map(|n| n.abs()),
					color: [0x22, 0xae, 0xd1, 0x33],
					radius: Px(0.),
				});
			},
			Tool::Orbit { .. } => {
				let center = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px));
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
				let srgba8 = self.current_color.to_srgb().to_srgba8();
				draw_commands.push(DrawCommand::Card {
					position: center.map(|x| x - hue_window_width / 2.),
					dimensions: Vex([hue_window_width; 2]),
					color: srgba8.0,
					radius: hue_window_width / 2.,
				});
			},
			Tool::PickColor { cursor_physical_origin: cursor_origin, .. } => {
				draw_commands.push(DrawCommand::ColorSelector {
					position: cursor_origin.map(|x| x - (HOLE_RADIUS + RING_WIDTH).s(self.scale)),
					hsv: self.current_color.0,
					trigon_radius: TRIGON_RADIUS.s(self.scale),
					hole_radius: HOLE_RADIUS.s(self.scale),
					ring_width: RING_WIDTH.s(self.scale),
				});

				let srgba8 = self.current_color.to_srgb().to_srgba8();

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
					color: srgba8.0,
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
					color: srgba8.0,
					radius: sv_window_width / 2.,
				});
			},
			_ => {},
		}

		self.renderer.render(&mut self.canvas, draw_commands)
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

		let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px)).s(self.scale).z(self.canvas.view.zoom);
		let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(self.canvas.view.zoom) - semidimensions).rotate(self.canvas.view.tilt);

		if self.input_monitor.is_fresh {
			self.should_redraw = true;
			execute_keymap(self, self.input_monitor.active_keys, self.input_monitor.fresh_keys, self.input_monitor.different_keys);
		}

		match self.mode_stack.get_mut() {
			Tool::Draw { current_stroke } => {
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Default);
				}
				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && current_stroke.is_none() {
						let srgba8 = self.current_color.to_srgb().to_srgba8();
						*current_stroke = Some(IncompleteStroke::new(cursor_virtual_position, srgba8));
					}

					if let Some(current_stroke) = current_stroke {
						let offset = self.canvas.view.position + cursor_virtual_position - current_stroke.position;
						current_stroke.add_point(offset, self.pressure.map_or(1., |pressure| (pressure / 32767.) as f32))
					}
				} else {
					if let Some(stroke) = current_stroke.take() {
						self.canvas.perform_operation(Operation::CommitStrokes { strokes: vec![stroke.commit()] });
					}
				}
			},
			Tool::Select { origin } => {
				let offset = cursor_virtual_position + self.canvas.view.position;
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);
				}

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						*origin = Some(offset);
					}
				} else {
					if let Some(origin) = origin.take() {
						let offset = cursor_virtual_position.rotate(-self.canvas.view.tilt);
						let origin = (origin - self.canvas.view.position).rotate(-self.canvas.view.tilt);
						let min = Vex([offset[0].min(origin[0]), offset[1].min(origin[1])]);
						let max = Vex([offset[0].max(origin[0]), offset[1].max(origin[1])]);
						self.canvas.select(min, max, self.canvas.view.tilt, self.canvas.view.position, self.input_monitor.active_keys.contains(Shift));
					}
				}
			},
			Tool::Pan { origin } => {
				if self.input_monitor.active_buttons.contains(Left) {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					}
					if origin.is_none() {
						*origin = Some(PanOrigin {
							cursor: cursor_virtual_position,
							position: self.canvas.view.position,
						});
					}
				} else {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					}
					if origin.is_some() {
						*origin = None;
					}
				}

				if let Some(origin) = origin {
					self.canvas.view.position = origin.position - (cursor_virtual_position - origin.cursor);
					self.canvas.is_view_dirty = true;
				}
			},
			Tool::Zoom { origin } => {
				if self.input_monitor.active_buttons.contains(Left) {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					}
					if origin.is_none() {
						let window_height = Px(self.renderer.config.height as f32);
						*origin = Some(ZoomOrigin {
							initial_zoom: self.canvas.view.zoom.0,
							initial_y_ratio: self.cursor_physical_position[1] / window_height,
						});
					}
				} else {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					}
					if origin.is_some() {
						*origin = None;
					}
				}

				if let Some(origin) = origin {
					let window_height = Px(self.renderer.config.height as f32);
					let y_ratio = self.cursor_physical_position[1] / window_height;
					let zoom_ratio = f32::powf(8., origin.initial_y_ratio - y_ratio);
					self.canvas.view.zoom = Zoom(origin.initial_zoom * zoom_ratio);
					self.canvas.is_view_dirty = true;
				}
			},
			Tool::Orbit { initial } => {
				if self.input_monitor.active_buttons.contains(Left) {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
					}
					if initial.is_none() {
						let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px));
						let vector = self.cursor_physical_position - semidimensions;
						let angle = vector.angle();
						*initial = Some(OrbitInitial {
							tilt: self.canvas.view.tilt,
							cursor_angle: angle,
						});
					}
				} else {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Grab);
					}
					if initial.is_some() {
						*initial = None;
					}
				}

				if let Some(OrbitInitial { tilt, cursor_angle }) = initial {
					let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px));
					let vector = self.cursor_physical_position - semidimensions;
					let angle = vector.angle();
					self.canvas.view.tilt = *tilt - angle + *cursor_angle;
					self.canvas.is_view_dirty = true;
				}
			},
			Tool::Move { origin } => {
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Move);
				}

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						*origin = Some(self.canvas.view.position + cursor_virtual_position);
					}
				} else {
					if let Some(origin) = origin.take() {
						let selection_offset = self.canvas.view.position + cursor_virtual_position - origin;

						let selected_image_indices = self.canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = self.canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							self.canvas.perform_operation(Operation::TranslateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								vector: selection_offset,
							});
						}
					}
				}
			},
			Tool::Rotate { origin } => {
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Move);
				}

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						// Compute the centroid.
						let (sum, count) = self.canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

						let (sum, count) = self.canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

						let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

						*origin = Some({
							RotateDraft {
								center,
								initial_position: self.canvas.view.position + cursor_virtual_position - center,
							}
						});
					}
				} else {
					if let Some(RotateDraft { center, initial_position }) = origin.take() {
						let selection_offset = self.canvas.view.position + cursor_virtual_position - center;
						let angle = initial_position.angle_to(selection_offset);

						let selected_image_indices = self.canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = self.canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							self.canvas.perform_operation(Operation::RotateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								angle,
							});
						}
					}
				}
			},
			Tool::Resize { origin } => {
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Move);
				}

				if self.input_monitor.active_buttons.contains(Left) {
					if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
						// Compute the centroid.
						let (sum, count) = self.canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

						let (sum, count) = self.canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

						let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

						*origin = Some({
							ResizeDraft {
								center,
								initial_distance: (self.canvas.view.position + cursor_virtual_position - center).norm(),
							}
						});
					}
				} else {
					if let Some(ResizeDraft { center, initial_distance }) = origin.take() {
						let selection_distance = (self.canvas.view.position + cursor_virtual_position - center).norm();
						let dilation = selection_distance / initial_distance;

						let selected_image_indices = self.canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = self.canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							self.canvas.perform_operation(Operation::ResizeObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								dilation,
							});
						}
					}
				}
			},
			Tool::PickColor { cursor_physical_origin, part } => {
				if self.is_cursor_relevant {
					self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);
				}

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

		if self.was_canvas_saved != self.canvas.is_saved() {
			self.was_canvas_saved = !self.was_canvas_saved;
			self.update_window_title();
		}

		// Reset inputs.
		self.input_monitor.defresh();
	}

	pub fn update_window_title(&mut self) {
		if self.was_canvas_saved {
			self.window.set_title(&format!(
				"{} - {}",
				self.canvas.file_path.as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
				APP_NAME_CAPITALIZED
			))
		} else {
			self.window.set_title(&format!(
				"*{} - {}",
				self.canvas.file_path.as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
				APP_NAME_CAPITALIZED
			))
		}
	}

	fn update_renderer(&mut self) {
		// Apply a resize if necessary; resizes are time-intensive.
		if let Some(size) = self.pending_resize.take() {
			self.renderer.resize(size.width, size.height, self.scale.0);
		}

		self.renderer.update();
	}
}
