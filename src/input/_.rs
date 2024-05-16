// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod wintab;

pub mod keymap;

use enumset::{EnumSet, EnumSetType};
use winit::event::{ElementState, KeyEvent};

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
	Control,
	Shift,
	LeftArrow,
	RightArrow,
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

	pub fn process_key_event(&mut self, event: &KeyEvent) {
		self.is_fresh = true;

		let winit::keyboard::PhysicalKey::Code(keycode) = event.physical_key else { return };

		use winit::keyboard::KeyCode;
		use Key::*;
		let key = match keycode {
			KeyCode::Digit1 => K0,
			KeyCode::Digit2 => K1,
			KeyCode::Digit3 => K2,
			KeyCode::Digit4 => K3,
			KeyCode::Digit5 => K4,
			KeyCode::Digit6 => K5,
			KeyCode::Digit7 => K6,
			KeyCode::Digit8 => K7,
			KeyCode::Digit9 => K8,
			KeyCode::Digit0 => K9,
			KeyCode::KeyA => A,
			KeyCode::KeyB => B,
			KeyCode::KeyC => C,
			KeyCode::KeyD => D,
			KeyCode::KeyE => E,
			KeyCode::KeyF => F,
			KeyCode::KeyG => G,
			KeyCode::KeyH => H,
			KeyCode::KeyI => I,
			KeyCode::KeyJ => J,
			KeyCode::KeyK => K,
			KeyCode::KeyL => L,
			KeyCode::KeyM => M,
			KeyCode::KeyN => N,
			KeyCode::KeyO => O,
			KeyCode::KeyP => P,
			KeyCode::KeyQ => Q,
			KeyCode::KeyR => R,
			KeyCode::KeyS => S,
			KeyCode::KeyT => T,
			KeyCode::KeyU => U,
			KeyCode::KeyV => V,
			KeyCode::KeyW => W,
			KeyCode::KeyX => X,
			KeyCode::KeyY => Y,
			KeyCode::KeyZ => Z,
			KeyCode::Backspace => Backspace,
			KeyCode::Escape => Escape,
			KeyCode::Space => Space,
			KeyCode::Tab => Tab,
			KeyCode::ShiftLeft | KeyCode::ShiftRight => Shift,
			KeyCode::ControlLeft | KeyCode::ControlRight => Control,
			KeyCode::ArrowLeft => LeftArrow,
			KeyCode::ArrowRight => RightArrow,
			_ => return,
		};

		let is_active = event.state == ElementState::Pressed;
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
}
