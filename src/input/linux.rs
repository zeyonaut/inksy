// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![cfg(target_os = "linux")]

#[repr(C)]
pub struct Packet {
	pub normal_pressure: u32,
}

pub struct TabletContext {}

impl TabletContext {
	pub fn new(window: &winit::window::Window) -> Option<Self> {
		Some(Self {})
	}

	pub fn enable(&mut self, enable: bool) -> Result<(), ()> {
		Ok(())
	}

	pub fn get_queue_size(&self) -> isize {
		0
	}

	pub fn get_packets(&mut self, num: usize) -> Box<[Packet]> {
		Vec::new().into_boxed_slice()
	}
}
