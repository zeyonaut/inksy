// Copyright (C) 2024 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
	input::InputMonitor,
	render::{Prerender, Renderer},
	utility::{Px, Scale, Vex},
};

pub trait Widget {
	fn update(&mut self, window: &winit::window::Window, renderer: &Renderer, input_monitor: &InputMonitor, is_cursor_relevant: bool, pressure: Option<f64>, cursor_physical_position: Vex<2, Px>, scale: Scale);
	fn prepare<'a>(&'a mut self, renderer: &mut Renderer, scale: Scale, cursor_physical_position: Vex<2, Px>, prerender: &mut Prerender<'a>);
}
