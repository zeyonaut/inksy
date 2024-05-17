// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct Hsv(pub [f32; 3]);

impl Hsv {
	pub fn to_srgb(self) -> Srgb {
		let Self([h, s, v]) = self;
		fn hue(h: f32) -> [f32; 3] {
			[(h * 6. - 3.).abs() - 1., 2. - (h * 6. - 2.).abs(), 2. - (h * 6. - 4.).abs()].map(|n| n.clamp(0., 1.))
		}
		Srgb(hue(h).map(|n: f32| ((n - 1.) * s + 1.) * v))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct Srgb(pub [f32; 3]);

impl Srgb {
	pub fn to_srgb8(self) -> Srgb8 {
		let Self(srgb) = self;
		Srgb8(srgb.map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 }))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct Srgb8(pub [u8; 3]);

impl Srgb8 {
	pub fn to_hsv(self) -> Hsv {
		let (argmax, max) = self.0.iter().copied().enumerate().max_by_key(|(_, x)| *x).unwrap();
		let min = self.0.iter().copied().min().unwrap();
		Hsv(if min == max {
			[0., 0., f32::from(max) / 255.]
		} else {
			let (max, min) = (f32::from(max) / 255., f32::from(min) / 255.);
			let saturation = (max - min) / max;
			let hue = ((f32::from(2 * argmax as u8) - (max - f32::from(self.0[(argmax + 1) % 3]) / 255.) / (max - min) + (max - f32::from(self.0[(argmax + 2) % 3]) / 255.) / (max - min)) / 6.).fract();
			[hue, saturation, max]
		})
	}

	pub fn opaque(self) -> Srgba8 {
		let Self([r, g, b]) = self;
		Srgba8([r, g, b, 0xff])
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct Srgba8(pub [u8; 4]);

impl Srgba8 {
	pub fn to_lrgba(self) -> Lrgba {
		let Self(srgba8) = self;
		Lrgba(srgba8.map(fast_srgb8::srgb8_to_f32))
	}
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Index, derive_more::IndexMut)]
pub struct Lrgba(pub [f32; 4]);
