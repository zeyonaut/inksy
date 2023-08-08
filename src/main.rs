// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![feature(array_windows)]
#![feature(drain_filter)]

mod app;
mod buffer;
mod input;
mod render;
mod stroke;
mod wintab;

use app::App;
use winit::event_loop::EventLoopBuilder;

// Program entry point.
fn main() {
	// Set up the event logger.
	env_logger::init();

	// Initialize the event loop.
	let event_loop = EventLoopBuilder::new().build();

	// Initialize the app at the event loop.
	let app = App::new(&event_loop);

	// Run the app with its event loop.
	app.run(event_loop);
}
