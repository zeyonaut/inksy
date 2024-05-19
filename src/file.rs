// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
	fs::File,
	io::{BufReader, BufWriter, Cursor, Read, Write},
	num::NonZero,
	path::{Path, PathBuf},
};

use crate::{
	canvas::{Canvas, Image, Point, Stroke, View},
	render::Renderer,
	utility::{Srgb8, Srgba8, Tracked, Vex, Vx, Zoom},
};

const MAGIC_NUMBERS: [u8; 8] = [b'I', b'N', b'K', b'S', b'Y', 0, 0, 0];

pub fn save_canvas_to_file(canvas: &Canvas, renderer: &Renderer, file_path: &Path) -> Option<()> {
	let old_file = if file_path.exists() {
		let mut buffer = Vec::new();
		let mut file = File::open(file_path).ok()?;
		file.read_to_end(&mut buffer).ok()?;
		Some(buffer)
	} else {
		None
	};

	if save_canvas_to_file_inner(canvas, renderer, file_path).is_none() {
		if let Some(old_file) = old_file {
			let mut file = File::create(file_path).ok()?;
			// TODO: Return a descriptive error saying that we messed up. Badly.
			file.write_all(&old_file).ok()?;
		}
		return None;
	}

	Some(())
}

fn save_canvas_to_file_inner(canvas: &Canvas, renderer: &Renderer, file_path: &Path) -> Option<()> {
	let mut file = BufWriter::new(File::create(file_path).ok()?);

	file.write_all(&MAGIC_NUMBERS).ok()?;
	file.write_all(&1u64.to_le_bytes()).ok()?;

	let background_color: [u8; 3] = canvas.background_color.0;
	let stroke_color: [u8; 3] = canvas.stroke_color.to_srgb().to_srgb8().0;
	let stroke_radius: f32 = canvas.stroke_radius.0;
	let position: [f32; 2] = [canvas.view.position[0].0, canvas.view.position[1].0];
	let tilt: f32 = canvas.view.tilt;
	let zoom: f32 = canvas.view.zoom.0;
	let stroke_count: u64 = u64::try_from(canvas.strokes.len()).ok()?;
	let image_count: u64 = u64::try_from(canvas.images.len()).ok()?;
	let texture_count: u64 = u64::try_from(canvas.textures.len()).ok()?;

	file.write_all(&background_color).ok()?;
	file.write_all(&stroke_color).ok()?;
	file.write_all(&stroke_radius.to_le_bytes()).ok()?;
	file.write_all(&position[0].to_le_bytes()).ok()?;
	file.write_all(&position[1].to_le_bytes()).ok()?;
	file.write_all(&tilt.to_le_bytes()).ok()?;
	file.write_all(&zoom.to_le_bytes()).ok()?;
	file.write_all(&stroke_count.to_le_bytes()).ok()?;
	file.write_all(&image_count.to_le_bytes()).ok()?;
	file.write_all(&texture_count.to_le_bytes()).ok()?;

	for stroke in canvas.strokes.iter() {
		let position: [f32; 2] = [stroke.position[0].0, stroke.position[1].0];
		let orientation: f32 = stroke.orientation;
		let dilation: f32 = stroke.dilation;
		let color: [u8; 4] = stroke.color.0;
		let stroke_radius: f32 = stroke.stroke_radius.0;
		let point_count: u64 = u64::try_from(stroke.points.len()).ok()?;

		file.write_all(&position[0].to_le_bytes()).ok()?;
		file.write_all(&position[1].to_le_bytes()).ok()?;
		file.write_all(&orientation.to_le_bytes()).ok()?;
		file.write_all(&dilation.to_le_bytes()).ok()?;
		file.write_all(&color).ok()?;
		file.write_all(&stroke_radius.to_le_bytes()).ok()?;
		file.write_all(&point_count.to_le_bytes()).ok()?;

		for point in stroke.points.iter() {
			let position: [f32; 2] = [point.position[0].0, point.position[1].0];
			let pressure: f32 = point.pressure;

			file.write_all(&position[0].to_le_bytes()).ok()?;
			file.write_all(&position[1].to_le_bytes()).ok()?;
			file.write_all(&pressure.to_le_bytes()).ok()?;
		}
	}

	let mut is_texture_referenced_array = vec![false; canvas.textures.len()];

	for image in canvas.images.iter() {
		let position: [f32; 2] = [image.position[0].0, image.position[1].0];
		let orientation: f32 = image.orientation;
		let dilation: f32 = image.dilation;
		is_texture_referenced_array[image.texture_index] = true;
		let texture_index: u64 = u64::try_from(image.texture_index).ok()?;
		let dimensions: [f32; 2] = [image.dimensions[0].0, image.dimensions[1].0];

		file.write_all(&position[0].to_le_bytes()).ok()?;
		file.write_all(&position[1].to_le_bytes()).ok()?;
		file.write_all(&orientation.to_le_bytes()).ok()?;
		file.write_all(&dilation.to_le_bytes()).ok()?;
		file.write_all(&texture_index.to_le_bytes()).ok()?;
		file.write_all(&dimensions[0].to_le_bytes()).ok()?;
		file.write_all(&dimensions[1].to_le_bytes()).ok()?;
	}

	let mut data = vec![];
	let mut compressed_data = vec![];
	for ((texture_index, texture), is_texture_referenced) in canvas.textures.iter().enumerate().zip(is_texture_referenced_array) {
		if is_texture_referenced {
			compressed_data.clear();
			data.reserve(texture.extent.width as usize * texture.extent.height as usize * 4);
			data.clear();

			// Set up the encoder.
			let mut encoder = png::Encoder::new(&mut compressed_data, texture.extent.width, texture.extent.height);
			encoder.set_color(png::ColorType::Rgba);
			encoder.set_depth(png::BitDepth::Eight);
			let mut writer = encoder.write_header().unwrap();

			// Fetch and map texture from device.
			let (buffer, bytes_per_row) = renderer.fetch_texture(canvas.textures.get(texture_index)?)?;
			let buffer_slice = buffer.slice(..);
			let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
			buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
				tx.send(result).unwrap();
			});
			renderer.device.poll(wgpu::Maintain::Wait);
			pollster::block_on(rx.receive()).unwrap().unwrap();

			// Read the texture row-by-row (each an initial slice of a mapped chunk).
			for chunk in buffer.slice(..).get_mapped_range().chunks(bytes_per_row) {
				data.extend(&chunk[..texture.extent.width as usize * 4])
			}

			// Unmap the texture buffer.
			buffer.unmap();
			
			writer.write_image_data(&data).ok()?;
			writer.finish().ok()?;

			let texture_flag: u64 = compressed_data.len() as u64;
			
			file.write_all(&texture_flag.to_le_bytes()).ok()?;
			file.write_all(&compressed_data).ok()?;
		} else {
			let texture_flag: u64 = 0;

			file.write_all(&texture_flag.to_le_bytes()).ok()?;
		}
	}

	Some(())
}

pub fn load_canvas_from_file(renderer: &mut Renderer, file_path: PathBuf) -> Option<Canvas> {
	let mut file = BufReader::new(File::open(file_path.clone()).ok()?);

	let mut magic_numbers = [0; 8];
	file.read_exact(&mut magic_numbers).ok()?;
	if magic_numbers != MAGIC_NUMBERS {
		return None;
	}

	let [discriminator] = read_u64s(&mut file)?;
	if !(discriminator == 0 || discriminator == 1) {
		return None;
	}

	let background_color = read_u8s::<3>(&mut file)?;
	let stroke_color = read_u8s::<3>(&mut file)?;
	let [stroke_radius] = read_f32s::<1>(&mut file)?;
	let position = read_f32s::<2>(&mut file)?;
	let [tilt, zoom] = read_f32s(&mut file)?;
	let [stroke_count, image_count, texture_count] = read_u64s(&mut file)?;

	let mut strokes = Vec::with_capacity((stroke_count as usize).min(2048));
	for _ in 0..stroke_count {
		let position = read_f32s::<2>(&mut file)?;
		let [orientation, dilation] = read_f32s(&mut file)?;
		let color = read_u8s::<4>(&mut file)?;
		let [stroke_radius] = read_f32s(&mut file)?;
		let [point_count] = read_u64s(&mut file)?;

		let mut points = Vec::with_capacity((point_count as usize).min(2048));
		for _ in 0..point_count {
			let position = read_f32s::<2>(&mut file)?;
			let [pressure] = read_f32s(&mut file)?;

			points.push(Point { position: Vex(position.map(Vx)), pressure })
		}

		strokes.push(Stroke::new(Srgba8(color), Vx(stroke_radius), points, Vex(position.map(Vx)), orientation, dilation).into());
	}

	let mut images = Vec::with_capacity((image_count as usize).min(128));
	for _ in 0..image_count {
		let position = read_f32s::<2>(&mut file)?;
		let [orientation, dilation] = read_f32s(&mut file)?;
		let [texture_index] = read_u64s(&mut file)?;
		let dimensions = read_f32s::<2>(&mut file)?;

		images.push(
			Image {
				texture_index: usize::try_from(texture_index).ok()?,
				dimensions: Vex(dimensions.map(Vx)),
				position: Vex(position.map(Vx)),
				orientation,
				dilation,
				is_selected: false,
			}
			.into(),
		);
	}

	let mut revised_texture_index_array = Vec::with_capacity(texture_count as usize);
	let mut revised_texture_index = 0;
	let mut textures = Vec::with_capacity((texture_count as usize).min(128));
	let mut compressed_data = vec![];
	for _ in 0..texture_count {
		revised_texture_index_array.push(revised_texture_index);
		match discriminator {
			0 => {
				let [width, height] = read_u32s(&mut file)?;
				// If either dimension are zero, no texture was saved.
				if let [Ok(width), Ok(height)] = [width, height].map(NonZero::try_from) {
					let mut buffer = vec![0; width.get() as usize * 4 * height.get() as usize];
					file.read_exact(&mut buffer).ok()?;
					textures.push(renderer.create_texture([width, height], buffer));
					revised_texture_index += 1;
				}
			},
			1 => {
				let [texture_flag] = read_u64s(&mut file)?;
				match texture_flag {
					0 => {},
					i => {
						compressed_data.clear();
						compressed_data.resize(i as usize, 0);
						file.read_exact(&mut compressed_data).ok()?;

						let png_decoder = png::Decoder::new(Cursor::new(&compressed_data));
						let mut png_reader = png_decoder.read_info().ok()?;
						let mut buffer = vec![0; png_reader.output_buffer_size()];
						let width = png_reader.info().width;
						let height = png_reader.info().height;
						png_reader.next_frame(&mut buffer).ok()?;
						png_reader.finish().ok()?;

						textures.push(renderer.create_texture([NonZero::new(width)?, NonZero::new(height)?], buffer));
						revised_texture_index += 1;
					},
				}
			},
			_ => return None,
		}
	}

	// Rebase the image texture indices.
	for image in images.iter_mut().map(Tracked::as_mut) {
		image.texture_index = revised_texture_index_array[image.texture_index];
	}

	Some(Canvas::from_file(
		file_path,
		Srgb8(background_color),
		Srgb8(stroke_color),
		Vx(stroke_radius),
		View {
			position: Vex(position.map(Vx)),
			tilt,
			zoom: Zoom(zoom),
		},
		images,
		strokes,
		textures,
	))
}

fn read_u64s<const N: usize>(file: &mut impl Read) -> Option<[u64; N]> {
	let mut array = [0; N];
	for element in &mut array {
		let mut buffer = [0; 8];
		file.read_exact(&mut buffer).ok()?;
		*element = u64::from_le_bytes(buffer);
	}
	Some(array)
}

fn read_u32s<const N: usize>(file: &mut impl Read) -> Option<[u32; N]> {
	let mut array = [0; N];
	for element in &mut array {
		let mut buffer = [0; 4];
		file.read_exact(&mut buffer).ok()?;
		*element = u32::from_le_bytes(buffer);
	}
	Some(array)
}

fn read_u8s<const N: usize>(file: &mut impl Read) -> Option<[u8; N]> {
	let mut buffer = [0; N];
	file.read_exact(&mut buffer).ok()?;
	Some(buffer)
}

fn read_f32s<const N: usize>(file: &mut impl Read) -> Option<[f32; N]> {
	let mut array = [0.; N];
	for element in &mut array {
		let mut buffer = [0; 4];
		file.read_exact(&mut buffer).ok()?;
		*element = f32::from_le_bytes(buffer);
	}
	Some(array)
}
