// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::{Duration, Instant};

use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::*,
	event_loop::{EventLoop, EventLoopWindowTarget},
};

#[cfg(target_os = "linux")]
use crate::input::linux::*;
#[cfg(target_os = "windows")]
use crate::input::wintab::*;
use crate::{
	actions::default_keymap,
	canvas::{Canvas, Image, IncompleteStroke, Operation, Stroke},
	clipboard::Clipboard,
	config::Config,
	input::{
		keymap::{execute_keymap, Keymap},
		Button, InputMonitor, Key,
	},
	render::{stroke_renderer::SelectionTransformation, DrawCommand, Renderer},
	tools::*,
	utility::{Lx, Px, Scale, Vex, Zero, Zoom},
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
	Subcanvas(Vec<Image>, Vec<Stroke>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreFullscreenState {
	Normal(PhysicalPosition<i32>, PhysicalSize<u32>),
	Maximized,
}

// Current state of our app.
pub struct App<'window> {
	pub config: Config,
	pub clipboard: Clipboard,
	pub pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
	pub should_redraw: bool,
	pub renderer: Renderer<'window>,
	pub cursor_physical_position: Vex<2, Px>,
	pub scale: Scale,
	pub is_cursor_relevant: bool,
	pub tablet_context: Option<TabletContext>,
	pub pressure: Option<f64>,
	pub canvases: Vec<Canvas>,
	pub current_canvas_index: Option<usize>,
	pub was_canvas_saved: bool,
	pub mode_stack: ModeStack,
	pub last_frame_instant: std::time::Instant,
	pub input_monitor: InputMonitor,
	pub keymap: Keymap,
	pub clipboard_contents: Option<ClipboardContents>,
	pub pre_fullscreen_state: Option<PreFullscreenState>,
	pub window: &'window winit::window::Window,
}

impl<'window> App<'window> {
	// Sets up the logger and renderer.
	pub fn new(window: &'window winit::window::Window) -> Self {
		let config = Config::load().unwrap_or_default();
		let keymap = default_keymap();

		// Attempt to establish a tablet context.
		let tablet_context = TabletContext::new(window);

		// Set up the renderer.
		let size = window.inner_size();
		let scale_factor = window.scale_factor() as f32;
		let renderer = Renderer::new(window, size.width, size.height, scale_factor);

		// Make the window visible and immediately clear color to prevent a flash.
		let clear_color = config.default_canvas_color.opaque().to_lrgba().0.map(f64::from);
		let output = renderer
			.clear(wgpu::Color {
				r: clear_color[0],
				g: clear_color[1],
				b: clear_color[2],
				a: clear_color[3],
			})
			.unwrap();
		window.set_visible(true);
		// FIXME: This sometimes flashes, and sometimes doesn't.
		output.present();

		// Return a new instance of the app state.
		Self {
			clipboard: Clipboard::new().unwrap(),
			pending_resize: None,
			should_redraw: false,
			renderer,
			scale: Scale(scale_factor),
			cursor_physical_position: Vex::ZERO,
			is_cursor_relevant: false,
			tablet_context,
			pressure: None,
			canvases: Vec::new(),
			current_canvas_index: None,
			was_canvas_saved: false,
			mode_stack: ModeStack::new(Tool::Draw { current_stroke: None }),
			last_frame_instant: Instant::now() - Duration::new(1, 0),
			input_monitor: InputMonitor::new(),
			keymap,
			clipboard_contents: None,
			pre_fullscreen_state: None,
			config,
			window,
		}
	}

	// Runs the event loop with the event handler.
	pub fn run(mut self, event_loop: EventLoop<()>) {
		// Update the window title.
		self.update_window_title();

		// Run the event loop.
		event_loop.run(move |event, window_target| self.handle_event(event, window_target)).unwrap();
	}

	// Handles a single event.
	fn handle_event(&mut self, event: Event<()>, window_target: &EventLoopWindowTarget<()>) {
		match event {
			// Emitted when the event loop resumes.
			Event::NewEvents(_) => {},
			// Check if a window event has occurred.
			Event::WindowEvent { ref event, window_id } if window_id == self.window.id() => {
				match event {
					// If the titlebar close button is clicked  or the escape key is pressed, exit the loop.
					WindowEvent::CloseRequested => window_target.exit(),
					WindowEvent::KeyboardInput { event, .. } => {
						self.input_monitor.process_key_event(event);
					},
					WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
						self.input_monitor.process_mouse_input(state);
					},
					WindowEvent::MouseWheel {
						delta: MouseScrollDelta::LineDelta(lines, rows), ..
					} => {
						if let Some(canvas) = self.current_canvas_index.and_then(|x| self.canvases.get_mut(x)) {
							if !self.input_monitor.active_keys.contains(Key::Control) {
								// Negative multiplier = reverse scrolling; positive multiplier = natural scrolling.
								canvas.view.position = canvas.view.position + Vex([*lines, *rows].map(Lx)).z(canvas.view.zoom).rotate(canvas.view.tilt) * -32.;
							} else {
								canvas.view.zoom = Zoom(canvas.view.zoom.0 * f32::powf(2., *rows / 32.));
							}
							self.should_redraw = true;
						}
					},
					WindowEvent::CursorMoved { position, .. } => {
						self.cursor_physical_position = Vex([position.x as _, position.y as _].map(Px));
					},
					WindowEvent::CursorEntered { .. } => {
						self.is_cursor_relevant = true;
						if let Some(c) = &mut self.tablet_context {
							c.enable(true).unwrap();
						}
					},
					WindowEvent::CursorLeft { .. } => {
						self.is_cursor_relevant = false;
						if let Some(c) = &mut self.tablet_context {
							c.enable(false).unwrap();
						}
					},

					// Resize the window if requested to.
					WindowEvent::Resized(physical_size) => {
						self.pending_resize = Some(*physical_size);
						self.should_redraw = true;
					},
					WindowEvent::ScaleFactorChanged { scale_factor, inner_size_writer: _ } => {
						self.scale = Scale(*scale_factor as f32);
						self.should_redraw = true;
					},

					// If a window redraw is requested, have the renderer update and render.
					WindowEvent::RedrawRequested => {
						self.update_renderer();
						if self.should_redraw || (Instant::now() - self.last_frame_instant) >= Duration::new(1, 0) / 90 {
							self.last_frame_instant = Instant::now();
							match self.repaint() {
								Ok(_) => {},
								Err(wgpu::SurfaceError::Lost) => self.renderer.resize(self.renderer.config.width, self.renderer.config.height, self.renderer.scale_factor),
								Err(wgpu::SurfaceError::OutOfMemory) => window_target.exit(),
								Err(e) => eprintln!("{:?}", e),
							}
							self.should_redraw = false;
						}
					},

					// Ignore all other window events.
					_ => {},
				}
			},

			Event::AboutToWait => {
				self.poll_tablet();
				self.process_input();
				self.window.request_redraw();
			},

			// Ignore all other events.
			_ => (),
		}
	}

	fn repaint(&mut self) -> Result<(), wgpu::SurfaceError> {
		let mut current_canvas = self.current_canvas_index.and_then(|x| self.canvases.get_mut(x));
		let mut draw_commands: Vec<DrawCommand> = vec![];

		if let Some(canvas) = current_canvas.as_mut() {
			let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px)).s(self.scale).z(canvas.view.zoom);
			let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(canvas.view.zoom) - semidimensions).rotate(canvas.view.tilt);

			// TODO: Move this somwhere else; it's more related to input handling than rendering.
			match &self.mode_stack.base_mode {
				Tool::Move { origin: Some(origin) } => {
					*canvas.selection_transformation = SelectionTransformation {
						translation: canvas.view.position + cursor_virtual_position - *origin,
						..Default::default()
					};
				},
				Tool::Rotate {
					origin: Some(RotateDraft { center, initial_position }),
				} => {
					let selection_offset = canvas.view.position + cursor_virtual_position - center;
					let angle = initial_position.angle_to(selection_offset);
					*canvas.selection_transformation = SelectionTransformation {
						center_of_transformation: *center,
						rotation: angle,
						..Default::default()
					};
				},
				Tool::Resize {
					origin: Some(ResizeDraft { center, initial_distance }),
				} => {
					let selection_distance = (canvas.view.position + cursor_virtual_position - center).norm();
					let dilation = selection_distance / initial_distance;
					*canvas.selection_transformation = SelectionTransformation {
						center_of_transformation: *center,
						dilation,
						..Default::default()
					};
				},
				_ => {},
			}

			match &self.mode_stack.get() {
				Tool::Select { origin: Some(origin) } => {
					let current = (cursor_virtual_position.rotate(-canvas.view.tilt) + semidimensions).z(canvas.view.zoom).s(self.scale);
					let origin = ((origin - canvas.view.position).rotate(-canvas.view.tilt) + semidimensions).z(canvas.view.zoom).s(self.scale);
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
					let srgba8 = canvas.stroke_color.to_srgb().to_srgb8().opaque();
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
						hsv: canvas.stroke_color.0,
						trigon_radius: TRIGON_RADIUS.s(self.scale),
						hole_radius: HOLE_RADIUS.s(self.scale),
						ring_width: RING_WIDTH.s(self.scale),
					});

					let srgba8 = canvas.stroke_color.to_srgb().to_srgb8().opaque();

					let ring_position = cursor_origin
						+ Vex([
							(HOLE_RADIUS + RING_WIDTH / 2.).s(self.scale) * -(canvas.stroke_color[0] * 2. * core::f32::consts::PI).cos(),
							(HOLE_RADIUS + RING_WIDTH / 2.).s(self.scale) * -(canvas.stroke_color[0] * 2. * core::f32::consts::PI).sin(),
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
							3.0f32.sqrt() * (canvas.stroke_color[2] - 0.5 * (canvas.stroke_color[1] * canvas.stroke_color[2] + 1.)),
							0.5 * (1. - 3. * canvas.stroke_color[1] * canvas.stroke_color[2]),
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
		}

		self.renderer.render(&self.config, current_canvas, self.mode_stack.current_stroke(), draw_commands)
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

		if self.input_monitor.is_fresh {
			self.should_redraw = true;
			execute_keymap(self, self.input_monitor.active_keys, self.input_monitor.fresh_keys, self.input_monitor.different_keys);
		}
		if let Some(canvas) = self.current_canvas_index.and_then(|x| self.canvases.get_mut(x)) {
			let semidimensions = Vex([self.renderer.config.width as f32 / 2., self.renderer.config.height as f32 / 2.].map(Px)).s(self.scale).z(canvas.view.zoom);
			let cursor_virtual_position = (self.cursor_physical_position.s(self.scale).z(canvas.view.zoom) - semidimensions).rotate(canvas.view.tilt);

			match self.mode_stack.get_mut() {
				Tool::Draw { current_stroke } => {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Default);
					}
					if self.input_monitor.active_buttons.contains(Left) {
						if self.input_monitor.different_buttons.contains(Left) && current_stroke.is_none() {
							*current_stroke = Some(IncompleteStroke::new(cursor_virtual_position, canvas));
						}

						if let Some(current_stroke) = current_stroke {
							let offset = canvas.view.position + cursor_virtual_position - current_stroke.position;
							current_stroke.add_point(offset, self.pressure.map_or(1., |pressure| (pressure / 32767.) as f32))
						}
					} else if let Some(stroke) = current_stroke.take() {
						canvas.perform_operation(Operation::CommitStrokes { strokes: vec![stroke.finalize().into()] });
					}
				},
				Tool::Select { origin } => {
					let offset = cursor_virtual_position + canvas.view.position;
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Crosshair);
					}

					if self.input_monitor.active_buttons.contains(Left) {
						if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
							*origin = Some(offset);
						}
					} else if let Some(origin) = origin.take() {
						let offset = cursor_virtual_position.rotate(-canvas.view.tilt);
						let origin = (origin - canvas.view.position).rotate(-canvas.view.tilt);
						let min = Vex([offset[0].min(origin[0]), offset[1].min(origin[1])]);
						let max = Vex([offset[0].max(origin[0]), offset[1].max(origin[1])]);
						canvas.select(min, max, canvas.view.tilt, canvas.view.position, self.input_monitor.active_keys.contains(Shift));
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
								position: canvas.view.position,
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
						canvas.view.position = origin.position - (cursor_virtual_position - origin.cursor);
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
								initial_zoom: canvas.view.zoom.0,
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
						canvas.view.zoom = Zoom(origin.initial_zoom * zoom_ratio);
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
							*initial = Some(OrbitInitial { tilt: canvas.view.tilt, cursor_angle: angle });
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
						canvas.view.tilt = *tilt - angle + *cursor_angle;
					}
				},
				Tool::Move { origin } => {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if self.input_monitor.active_buttons.contains(Left) {
						if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
							*origin = Some(canvas.view.position + cursor_virtual_position);
						}
					} else if let Some(origin) = origin.take() {
						let selection_offset = canvas.view.position + cursor_virtual_position - origin;

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::TranslateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								vector: selection_offset,
							});
						}

						canvas.selection_transformation = Default::default();
					}
				},
				Tool::Rotate { origin } => {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if self.input_monitor.active_buttons.contains(Left) {
						if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
							// Compute the centroid.
							let (sum, count) = canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

							let (sum, count) = canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

							let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

							*origin = Some({
								RotateDraft {
									center,
									initial_position: canvas.view.position + cursor_virtual_position - center,
								}
							});
						}
					} else if let Some(RotateDraft { center, initial_position }) = origin.take() {
						let selection_offset = canvas.view.position + cursor_virtual_position - center;
						let angle = initial_position.angle_to(selection_offset);

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::RotateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								angle,
							});
						}

						canvas.selection_transformation = Default::default();
					}
				},
				Tool::Resize { origin } => {
					if self.is_cursor_relevant {
						self.window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if self.input_monitor.active_buttons.contains(Left) {
						if self.input_monitor.different_buttons.contains(Left) && origin.is_none() {
							// Compute the centroid.
							let (sum, count) = canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

							let (sum, count) = canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

							let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

							*origin = Some({
								ResizeDraft {
									center,
									initial_distance: (canvas.view.position + cursor_virtual_position - center).norm(),
								}
							});
						}
					} else if let Some(ResizeDraft { center, initial_distance }) = origin.take() {
						let selection_distance = (canvas.view.position + cursor_virtual_position - center).norm();
						let dilation = selection_distance / initial_distance;

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::ResizeObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								dilation,
							});
						}

						canvas.selection_transformation = Default::default();
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
								canvas.stroke_color[0] = vector.angle() / (2.0 * std::f32::consts::PI) + 0.5;
							},
							Some(ColorSelectionPart::SaturationValue) => {
								let scaled_vector = vector / TRIGON_RADIUS.s(self.scale);
								let other = Vex([-(3.0f32.sqrt()) / 2., -1. / 2.]);
								let dot = other.dot(scaled_vector);
								let scaled_vector = scaled_vector + -other * (dot - dot.min(0.5));
								let scaled_vector = Vex([scaled_vector[0].max(-(3.0f32.sqrt()) / 2.), scaled_vector[1].min(0.5)]);
								let s = (1. - 2. * scaled_vector[1]) / (2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]);
								canvas.stroke_color[1] = if s.is_nan() { 0. } else { s.clamp(0., 1.) };
								canvas.stroke_color[2] = ((2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]) / 3.).clamp(0., 1.);
							},
							None => {},
						}
					} else {
						*part = None;
					}
				},
			}

			// TODO: Find a better way to handle this.
			if self.was_canvas_saved != canvas.is_saved() {
				self.was_canvas_saved = !self.was_canvas_saved;
				self.update_window_title();
			}
		}

		// Reset inputs.
		self.input_monitor.defresh();
	}

	pub fn update_window_title(&mut self) {
		let current_canvas = self.current_canvas_index.and_then(|x| self.canvases.get(x));
		if let Some(canvas) = current_canvas {
			if self.was_canvas_saved {
				self.window.set_title(&format!(
					"{} - {}",
					canvas.file_path.as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
					APP_NAME_CAPITALIZED
				));
			} else {
				self.window.set_title(&format!(
					"*{} - {}",
					canvas.file_path.as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
					APP_NAME_CAPITALIZED
				));
			}
		} else {
			self.window.set_title(APP_NAME_CAPITALIZED);
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
