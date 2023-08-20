// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
	fs::File,
	io::{Read, Write, BufWriter, BufReader},
	path::{Path, PathBuf},
};

use crate::{
	canvas::{Canvas, Image, Object, Point, Stroke, View},
	pixel::{Vex, Vx, Zoom},
	render::Renderer,
	utility::{HSV, SRGBA8},
};

const MAGIC_NUMBERS: [u8; 8] = [b'I', b'N', b'K', b'S', b'Y', 0, 0, 0];

pub fn save_canvas_to_file(canvas: &Canvas, renderer: &Renderer) -> Option<()> {
	let file_path = canvas.file_path.as_ref()?;
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
	}

	Some(())
}

fn save_canvas_to_file_inner(canvas: &Canvas, renderer: &Renderer, file_path: &Path) -> Option<()> {
	let mut file = BufWriter::new(File::create(file_path).ok()?);

	file.write_all(&MAGIC_NUMBERS).ok()?;
	file.write_all(&0u64.to_le_bytes()).ok()?;

	let background_color: [f32; 3] = canvas.background_color.0;
	let position: [f32; 2] = [canvas.view.position[0].0, canvas.view.position[1].0];
	let tilt: f32 = canvas.view.tilt;
	let zoom: f32 = canvas.view.zoom.0;
	let stroke_count: u64 = u64::try_from(canvas.strokes.len()).ok()?;
	let image_count: u64 = u64::try_from(canvas.images.len()).ok()?;
	let texture_count: u64 = u64::try_from(canvas.textures.len()).ok()?;

	file.write_all(&background_color[0].to_le_bytes()).ok()?;
	file.write_all(&background_color[1].to_le_bytes()).ok()?;
	file.write_all(&background_color[2].to_le_bytes()).ok()?;
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
		let color: [u8; 4] = stroke.object.color.0;
		let stroke_radius: f32 = stroke.object.stroke_radius.0;
		let point_count: u64 = u64::try_from(stroke.object.points.len()).ok()?;

		file.write_all(&position[0].to_le_bytes()).ok()?;
		file.write_all(&position[1].to_le_bytes()).ok()?;
		file.write_all(&orientation.to_le_bytes()).ok()?;
		file.write_all(&dilation.to_le_bytes()).ok()?;
		file.write_all(&color).ok()?;
		file.write_all(&stroke_radius.to_le_bytes()).ok()?;
		file.write_all(&point_count.to_le_bytes()).ok()?;

		for point in stroke.object.points.iter() {
			let position: [f32; 2] = [point.position[0].0, point.position[1].0];
			let pressure: f32 = point.pressure;

			file.write_all(&position[0].to_le_bytes()).ok()?;
			file.write_all(&position[1].to_le_bytes()).ok()?;
			file.write_all(&pressure.to_le_bytes()).ok()?;
		}
	}

	for image in canvas.images.iter() {
		let position: [f32; 2] = [image.position[0].0, image.position[1].0];
		let orientation: f32 = image.orientation;
		let dilation: f32 = image.dilation;
		let texture_index: u64 = u64::try_from(image.object.texture_index).ok()?;
		let dimensions: [f32; 2] = [image.object.dimensions[0].0, image.object.dimensions[1].0];

		file.write_all(&position[0].to_le_bytes()).ok()?;
		file.write_all(&position[1].to_le_bytes()).ok()?;
		file.write_all(&orientation.to_le_bytes()).ok()?;
		file.write_all(&dilation.to_le_bytes()).ok()?;
		file.write_all(&texture_index.to_le_bytes()).ok()?;
		file.write_all(&dimensions[0].to_le_bytes()).ok()?;
		file.write_all(&dimensions[1].to_le_bytes()).ok()?;
	}

	for (texture_index, texture) in canvas.textures.iter().enumerate() {
		let dimensions: [u32; 2] = [texture.extent.width, texture.extent.height];

		file.write_all(&dimensions[0].to_le_bytes()).ok()?;
		file.write_all(&dimensions[1].to_le_bytes()).ok()?;

		let (buffer, bytes_per_row) = renderer.fetch_texture(canvas.textures.get(texture_index)?)?;

		// Map the buffer before calling get_mapped_range.
		let buffer_slice = buffer.slice(..);
		let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
		buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
			tx.send(result).unwrap();
		});
		renderer.device.poll(wgpu::Maintain::Wait);
		pollster::block_on(rx.receive()).unwrap().unwrap();

		for chunk in buffer.slice(..).get_mapped_range().chunks(bytes_per_row) {
			file.write_all(&chunk[..texture.extent.width as usize * 4]).ok()?;
		}

		buffer.unmap();
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
	if discriminator != 0 {
		return None;
	}

	let background_color = read_f32s::<3>(&mut file)?;
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

		strokes.push(Object {
			object: Stroke {
				color: SRGBA8(color),
				stroke_radius: Vx(stroke_radius),
				points,
			},
			position: Vex(position.map(Vx)),
			orientation,
			dilation,
			is_selected: false,
		});
	}

	let mut images = Vec::with_capacity((image_count as usize).min(128));
	for _ in 0..image_count {
		let position = read_f32s::<2>(&mut file)?;
		let [orientation, dilation] = read_f32s(&mut file)?;
		let [texture_index] = read_u64s(&mut file)?;
		let dimensions = read_f32s::<2>(&mut file)?;

		images.push(Object {
			object: Image {
				texture_index: usize::try_from(texture_index).ok()?,
				dimensions: Vex(dimensions.map(Vx)),
			},
			position: Vex(position.map(Vx)),
			orientation,
			dilation,
			is_selected: false,
		});
	}

	let mut textures = Vec::with_capacity((texture_count as usize).min(128));
	for _ in 0..texture_count {
		let [width, height] = read_u32s(&mut file)?;
		let mut buffer = vec![0; width as usize * 4 * height as usize];
		file.read_exact(&mut buffer).ok()?;
		textures.push(renderer.create_texture([width, height], buffer));
	}

	Some(Canvas::from_file(
		file_path,
		HSV(background_color),
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
	for i in 0..N {
		let mut buffer = [0; 8];
		file.read_exact(&mut buffer).ok()?;
		array[i] = u64::from_le_bytes(buffer);
	}
	Some(array)
}

fn read_u32s<const N: usize>(file: &mut impl Read) -> Option<[u32; N]> {
	let mut array = [0; N];
	for i in 0..N {
		let mut buffer = [0; 4];
		file.read_exact(&mut buffer).ok()?;
		array[i] = u32::from_le_bytes(buffer);
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
	for i in 0..N {
		let mut buffer = [0; 4];
		file.read_exact(&mut buffer).ok()?;
		array[i] = f32::from_le_bytes(buffer);
	}
	Some(array)
}
