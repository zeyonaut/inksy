// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::{Duration, Instant};

use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::*,
	event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
};

#[cfg(target_os = "linux")]
use crate::input::linux::*;
#[cfg(target_os = "windows")]
use crate::input::wintab::*;
use crate::{
	actions::default_keymap,
	canvas::{Image, Multicanvas, Stroke},
	clipboard::Clipboard,
	config::Config,
	input::{
		keymap::{execute_keymap, Keymap},
		Button, InputMonitor, Key,
	},
	render::{Prerender, Renderer},
	ui::Widget,
	utility::{Lx, Px, Scale, Vex, Zero, Zoom},
	APP_NAME_CAPITALIZED,
};
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
	pub multicanvas: Multicanvas,
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
			multicanvas: Multicanvas::new(),
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
			Event::WindowEvent { ref event, window_id } if window_id == self.window.id() => 'window_event: {
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
						if let Some(canvas) = self.multicanvas.current_canvas_mut() {
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
						window_target.set_control_flow(ControlFlow::Wait);
						break 'window_event;
					},

					// Ignore all other window events.
					_ => break 'window_event,
				}

				self.poll_tablet();
				self.process_input();
				self.window.request_redraw();
			},

			// Ignore all other events.
			_ => (),
		}
	}

	fn repaint(&mut self) -> Result<(), wgpu::SurfaceError> {
		let mut prerender = Prerender::new();
		self.multicanvas.prepare(&mut self.renderer, self.scale, self.cursor_physical_position, &mut prerender);
		self.renderer.render(&self.config, prerender)
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
		if self.input_monitor.is_fresh {
			self.should_redraw = true;
			execute_keymap(self, self.input_monitor.active_keys, self.input_monitor.fresh_keys, self.input_monitor.different_keys);
		}

		self.multicanvas.update(self.window, &self.renderer, &self.input_monitor, self.is_cursor_relevant, self.pressure, self.cursor_physical_position, self.scale);

		// TODO: Find a better way to handle this.
		if let Some(canvas) = self.multicanvas.current_canvas_index.and_then(|x| self.multicanvas.canvases.get_mut(x)) {
			if self.multicanvas.was_canvas_saved != canvas.is_saved() || canvas.file_path.read_if_dirty().is_some() {
				self.multicanvas.was_canvas_saved = !self.multicanvas.was_canvas_saved;
				self.update_window_title();
			}
		}

		// Reset inputs.
		self.input_monitor.defresh();
	}

	pub fn update_window_title(&mut self) {
		let current_canvas = self.multicanvas.current_canvas_index.and_then(|x| self.multicanvas.canvases.get(x));
		if let Some(canvas) = current_canvas {
			if canvas.is_saved() {
				self.window.set_title(&format!(
					"{} - {}",
					canvas.file_path.as_ref().as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
					APP_NAME_CAPITALIZED
				));
			} else {
				self.window.set_title(&format!(
					"*{} - {}",
					canvas.file_path.as_ref().as_ref().and_then(|file_path| file_path.file_stem()).and_then(|s| s.to_str()).unwrap_or("[Untitled]"),
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
	}
}
