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
	pub fn to_srgb8(self) -> SRGB8 {
		let Self(srgb) = self;
		SRGB8(srgb.map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 }))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct SRGB8(pub [u8; 3]);

impl SRGB8 {
	pub fn to_hsv(self) -> HSV {
		let (argmax, max) = self.0.iter().copied().enumerate().max_by_key(|(_, x)| *x).unwrap();
		let min = self.0.iter().copied().min().unwrap();
		HSV(if min == max {
			[0., 0., f32::from(max) / 255.]
		} else {
			let (max, min) = (f32::from(max) / 255., f32::from(min) / 255.);
			let saturation = (max - min) / max;
			let hue = ((f32::from(2 * argmax as u8) - (max - f32::from(self.0[(argmax + 1) % 3]) / 255.) / (max - min) + (max - f32::from(self.0[(argmax + 2) % 3]) / 255.) / (max - min)) / 6.).fract();
			[hue, saturation, max]
		})
	}

	pub fn opaque(self) -> SRGBA8 {
		let Self([r, g, b]) = self;
		SRGBA8([r, g, b, 0xff])
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
