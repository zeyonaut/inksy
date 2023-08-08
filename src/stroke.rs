// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use fast_srgb8::srgb8_to_f32;
use vek::Vec2;

use crate::render::Vertex;

pub struct Point {
	position: Vec2<f32>,
	pressure: f32,
}

pub struct Stroke {
	pub origin: Vec2<f32>,
	pub color: [u8; 4],
	pub points: Vec<Point>,
	pub is_selected: bool,
}

const STROKE_RADIUS: f32 = 8.;

impl Stroke {
	pub fn new(color: [u8; 4]) -> Self {
		Self {
			origin: Vec2::zero(),
			color,
			points: Vec::new(),
			is_selected: false,
		}
	}

	pub fn add_point(&mut self, x: f32, y: f32, pressure: f32) {
		if self.points.last().map_or(true, |point| (Vec2::new(x, y) - point.position).magnitude() > 2.) {
			self.points.push(Point { position: Vec2::new(x, y), pressure });
		}
	}
}

pub struct Canvas {
	pub strokes: Vec<Stroke>,
}

impl Canvas {
	pub fn new() -> Self {
		Self { strokes: Vec::new() }
	}

	pub fn select(&mut self, min: Vec2<f32>, max: Vec2<f32>, should_aggregate: bool) {
		'strokes: for stroke in self.strokes.iter_mut() {
			let min = min - stroke.origin;
			let max = max - stroke.origin;
			if should_aggregate {
				for point in stroke.points.iter() {
					if point.position.x >= min.x && point.position.y >= min.y && point.position.x <= max.x && point.position.y <= max.y {
						stroke.is_selected = !stroke.is_selected;
						continue 'strokes;
					}
				}
			} else {
				for point in stroke.points.iter() {
					if point.position.x >= min.x && point.position.y >= min.y && point.position.x <= max.x && point.position.y <= max.y {
						stroke.is_selected = true;
						continue 'strokes;
					}
				}
				stroke.is_selected = false;
			}
		}
	}

	pub fn bake(&self, current_stroke: Option<&Stroke>, selection_offset: Option<Vec2<f32>>) -> (Vec<Vertex>, Vec<u16>) {
		let mut vertices = vec![];
		let mut indices = vec![];

		for stroke in self.strokes.iter().chain(current_stroke.into_iter()) {
			let stroke_offset = if stroke.is_selected { selection_offset.unwrap_or(Vec2::zero()) } else { Vec2::zero() };
			let stroke_index = u16::try_from(vertices.len()).unwrap();
			let mut positions = vec![];
			let perpendiculars = stroke
				.points
				.array_windows::<2>()
				.map(|[a, b]| {
					let forward = b.position - a.position;
					Vec2::new(forward.y, -forward.x).normalized() * STROKE_RADIUS
				})
				.collect::<Vec<_>>();

			for ([a, b], p) in stroke.points.array_windows::<2>().zip(&perpendiculars) {
				let current_index = stroke_index + u16::try_from(positions.len()).unwrap();
				positions.extend([a.position + p * a.pressure, a.position - p * a.pressure, b.position + p * b.pressure, b.position - p * b.pressure].map(|x| x + stroke.origin + stroke_offset));
				indices.extend([0, 2, 3, 0, 3, 1].map(|n| current_index + n));
			}

			for (i, [p, q]) in perpendiculars.array_windows::<2>().enumerate() {
				let i = u16::try_from(i).unwrap();
				let cross_product = p.x * q.y - p.y * q.x;

				if cross_product > 0. {
					/* Clockwise */
					indices.extend([2, 4 + 0, 4 + 1].map(|n| stroke_index + n + i * 4));
				} else if cross_product < 0. {
					/* Counterclockwise */
					indices.extend([3, 4 + 1, 4 + 0].map(|n| stroke_index + n + i * 4));
				}
			}

			vertices.extend(positions.into_iter().map(|position| Vertex {
				position: [position.x, position.y, 0.],
				color: if !stroke.is_selected { stroke.color } else { [0x01, 0x6f, 0xb9, 0xff] }.map(srgb8_to_f32),
			}));
		}

		(vertices, indices)
	}
}
