// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod wintab;

use enumset::{EnumSet, EnumSetType};
use winit::event::{ElementState, KeyboardInput};

#[derive(EnumSetType)]
pub enum Key {
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
	Backspace,
	Space,
	Tab,
	LControl,
	LShift,
}

#[derive(EnumSetType)]
pub enum Button {
	Left,
	Right,
}

pub struct InputMonitor {
	pub active_keys: EnumSet<Key>,
	pub fresh_keys: EnumSet<Key>,
	pub different_keys: EnumSet<Key>,
	pub active_buttons: EnumSet<Button>,
	pub different_buttons: EnumSet<Button>,
	pub is_fresh: bool,
}

impl InputMonitor {
	pub fn new() -> Self {
		Self {
			active_keys: EnumSet::EMPTY,
			fresh_keys: EnumSet::EMPTY,
			different_keys: EnumSet::EMPTY,
			active_buttons: EnumSet::EMPTY,
			different_buttons: EnumSet::EMPTY,
			is_fresh: false,
		}
	}

	pub fn process_keyboard_input(&mut self, keyboard_input: &KeyboardInput) {
		if let Some(keycode) = keyboard_input.virtual_keycode {
			use winit::event::VirtualKeyCode;
			use Key::*;
			let key = match keycode {
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
				VirtualKeyCode::Back => Backspace,
				VirtualKeyCode::Escape => Escape,
				VirtualKeyCode::Space => Space,
				VirtualKeyCode::Tab => Tab,
				VirtualKeyCode::LShift => LShift,
				VirtualKeyCode::LControl => LControl,
				_ => return,
			};
			let is_active = keyboard_input.state == ElementState::Pressed;
			self.fresh_keys.insert(key);
			if self.active_keys.contains(key) != is_active {
				self.different_keys.insert(key);
			}
			if is_active {
				self.active_keys.insert(key);
			} else {
				self.active_keys.remove(key);
			}
		}
		self.is_fresh = true;
	}

	pub fn process_mouse_input(&mut self, element_state: &ElementState) {
		use Button::*;
		let is_active = *element_state == ElementState::Pressed;
		if self.active_buttons.contains(Left) != is_active {
			self.different_buttons.insert(Left);
		}
		if is_active {
			self.active_buttons.insert(Left);
		} else {
			self.active_buttons.remove(Left);
		}
		self.is_fresh = true;
	}

	pub fn defresh(&mut self) {
		self.fresh_keys = EnumSet::EMPTY;
		self.different_keys = EnumSet::EMPTY;
		self.different_buttons = EnumSet::EMPTY;
		self.is_fresh = false;
	}

	// Returns true iff the given keystroke was triggered since the last defresh.
	pub fn should_trigger(&self, modifiers: impl Into<EnumSet<Key>>, triggers: impl Into<EnumSet<Key>>) -> bool {
		let triggers = triggers.into();
		(modifiers.into().union(triggers) == self.active_keys) && !self.different_keys.is_empty() && self.different_keys.is_subset(triggers)
	}

	// Returns true iff the given keystroke was (re)triggered since the last defresh.
	pub fn should_retrigger(&self, modifiers: impl Into<EnumSet<Key>>, triggers: impl Into<EnumSet<Key>>) -> bool {
		let triggers = triggers.into();
		(modifiers.into().union(triggers) == self.active_keys) && !self.fresh_keys.is_empty() && self.fresh_keys.is_subset(triggers)
	}

	// Return true iff the given keystroke was "discovered" since the last defresh.
	// A keystroke can be discovered multiple times before it is undiscovered.
	pub fn was_discovered(&self, modifiers: impl Into<EnumSet<Key>>, triggers: impl Into<EnumSet<Key>>) -> bool {
		let triggers = triggers.into();
		(modifiers.into().union(triggers) == self.active_keys) && !self.different_keys.is_empty() && (self.different_keys.is_subset(triggers) || !self.active_keys.complement().intersection(self.different_keys).is_empty())
	}

	pub fn was_undiscovered(&self, modifiers: impl Into<EnumSet<Key>>, triggers: impl Into<EnumSet<Key>>) -> bool {
		let wanted_keys = modifiers.into().union(triggers.into());
		!wanted_keys.is_subset(self.active_keys) && wanted_keys.is_subset(self.active_keys.symmetrical_difference(self.different_keys))
	}
}