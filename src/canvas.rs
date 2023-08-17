// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use fast_srgb8::srgb8_to_f32;

use crate::{
	pixel::{Vex, Vx, Vx2, Zero},
	render::{DrawCommand, Vertex},
};

#[derive(Clone)]
pub struct Point {
	position: Vex<2, Vx>,
	pressure: f32,
}

#[derive(Clone)]
pub struct Image {
	pub texture_index: usize,
	pub position: Vex<2, Vx>,
	pub dimensions: Vex<2, Vx>,
}

#[derive(Clone)]
pub struct Stroke {
	pub origin: Vex<2, Vx>,
	pub color: [u8; 4],
	pub points: Vec<Point>,
	pub is_selected: bool,
	pub max_pressure: f32,
}

const STROKE_RADIUS: Vx = Vx(4.);

impl Stroke {
	pub fn new(origin: Vex<2, Vx>, color: [u8; 4]) -> Self {
		Self {
			origin,
			color,
			points: Vec::new(),
			is_selected: false,
			max_pressure: 0.,
		}
	}

	pub fn add_point(&mut self, position: Vex<2, Vx>, pressure: f32) {
		let threshold = if self.points.len() < 2 {
			(self.max_pressure.max(pressure) * STROKE_RADIUS).max(Vx(1.))
		} else {
			self.max_pressure.max(pressure) * STROKE_RADIUS.min(Vx(1.))
		};
		if self.points.last().map_or(true, |point| (position - (self.origin + point.position)).norm() > threshold) {
			self.points.push(Point { position: position - self.origin, pressure });
			self.max_pressure = pressure;
		} else {
			self.max_pressure = self.max_pressure.max(pressure);
		}
	}
}

enum Retraction {
	CommitStrokes(usize),
	PasteImage,
	DeleteStrokes { antitone_index_stroke_pairs: Vec<(usize, Stroke)> },
	RecolorStrokes { index_color_pairs: Vec<(usize, [u8; 4])>, new_color: [u8; 4] },
	TranslateStrokes { indices: Vec<usize>, vector: Vex<2, Vx> },
}

pub enum Operation {
	CommitStrokes { strokes: Vec<Stroke> },
	PasteImage { image: Image },
	DeleteStrokes { monotone_indices: Vec<usize> },
	RecolorStrokes { indices: Vec<usize>, new_color: [u8; 4] },
	TranslateStrokes { indices: Vec<usize>, vector: Vex<2, Vx> },
}

pub struct Canvas {
	images: Vec<Image>,
	strokes: Vec<Stroke>,
	retractions: Vec<Retraction>,
	operations: Vec<Operation>,
}

impl Canvas {
	pub fn new() -> Self {
		Self {
			images: Vec::new(),
			strokes: Vec::new(),
			retractions: Vec::new(),
			operations: Vec::new(),
		}
	}

	pub fn strokes(&self) -> &[Stroke] {
		self.strokes.as_ref()
	}

	pub fn redo(&mut self) {
		if let Some(operation) = self.operations.pop() {
			use Operation::*;
			self.retractions.push(match operation {
				CommitStrokes { mut strokes } => {
					let length = strokes.len();
					self.strokes.append(&mut strokes);

					Retraction::CommitStrokes(length)
				},
				PasteImage { image } => {
					self.images.push(image);

					Retraction::PasteImage
				},
				DeleteStrokes { monotone_indices } => {
					let mut antitone_index_stroke_pairs = Vec::with_capacity(monotone_indices.len());

					for index in monotone_indices.iter().rev().copied() {
						debug_assert!(index < self.strokes.len());
						let stroke = self.strokes.remove(index);
						antitone_index_stroke_pairs.push((index, stroke));
					}

					Retraction::DeleteStrokes { antitone_index_stroke_pairs }
				},
				RecolorStrokes { indices, new_color } => {
					let mut index_color_pairs = Vec::with_capacity(indices.len());

					for index in indices {
						if let Some(stroke) = self.strokes.get_mut(index) {
							index_color_pairs.push((index, stroke.color));
							stroke.color = new_color;
						}
					}

					Retraction::RecolorStrokes { index_color_pairs, new_color }
				},
				TranslateStrokes { indices, vector } => {
					for index in indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index) {
							stroke.origin = stroke.origin + vector;
						}
					}

					Retraction::TranslateStrokes { indices, vector }
				},
			});
		}
	}

	pub fn undo(&mut self) {
		if let Some(operation) = self.retractions.pop() {
			use Retraction::*;
			self.operations.push(match operation {
				CommitStrokes(length) => {
					let mut strokes = Vec::with_capacity(length);

					debug_assert!(length <= self.strokes.len());
					for _ in 0..length {
						strokes.push(self.strokes.pop().unwrap());
					}

					Operation::CommitStrokes { strokes }
				},
				PasteImage => Operation::PasteImage { image: self.images.pop().unwrap() },
				DeleteStrokes { antitone_index_stroke_pairs } => {
					let mut monotone_indices = Vec::with_capacity(antitone_index_stroke_pairs.len());

					for (index, stroke) in antitone_index_stroke_pairs.into_iter().rev() {
						debug_assert!(index <= self.strokes.len());
						self.strokes.insert(index, stroke);
						monotone_indices.push(index);
					}

					Operation::DeleteStrokes { monotone_indices }
				},
				RecolorStrokes { index_color_pairs, new_color } => {
					let mut indices = Vec::with_capacity(index_color_pairs.len());

					for (index, old_color) in index_color_pairs.into_iter() {
						if let Some(stroke) = self.strokes.get_mut(index) {
							stroke.color = old_color;
						}

						indices.push(index);
					}

					Operation::RecolorStrokes { indices, new_color }
				},
				TranslateStrokes { indices, vector } => {
					for index in indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index) {
							stroke.origin = stroke.origin - vector;
						}
					}

					Operation::TranslateStrokes { indices, vector }
				},
			});
		}
	}

	pub fn perform_operation(&mut self, operation: Operation) {
		self.operations.clear();
		self.operations.push(operation);
		self.redo();
	}

	pub fn select(&mut self, min: Vex<2, Vx>, max: Vex<2, Vx>, tilt: f32, screen_center: Vex<2, Vx>, should_aggregate: bool) {
		'strokes: for stroke in self.strokes.iter_mut() {
			if should_aggregate {
				for point in stroke.points.iter() {
					let point_position = (stroke.origin + point.position - screen_center).rotate(tilt);
					if point_position[0] >= min[0] && point_position[1] >= min[1] && point_position[0] <= max[0] && point_position[1] <= max[1] {
						stroke.is_selected = !stroke.is_selected;
						continue 'strokes;
					}
				}
			} else {
				for point in stroke.points.iter() {
					let point_position = (stroke.origin + point.position - screen_center).rotate(tilt);
					if point_position[0] >= min[0] && point_position[1] >= min[1] && point_position[0] <= max[0] && point_position[1] <= max[1] {
						stroke.is_selected = true;
						continue 'strokes;
					}
				}
				stroke.is_selected = false;
			}
		}
	}

	pub fn select_all(&mut self, is_selected: bool) {
		for stroke in self.strokes.iter_mut() {
			stroke.is_selected = is_selected;
		}
	}

	pub fn bake(&self, draw_commands: &mut Vec<DrawCommand>, current_stroke: Option<&Stroke>, selection_offset: Option<Vex<2, Vx>>) {
		for image in self.images.iter() {
			draw_commands.push(DrawCommand::Texture {
				position: image.position,
				dimensions: image.dimensions,
				index: image.texture_index,
			});
		}

		let mut vertices = vec![];
		let mut indices = vec![];
		const BORDER_RADIUS: Vx = Vx(6.);
		const BORDER_COLOR: [u8; 4] = [0x28, 0xc2, 0xff, 0xff];

		for (stroke, is_current) in self.strokes.iter().zip(std::iter::repeat(false)).chain(current_stroke.map(|stroke| (stroke, true))) {
			let stroke_offset = if stroke.is_selected { selection_offset.unwrap_or(Vex::ZERO) } else { Vex::ZERO };

			if stroke.points.len() == 1 && !is_current {
				let point = stroke.points.first().unwrap();
				fn heptagonal_vertices() -> [Vex<2, f32>; 8] {
					use std::f32::consts::PI;
					let i = Vex([1., 0.]);
					[
						Vex::ZERO,
						i,
						i.rotate(2. * PI * 1. / 7.),
						i.rotate(2. * PI * 2. / 7.),
						i.rotate(2. * PI * 3. / 7.),
						i.rotate(2. * PI * 4. / 7.),
						i.rotate(2. * PI * 5. / 7.),
						i.rotate(2. * PI * 6. / 7.),
					]
				}

				if stroke.is_selected {
					let stroke_index = u32::try_from(vertices.len()).unwrap();
					let heptagonal_vertices = heptagonal_vertices().map(|v| stroke.origin + stroke_offset + point.position + v * (stroke.max_pressure * STROKE_RADIUS + BORDER_RADIUS));
					vertices.extend(heptagonal_vertices.map(|position| Vertex {
						position: [position[0], position[1]],
						color: BORDER_COLOR.map(srgb8_to_f32),
					}));
					indices.extend([0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 7, 0, 7, 1].map(|n| stroke_index + n));
				}
				let stroke_index = u32::try_from(vertices.len()).unwrap();
				let heptagonal_vertices = heptagonal_vertices().map(|v| stroke.origin + stroke_offset + point.position + v * stroke.max_pressure * STROKE_RADIUS);
				vertices.extend(heptagonal_vertices.map(|position| Vertex {
					position: [position[0], position[1]],
					color: stroke.color.map(srgb8_to_f32),
				}));
				indices.extend([0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 7, 0, 7, 1].map(|n| stroke_index + n));
			} else {
				let perpendiculars = stroke
					.points
					.array_windows::<2>()
					.map(|[a, b]| {
						let forward = b.position - a.position;
						Vex([forward[1], -forward[0]]).normalized() * STROKE_RADIUS
					})
					.collect::<Vec<_>>();

				if stroke.is_selected {
					let stroke_index = u32::try_from(vertices.len()).unwrap();

					let mut positions = vec![];
					let border_perpendiculars = stroke
						.points
						.array_windows::<2>()
						.map(|[a, b]| {
							let forward = b.position - a.position;
							Vex([forward[1], -forward[0]]).normalized() * BORDER_RADIUS
						})
						.collect::<Vec<_>>();

					for ([a, b], (p, o)) in stroke.points.array_windows::<2>().zip(perpendiculars.iter().zip(border_perpendiculars)) {
						let current_index = stroke_index + u32::try_from(positions.len()).unwrap();
						positions.extend([a.position + p * a.pressure + o, a.position - p * a.pressure - o, b.position + p * b.pressure + o, b.position - p * b.pressure - o].map(|x| x + stroke.origin + stroke_offset));
						indices.extend([0, 2, 3, 0, 3, 1].map(|n| current_index + n));
					}

					for (i, [p, q]) in perpendiculars.array_windows::<2>().enumerate() {
						let i = u32::try_from(i).unwrap();
						let cross_product = p.cross(*q);

						if cross_product > Vx2(0.) {
							/* Clockwise */
							indices.extend([2, 4 + 0, 4 + 1].map(|n| stroke_index + n + i * 4));
						} else if cross_product < Vx2(0.) {
							/* Counterclockwise */
							indices.extend([3, 4 + 1, 4 + 0].map(|n| stroke_index + n + i * 4));
						}
					}

					vertices.extend(positions.into_iter().map(|position| Vertex {
						position: [position[0], position[1]],
						color: BORDER_COLOR.map(srgb8_to_f32),
					}));
				}

				let stroke_index = u32::try_from(vertices.len()).unwrap();

				let mut positions = vec![];
				for ([a, b], p) in stroke.points.array_windows::<2>().zip(&perpendiculars) {
					let current_index = stroke_index + u32::try_from(positions.len()).unwrap();
					positions.extend([a.position + p * a.pressure, a.position - p * a.pressure, b.position + p * b.pressure, b.position - p * b.pressure].map(|x| x + stroke.origin + stroke_offset));
					indices.extend([0, 2, 3, 0, 3, 1].map(|n| current_index + n));
				}

				for (i, [p, q]) in perpendiculars.array_windows::<2>().enumerate() {
					let i = u32::try_from(i).unwrap();
					let cross_product = p.cross(*q);

					if cross_product > Vx2(0.) {
						/* Clockwise */
						indices.extend([2, 4 + 0, 4 + 1].map(|n| stroke_index + n + i * 4));
					} else if cross_product < Vx2(0.) {
						/* Counterclockwise */
						indices.extend([3, 4 + 1, 4 + 0].map(|n| stroke_index + n + i * 4));
					}
				}

				vertices.extend(positions.into_iter().map(|position| Vertex {
					position: [position[0], position[1]],
					color: stroke.color.map(srgb8_to_f32),
				}));
			}
		}

		draw_commands.push(DrawCommand::Trimesh { vertices, indices });
	}
}
