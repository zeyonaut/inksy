// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use enum_map::{enum_map, Enum, EnumMap};
use winit::event::{ElementState, KeyboardInput};

#[derive(Enum, Copy, Clone)]
pub enum Input {
	K0,
	K1,
	K2,
	K3,
	K4,
	K5,
	K6,
	K7,
	K8,
	K9,
	A,
	B,
	C,
	D,
	E,
	F,
	G,
	H,
	I,
	J,
	K,
	L,
	M,
	N,
	O,
	P,
	Q,
	R,
	S,
	T,
	U,
	V,
	W,
	X,
	Y,
	Z,
	Escape,
	Space,
	Tab,
	LMouse,
	LShift,
}

pub struct InputState {
	pub is_active: bool,
	pub is_fresh: bool,
	pub is_different: bool,
}

impl InputState {
	pub const fn new() -> Self {
		Self {
			is_active: false,
			is_fresh: false,
			is_different: false,
		}
	}

	pub fn was_emitted(&self) -> bool {
		self.is_active && self.is_fresh
	}

	pub fn was_pressed(&self) -> bool {
		self.is_active && self.is_different
	}
}

pub struct InputMonitor {
	pub inputs: EnumMap<Input, InputState>,
	pub is_fresh: bool,
}

impl InputMonitor {
	pub fn new() -> Self {
		Self {
			inputs: enum_map! {_ => InputState::new()},
			is_fresh: false,
		}
	}

	pub fn process_keyboard_input(&mut self, keyboard_input: &KeyboardInput) {
		if let Some(keycode) = keyboard_input.virtual_keycode {
			use winit::event::VirtualKeyCode;
			use Input::*;
			let input = match keycode {
				VirtualKeyCode::Key1 => K0,
				VirtualKeyCode::Key2 => K1,
				VirtualKeyCode::Key3 => K2,
				VirtualKeyCode::Key4 => K3,
				VirtualKeyCode::Key5 => K4,
				VirtualKeyCode::Key6 => K5,
				VirtualKeyCode::Key7 => K6,
				VirtualKeyCode::Key8 => K7,
				VirtualKeyCode::Key9 => K8,
				VirtualKeyCode::Key0 => K9,
				VirtualKeyCode::A => A,
				VirtualKeyCode::B => B,
				VirtualKeyCode::C => C,
				VirtualKeyCode::D => D,
				VirtualKeyCode::E => E,
				VirtualKeyCode::F => F,
				VirtualKeyCode::G => G,
				VirtualKeyCode::H => H,
				VirtualKeyCode::I => I,
				VirtualKeyCode::J => J,
				VirtualKeyCode::K => K,
				VirtualKeyCode::L => L,
				VirtualKeyCode::M => M,
				VirtualKeyCode::N => N,
				VirtualKeyCode::O => O,
				VirtualKeyCode::P => P,
				VirtualKeyCode::Q => Q,
				VirtualKeyCode::R => R,
				VirtualKeyCode::S => S,
				VirtualKeyCode::T => T,
				VirtualKeyCode::U => U,
				VirtualKeyCode::V => V,
				VirtualKeyCode::W => W,
				VirtualKeyCode::X => X,
				VirtualKeyCode::Y => Y,
				VirtualKeyCode::Z => Z,
				VirtualKeyCode::Escape => Escape,
				VirtualKeyCode::Space => Space,
				VirtualKeyCode::Tab => Tab,
				VirtualKeyCode::LShift => LShift,
				_ => return,
			};
			let is_active = keyboard_input.state == ElementState::Pressed;
			self.inputs[input].is_fresh = true;
			self.inputs[input].is_different = self.inputs[input].is_active != is_active;
			self.inputs[input].is_active = is_active;
			self.is_fresh = true;
		}
	}

	pub fn process_mouse_input(&mut self, element_state: &ElementState) {
		use Input::*;
		let is_active = *element_state == ElementState::Pressed;
		self.inputs[LMouse].is_fresh = true;
		self.inputs[LMouse].is_different = self.inputs[LMouse].is_active != is_active;
		self.inputs[LMouse].is_active = is_active;
		self.is_fresh = true;
	}

	pub fn defresh(&mut self) {
		self.inputs.iter_mut().for_each(|(_, input_state)| {
			input_state.is_fresh = false;
			input_state.is_different = false
		});
		self.is_fresh = false;
	}
}
