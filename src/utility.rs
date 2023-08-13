// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub fn hsv_to_srgb(h: f32, s: f32, v: f32) -> [f32; 3] {
	fn hue(h: f32) -> [f32; 3] {
		[(h * 6. - 3.).abs() - 1., 2. - (h * 6. - 2.).abs(), 2. - (h * 6. - 4.).abs()].map(|n| n.clamp(0., 1.))
	}
	hue(h).map(|n: f32| ((n - 1.) * s + 1.) * v)
}

pub fn hsv_to_srgba8(hsv: [f32; 3]) -> [u8; 4] {
	let [h, s, v] = hsv;
	let [r, g, b] = hsv_to_srgb(h, s, v).map(|n| if n >= 1.0 { 255 } else { (n * 256.) as u8 });
	[r, g, b, 0xff]
}
