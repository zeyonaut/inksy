// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::ops::{Deref, DerefMut};

// A 'tracked' value keeps track of whether it has been modified since the last time it was read.
pub struct Tracked<T> {
	value: T,
	is_dirty: bool,
}

impl<T> Tracked<T> {
	pub fn invalidate(&mut self) {
		self.is_dirty = true;
	}

	pub fn take(self) -> T {
		self.value
	}

	pub fn read(&mut self) -> &T {
		self.is_dirty = false;
		&self.value
	}

	pub fn read_if_dirty(&mut self) -> Option<&T> {
		if self.is_dirty {
			self.is_dirty = false;
			Some(&self.value)
		} else {
			None
		}
	}

	pub fn read_if_with_is_dirty(&mut self, mut f: impl FnMut(bool) -> bool) -> Option<&T> {
		if f(self.is_dirty) {
			self.is_dirty = false;
			Some(&self.value)
		} else {
			None
		}
	}
}

impl<T: Default> Default for Tracked<T> {
	fn default() -> Self {
		Self { value: Default::default(), is_dirty: true }
	}
}

impl<T> AsRef<T> for Tracked<T> {
	fn as_ref(&self) -> &T {
		&self.value
	}
}

impl<T> AsMut<T> for Tracked<T> {
	fn as_mut(&mut self) -> &mut T {
		self.is_dirty = true;
		&mut self.value
	}
}

impl<T> Deref for Tracked<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		self.as_ref()
	}
}

impl<T> DerefMut for Tracked<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.as_mut()
	}
}

impl<T> From<T> for Tracked<T> {
	fn from(value: T) -> Self {
		Self { value, is_dirty: true }
	}
}
