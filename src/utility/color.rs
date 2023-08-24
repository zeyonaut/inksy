// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct HSV(pub [f32; 3]);

impl HSV {
	pub fn to_srgb(self) -> SRGB {
		let Self([h, s, v]) = self;
		fn hue(h: f32) -> [f32; 3] {
			[(h * 6. - 3.).abs() - 1., 2. - (h * 6. - 2.).abs(), 2. - (h * 6. - 4.).abs()].map(|n| n.clamp(0., 1.))
		}
		SRGB(hue(h).map(|n: f32| ((n - 1.) * s + 1.) * v))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct SRGB(pub [f32; 3]);

impl SRGB {
	pub fn to_srgba8(self) -> SRGBA8 {
		let Self(srgb) = self;
		let [r8, g8, b8] = srgb.map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 });
		SRGBA8([r8, g8, b8, 0xff])
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct SRGBA8(pub [u8; 4]);

impl SRGBA8 {
	pub fn to_lrgba(self) -> LRGBA {
		let Self(srgba8) = self;
		LRGBA(srgba8.map(fast_srgb8::srgb8_to_f32))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct LRGBA(pub [f32; 4]);
