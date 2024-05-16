// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![feature(array_try_map)]
#![feature(array_windows)]
#![feature(extract_if)]
#![feature(maybe_uninit_uninit_array_transpose)]
// We disable windows_subsystem = "windows" in debug mode to show wgpu validation errors.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod canvas;
mod clipboard;
mod config;
mod file;
#[path = "input/_.rs"]
mod input;
#[path = "render/_.rs"]
mod render;
mod tools;
#[path = "utility/_.rs"]
mod utility;
#[cfg(target_os = "windows")]
mod windows;

use app::App;
use winit::event_loop::EventLoopBuilder;

pub const APP_NAME_CAPITALIZED: &str = "Inksy";
pub const APP_NAME_LOWERCASE: &str = "inksy";

// Program entry point.
fn main() {
	// Set up the event logger.
	env_logger::init();

	// Initialize the event loop.
	let event_loop = EventLoopBuilder::new().build().unwrap();

	// Create a window.
	let window = winit::window::WindowBuilder::new().with_title(crate::APP_NAME_CAPITALIZED).with_visible(false).build(&event_loop).unwrap();

	// Set the icon (on Windows).
	#[cfg(target_os = "windows")]
	{
		crate::windows::set_window_icon(crate::windows::window_hwnd(&window).into());
	}

	// Resize the window to a reasonable size.
	let monitor_size = window.current_monitor().unwrap().size();
	let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(monitor_size.width as f64 / 1.5, monitor_size.height as f64 / 1.5));
	window.set_outer_position(winit::dpi::PhysicalPosition::new(monitor_size.width as f64 / 6., monitor_size.height as f64 / 6.));

	// Initialize the app at the event loop.
	let app = App::new(&window);

	// Run the app with its event loop.
	app.run(event_loop);
}
