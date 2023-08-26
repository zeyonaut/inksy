// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fs::File;

use kdl::{KdlDocument, KdlValue};

use crate::utility::{Vx, SRGB8};

pub struct Config {
	pub default_canvas_color: SRGB8,
	pub default_stroke_color: SRGB8,
	pub default_stroke_radius: Vx,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			default_canvas_color: SRGB8([0x12, 0x12, 0x12]),
			default_stroke_color: SRGB8([0xff, 0xff, 0xff]),
			default_stroke_radius: Vx(4.),
		}
	}
}

impl Config {
	pub fn load() -> Option<Self> {
		let mut inksy_config_file_path = dirs::config_dir()?;
		inksy_config_file_path.push("inksy");
		if !inksy_config_file_path.exists() {
			std::fs::create_dir(inksy_config_file_path.clone()).ok()?;
		}
		inksy_config_file_path.push("inksy.kdl");
		if !inksy_config_file_path.exists() {
			File::create(inksy_config_file_path).ok()?;
			return None;
		}

		let inksy_config_file_data = std::fs::read_to_string(inksy_config_file_path).ok()?;

		let inksy_config_document = inksy_config_file_data.parse::<KdlDocument>().ok()?;

		let default = Self::default();

		let default_canvas_color = parse_kdl_integer_array(inksy_config_document.get_args("default-canvas-color")).map(SRGB8).unwrap_or(default.default_canvas_color);
		let default_stroke_color = parse_kdl_integer_array(inksy_config_document.get_args("default-stroke-color")).map(SRGB8).unwrap_or(default.default_stroke_color);
		let default_stroke_radius = parse_kdl_f64(inksy_config_document.get_args("default-stroke-radius")).map(|x| Vx(x as _)).unwrap_or(default.default_stroke_radius);
		Some(Config {
			default_canvas_color,
			default_stroke_color,
			default_stroke_radius,
		})
	}
}

fn parse_kdl_f64<'a>(values: impl AsRef<[&'a KdlValue]>) -> Option<f64> {
	let [n] = <[_; 1]>::try_from(values.as_ref()).ok()?.try_map(KdlValue::as_f64)?;
	Some(n)
}

fn parse_kdl_integer_array<'a, T: TryFrom<i64>, const N: usize>(values: impl AsRef<[&'a KdlValue]>) -> Option<[T; N]> {
	<[_; N]>::try_from(values.as_ref()).ok()?.try_map(KdlValue::as_i64)?.try_map(T::try_from).ok()
}
