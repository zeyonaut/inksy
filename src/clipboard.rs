// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{io::Cursor, num::NonZeroU32};

use clipboard_win::{
	formats::{Bitmap, CF_BITMAP},
	Getter,
};
use embedded_graphics::pixelcolor::RgbColor;

pub struct Clipboard {
	custom_format: NonZeroU32,
	png_format: NonZeroU32,
}

const CLIPBOARD_FORMAT_NAME: &str = crate::APP_NAME_LOWERCASE;

pub enum ClipboardData {
	Custom,
	Image { dimensions: [u32; 2], data: Vec<u8> },
}

impl Clipboard {
	pub fn new() -> Option<Self> {
		let custom_format = clipboard_win::register_format(CLIPBOARD_FORMAT_NAME)?;
		let png_format = clipboard_win::register_format("PNG")?;
		Some(Self { custom_format, png_format })
	}

	pub fn write(&self, content: ClipboardData) -> Option<()> {
		match content {
			ClipboardData::Custom => {
				clipboard_win::raw::open().ok()?;
				clipboard_win::raw::set(self.custom_format.into(), &[0]).ok()?;
				clipboard_win::raw::close().ok()?;
			},
			ClipboardData::Image { .. } => {},
		}
		Some(())
	}

	pub fn read(&self) -> Option<ClipboardData> {
		if clipboard_win::is_format_avail(self.custom_format.into()) {
			return Some(ClipboardData::Custom);
		} else if clipboard_win::is_format_avail(self.png_format.into()) {
			let mut data = Vec::new();
			clipboard_win::raw::open().ok()?;
			clipboard_win::raw::get_vec(self.png_format.into(), &mut data).ok()?;
			clipboard_win::raw::close().ok()?;

			let png_decoder = png::Decoder::new(Cursor::new(data));
			let mut png_reader = png_decoder.read_info().ok()?;
			let mut image_buffer = vec![0; png_reader.output_buffer_size()];
			let width = png_reader.info().width;
			let height = png_reader.info().height;
			png_reader.next_frame(&mut image_buffer).ok()?;

			return Some(ClipboardData::Image { dimensions: [width, height], data: image_buffer });
		} else if clipboard_win::is_format_avail(CF_BITMAP) {
			let mut data = Vec::new();
			clipboard_win::raw::open().ok()?;
			Bitmap.read_clipboard(&mut data).ok()?;
			clipboard_win::raw::close().ok()?;

			let bmp = tinybmp::Bmp::<embedded_graphics::pixelcolor::Rgb888>::from_slice(&data).ok()?;
			let width = bmp.as_raw().header().image_size.width;
			let height = bmp.as_raw().header().image_size.height;
			let mut data = Vec::<u8>::with_capacity(width as usize * height as usize * 4);
			let bmp_iterator = bmp.as_raw().image_data().chunks_exact(width as usize * 4);

			// If the bitmap data is upside down, we interpret it in reverse by row.
			if bmp.as_raw().header().row_order == tinybmp::RowOrder::BottomUp {
				for row in bmp_iterator.rev() {
					data.extend(row)
				}
			} else {
				for row in bmp_iterator {
					data.extend(row)
				}
			}

			// We premultiply RGB by transparency to prevent the 'white border' effect.
			// For future reference: this might not be a good idea for the general case, but it works nicely
			// for Windows' legacy snipping tool.
			for (chunk, pixel) in data.chunks_exact_mut(4).zip(bmp.pixels()) {
				chunk[0] = (pixel.1.r() as f64 * chunk[3] as f64 / 255.) as u8;
				chunk[1] = (pixel.1.g() as f64 * chunk[3] as f64 / 255.) as u8;
				chunk[2] = (pixel.1.b() as f64 * chunk[3] as f64 / 255.) as u8;
			}

			return Some(ClipboardData::Image { dimensions: [width, height], data });
		}
		None
	}
}
