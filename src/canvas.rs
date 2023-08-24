// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use crate::{
	render::{stroke_renderer::SelectionTransformation, texture::Texture, Renderer},
	utility::{Tracked, Vex, Vx, Vx2, Zero, Zoom, HSV, SRGBA8},
};

#[derive(Clone)]
pub struct Point {
	pub position: Vex<2, Vx>,
	pub pressure: f32,
}

#[derive(Clone)]
pub struct Image {
	pub texture_index: usize,
	pub dimensions: Vex<2, Vx>,
}

#[derive(Clone)]
pub struct Stroke {
	// Local coordinate system.
	pub position: Vex<2, Vx>,
	pub orientation: f32,
	pub dilation: f32,

	// Modifiable data.
	pub is_selected: bool,
	pub color: SRGBA8,

	// Geometry parameters.
	pub stroke_radius: Vx,
	pub points: Vec<Point>,

	// Cached geometry.
	pub vertices: Vec<(Vex<2, Vx>, f32)>,
	pub relative_indices: Vec<u32>,
}

impl Stroke {
	pub fn new(color: SRGBA8, stroke_radius: Vx, points: Vec<Point>, position: Vex<2, Vx>, orientation: f32, dilation: f32) -> Self {
		let (vertices, relative_indices) = Self::compute_geometry(&points, stroke_radius);

		Self {
			position,
			orientation,
			dilation,
			is_selected: false,
			color,
			stroke_radius,
			points,
			vertices,
			relative_indices,
		}
	}

	fn compute_geometry(points: &[Point], stroke_radius: Vx) -> (Vec<(Vex<2, Vx>, f32)>, Vec<u32>) {
		if let [point] = points {
			let heptagonal_vertices = {
				use std::f32::consts::PI;
				let i = Vex([point.pressure * stroke_radius, Vx(0.)]);
				vec![
					(Vex::ZERO, 0.),
					(i, 1.),
					(i.rotate(2. * PI * 1. / 7.), 1.),
					(i.rotate(2. * PI * 2. / 7.), 1.),
					(i.rotate(2. * PI * 3. / 7.), 1.),
					(i.rotate(2. * PI * 4. / 7.), 1.),
					(i.rotate(2. * PI * 5. / 7.), 1.),
					(i.rotate(2. * PI * 6. / 7.), 1.),
				]
			};

			let heptagonal_indices = vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 7, 0, 7, 1];

			(heptagonal_vertices, heptagonal_indices)
		} else {
			let mut vertices = vec![];
			let mut indices = vec![];

			// We compute the four bounding vertices of each line segment.
			for [a, b] in points.array_windows::<2>() {
				// We compute a unit normal.
				let perpendicular = {
					let forward = b.position - a.position;
					Vex([forward[1], -forward[0]]).normalized() * STROKE_RADIUS
				};

				let current_index = u32::try_from(vertices.len()).unwrap();
				vertices.extend([
					(a.position + perpendicular * a.pressure, 1.),
					(a.position - perpendicular * a.pressure, -1.),
					(b.position + perpendicular * b.pressure, 1.),
					(b.position - perpendicular * b.pressure, -1.),
				]);
				indices.extend([0, 2, 3, 0, 3, 1].map(|n| current_index + n));
			}

			for (i, [a, b, c]) in points.array_windows::<3>().enumerate() {
				let p = b.position - a.position;
				let q = c.position - b.position;
				let i = u32::try_from(i).unwrap();
				let cross_product = p.cross(q);

				if cross_product > Vx2(0.) {
					/* Clockwise */
					indices.extend([2, 4 + 0, 4 + 1].map(|n| n + i * 4));
				} else if cross_product < Vx2(0.) {
					/* Counterclockwise */
					indices.extend([3, 4 + 1, 4 + 0].map(|n| n + i * 4));
				}
			}

			(vertices, indices)
		}
	}
}

#[derive(Clone)]
pub struct IncompleteStroke {
	pub position: Vex<2, Vx>,
	pub color: SRGBA8,
	pub points: Vec<Point>,
	pub max_pressure: f32,
}

const STROKE_RADIUS: Vx = Vx(4.);

impl IncompleteStroke {
	pub fn new(position: Vex<2, Vx>, color: SRGBA8) -> Self {
		Self {
			position,
			color,
			points: Vec::new(),
			max_pressure: 0.,
		}
	}

	pub fn add_point(&mut self, position: Vex<2, Vx>, pressure: f32) {
		let threshold = if self.points.len() < 2 {
			(self.max_pressure.max(pressure) * STROKE_RADIUS).max(Vx(1.))
		} else {
			self.max_pressure.max(pressure) * STROKE_RADIUS.min(Vx(1.))
		};
		if self.points.last().map_or(true, |point| (position - point.position).norm() > threshold) {
			self.points.push(Point { position, pressure });
			self.max_pressure = pressure;
		} else {
			self.max_pressure = self.max_pressure.max(pressure);
		}
	}

	pub fn finalize(mut self) -> Stroke {
		let local_centroid = if !self.points.is_empty() {
			let local_centroid = self.points.iter().fold(Vex::ZERO, |acc, point| acc + point.position) / self.points.len() as f32;
			for point in self.points.iter_mut() {
				point.position = point.position - local_centroid;
			}
			local_centroid
		} else {
			Vex::ZERO
		};

		if let [point] = self.points.as_mut_slice() {
			point.pressure = self.max_pressure;
		}

		Stroke::new(self.color, STROKE_RADIUS, self.points, self.position + local_centroid, 0., 1.)
	}

	pub fn preview(&self) -> Stroke {
		let points = if self.points.len() != 1 { self.points.clone() } else { Vec::new() };

		Stroke::new(self.color, STROKE_RADIUS, points, self.position, 0., 1.)
	}
}

enum Retraction {
	CommitStrokes(usize),
	CommitImages(usize),
	DeleteObjects {
		antitone_index_image_pairs: Vec<(usize, Object<Image>)>,
		antitone_index_stroke_pairs: Vec<(usize, Stroke)>,
	},
	RecolorStrokes {
		index_color_pairs: Vec<(usize, SRGBA8)>,
		new_color: SRGBA8,
	},
	TranslateObjects {
		image_indices: Vec<usize>,
		stroke_indices: Vec<usize>,
		vector: Vex<2, Vx>,
	},
	RotateObjects {
		image_indices: Vec<usize>,
		stroke_indices: Vec<usize>,
		center: Vex<2, Vx>,
		angle: f32,
	},
	ResizeObjects {
		image_indices: Vec<usize>,
		stroke_indices: Vec<usize>,
		center: Vex<2, Vx>,
		dilation: f32,
	},
}

pub enum Operation {
	CommitStrokes { strokes: Vec<Tracked<Stroke>> },
	CommitImages { images: Vec<Object<Image>> },
	DeleteObjects { monotone_image_indices: Vec<usize>, monotone_stroke_indices: Vec<usize> },
	RecolorStrokes { indices: Vec<usize>, new_color: SRGBA8 },
	TranslateObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, vector: Vex<2, Vx> },
	RotateObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, center: Vex<2, Vx>, angle: f32 },
	ResizeObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, center: Vex<2, Vx>, dilation: f32 },
}

#[derive(Clone)]
pub struct Object<T> {
	pub object: T,
	// Position of the local origin.
	pub position: Vex<2, Vx>,
	// Orientation about the local origin.
	pub orientation: f32,
	// Dilation about the local origin.
	pub dilation: f32,
	pub is_selected: bool,
}

pub struct View {
	pub position: Vex<2, Vx>,
	pub tilt: f32,
	pub zoom: Zoom,
}

impl View {
	fn new() -> Self {
		Self { position: Vex::ZERO, tilt: 0., zoom: Zoom(1.) }
	}
}

pub struct Canvas {
	pub file_path: Option<PathBuf>,
	pub background_color: HSV,
	pub view: Tracked<View>,
	pub images: Vec<Object<Image>>,
	pub strokes: Vec<Tracked<Stroke>>,
	retractions: Vec<Retraction>,
	operations: Vec<Operation>,
	pub textures: Vec<Texture>,
	pub retraction_count_at_save: Option<usize>,
	// Tracks the smallest index of a stroke with invalidated geometry.
	pub base_dirty_stroke_index: usize,
	pub selection_transformation: Tracked<SelectionTransformation>,
}

impl Canvas {
	pub fn new(background_color: HSV) -> Self {
		Self {
			file_path: None,
			background_color,
			view: View::new().into(),
			images: Vec::new(),
			strokes: Vec::new(),
			retractions: Vec::new(),
			operations: Vec::new(),
			textures: Vec::new(),
			retraction_count_at_save: None,
			base_dirty_stroke_index: 0,
			selection_transformation: Default::default(),
		}
	}

	pub fn from_file(file_path: PathBuf, background_color: HSV, view: View, images: Vec<Object<Image>>, strokes: Vec<Tracked<Stroke>>, textures: Vec<Texture>) -> Self {
		Self {
			file_path: Some(file_path),
			background_color,
			view: view.into(),
			images,
			strokes,
			retractions: Vec::new(),
			operations: Vec::new(),
			textures,
			retraction_count_at_save: Some(0),
			base_dirty_stroke_index: 0,
			selection_transformation: Default::default(),
		}
	}

	pub fn images(&self) -> &[Object<Image>] {
		self.images.as_ref()
	}

	pub fn strokes(&self) -> &[Tracked<Stroke>] {
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
				CommitImages { mut images } => {
					let length = images.len();
					self.images.append(&mut images);

					Retraction::CommitImages(length)
				},
				DeleteObjects { monotone_image_indices, monotone_stroke_indices } => {
					let mut antitone_index_image_pairs = Vec::with_capacity(monotone_image_indices.len());

					for index in monotone_image_indices.iter().rev().copied() {
						debug_assert!(index < self.images.len());
						let image = self.images.remove(index);
						antitone_index_image_pairs.push((index, image));
					}

					let mut antitone_index_stroke_pairs = Vec::with_capacity(monotone_stroke_indices.len());

					for index in monotone_stroke_indices.iter().rev().copied() {
						debug_assert!(index < self.strokes.len());
						let stroke = self.strokes.remove(index);
						antitone_index_stroke_pairs.push((index, stroke.take()));
					}

					if let Some(index) = monotone_stroke_indices.first() {
						self.base_dirty_stroke_index = self.base_dirty_stroke_index.min(*index);
					}

					Retraction::DeleteObjects {
						antitone_index_image_pairs,
						antitone_index_stroke_pairs,
					}
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
				TranslateObjects { image_indices, stroke_indices, vector } => {
					for index in image_indices.iter().copied() {
						if let Some(object) = self.images.get_mut(index) {
							object.position = object.position + vector;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index) {
							stroke.position = stroke.position + vector;
						}
					}

					Retraction::TranslateObjects { image_indices, stroke_indices, vector }
				},
				RotateObjects { image_indices, stroke_indices, center, angle } => {
					for index in image_indices.iter().copied() {
						if let Some(object) = self.images.get_mut(index) {
							object.position = object.position.rotate_about(center, angle);
							object.orientation += angle;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index).map(AsMut::as_mut) {
							stroke.position = stroke.position.rotate_about(center, angle);
							stroke.orientation += angle;
						}
					}

					Retraction::RotateObjects { image_indices, stroke_indices, center, angle }
				},
				ResizeObjects { image_indices, stroke_indices, center, dilation } => {
					for index in image_indices.iter().copied() {
						if let Some(object) = self.images.get_mut(index) {
							object.position = object.position.dilate_about(center, dilation);
							object.dilation *= dilation;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index).map(AsMut::as_mut) {
							stroke.position = stroke.position.dilate_about(center, dilation);
							stroke.dilation *= dilation;
						}
					}

					Retraction::ResizeObjects { image_indices, stroke_indices, center, dilation }
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

					self.base_dirty_stroke_index = self.base_dirty_stroke_index.min(self.strokes.len());

					Operation::CommitStrokes { strokes }
				},
				CommitImages(length) => {
					let mut images = Vec::with_capacity(length);

					debug_assert!(length <= self.images.len());
					for _ in 0..length {
						images.push(self.images.pop().unwrap());
					}

					Operation::CommitImages { images }
				},
				DeleteObjects {
					antitone_index_image_pairs,
					antitone_index_stroke_pairs,
				} => {
					let mut monotone_image_indices = Vec::with_capacity(antitone_index_image_pairs.len());

					for (index, image) in antitone_index_image_pairs.into_iter().rev() {
						debug_assert!(index <= self.strokes.len());
						self.images.insert(index, image);
						monotone_image_indices.push(index);
					}

					let mut monotone_stroke_indices = Vec::with_capacity(antitone_index_stroke_pairs.len());

					for (index, stroke) in antitone_index_stroke_pairs.into_iter().rev() {
						debug_assert!(index <= self.strokes.len());
						self.strokes.insert(index, stroke.into());
						monotone_stroke_indices.push(index);
					}

					if let Some(index) = monotone_stroke_indices.first() {
						self.base_dirty_stroke_index = self.base_dirty_stroke_index.min(*index);
					}

					Operation::DeleteObjects { monotone_image_indices, monotone_stroke_indices }
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
				TranslateObjects { image_indices, stroke_indices, vector } => {
					for index in image_indices.iter().copied() {
						if let Some(image) = self.images.get_mut(index) {
							image.position = image.position - vector;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index) {
							stroke.position = stroke.position - vector;
						}
					}

					Operation::TranslateObjects { image_indices, stroke_indices, vector }
				},
				RotateObjects { image_indices, stroke_indices, center, angle } => {
					for index in image_indices.iter().copied() {
						if let Some(object) = self.images.get_mut(index) {
							object.position = object.position.rotate_about(center, -angle);
							object.orientation -= angle;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index).map(AsMut::as_mut) {
							stroke.position = stroke.position.rotate_about(center, -angle);
							stroke.orientation -= angle;
						}
					}

					Operation::RotateObjects { image_indices, stroke_indices, center, angle }
				},
				ResizeObjects { image_indices, stroke_indices, center, dilation } => {
					for index in image_indices.iter().copied() {
						if let Some(object) = self.images.get_mut(index) {
							object.position = object.position.dilate_about(center, 1. / dilation);
							object.dilation /= dilation;
						}
					}

					for index in stroke_indices.iter().copied() {
						if let Some(stroke) = self.strokes.get_mut(index).map(AsMut::as_mut) {
							stroke.position = stroke.position.dilate_about(center, 1. / dilation);
							stroke.dilation /= dilation;
						}
					}

					Operation::ResizeObjects { image_indices, stroke_indices, center, dilation }
				},
			});
		}
	}

	pub fn perform_operation(&mut self, operation: Operation) {
		if let Some(retraction_count_at_save) = self.retraction_count_at_save {
			if self.retractions.len() < retraction_count_at_save {
				self.retraction_count_at_save = None;
			}
		}
		self.operations.clear();
		self.operations.push(operation);
		self.redo();
	}

	pub fn select(&mut self, min: Vex<2, Vx>, max: Vex<2, Vx>, tilt: f32, screen_center: Vex<2, Vx>, should_aggregate: bool) {
		let selection_corners = [min, Vex([max[0], min[1]]), max, Vex([min[0], max[1]])].map(|v| v.rotate(tilt) + screen_center);
		let selection_center = (selection_corners[2] + selection_corners[0]) / 2.;
		let selection_semidimensions = (selection_corners[2] - selection_corners[0]) / 2.;
		let alpha_hat = (selection_corners[1] - selection_corners[0]).normalized();
		let beta_hat = (selection_corners[3] - selection_corners[0]).normalized();
		for image in self.images.iter_mut() {
			let image_corners = [-image.object.dimensions, image.object.dimensions.flip::<1>(), image.object.dimensions, image.object.dimensions.flip::<0>()].map(|v| ((v * 0.5).rotate(image.orientation) * image.dilation) + image.position);
			let image_semidimensions = image.object.dimensions * 0.5 * image.dilation;
			let gamma_hat = (image_corners[1] - image_corners[0]).normalized();
			let delta_hat = (image_corners[3] - image_corners[0]).normalized();
			// I'm so sorry for this, I promise I'll clean it up later :(
			let no_overlap = {
				let projected_image_corners = image_corners.map(|corner| (corner - selection_center).dot(alpha_hat));
				projected_image_corners[1] * projected_image_corners[0] >= Vx2(0.)
					&& projected_image_corners[2] * projected_image_corners[1] >= Vx2(0.)
					&& projected_image_corners[3] * projected_image_corners[2] >= Vx2(0.)
					&& projected_image_corners.iter().fold(true, |acc, corner| acc && corner.abs() > selection_semidimensions[0])
			} || {
				let projected_image_corners = image_corners.map(|corner| (corner - selection_center).dot(beta_hat));
				projected_image_corners[1] * projected_image_corners[0] >= Vx2(0.)
					&& projected_image_corners[2] * projected_image_corners[1] >= Vx2(0.)
					&& projected_image_corners[3] * projected_image_corners[2] >= Vx2(0.)
					&& projected_image_corners.iter().fold(true, |acc, corner| acc && corner.abs() > selection_semidimensions[1])
			} || {
				let projected_selection_corners = selection_corners.map(|corner| (corner - image.position).dot(gamma_hat));
				projected_selection_corners[1] * projected_selection_corners[0] >= Vx2(0.)
					&& projected_selection_corners[2] * projected_selection_corners[1] >= Vx2(0.)
					&& projected_selection_corners[3] * projected_selection_corners[2] >= Vx2(0.)
					&& projected_selection_corners.iter().fold(true, |acc, corner| acc && corner.abs() > image_semidimensions[0])
			} || {
				let projected_selection_corners = selection_corners.map(|corner| (corner - image.position).dot(delta_hat));
				projected_selection_corners[1] * projected_selection_corners[0] >= Vx2(0.)
					&& projected_selection_corners[2] * projected_selection_corners[1] >= Vx2(0.)
					&& projected_selection_corners[3] * projected_selection_corners[2] >= Vx2(0.)
					&& projected_selection_corners.iter().fold(true, |acc, corner| acc && corner.abs() > image_semidimensions[1])
			};

			if should_aggregate {
				image.is_selected = image.is_selected ^ !no_overlap;
			} else {
				image.is_selected = !no_overlap;
			}
		}

		'strokes: for stroke in self.strokes.iter_mut() {
			if should_aggregate {
				for point in stroke.points.iter() {
					let point_position = (stroke.position + point.position.rotate(stroke.orientation) * stroke.dilation - screen_center).rotate(-tilt);
					if point_position[0] >= min[0] && point_position[1] >= min[1] && point_position[0] <= max[0] && point_position[1] <= max[1] {
						stroke.is_selected = !stroke.is_selected;
						continue 'strokes;
					}
				}
			} else {
				for point in stroke.points.iter() {
					let point_position = (stroke.position + point.position.rotate(stroke.orientation) * stroke.dilation - screen_center).rotate(-tilt);
					if point_position[0] >= min[0] && point_position[1] >= min[1] && point_position[0] <= max[0] && point_position[1] <= max[1] {
						if stroke.is_selected == false {
							stroke.is_selected = true;
						}
						continue 'strokes;
					}
				}
				if stroke.is_selected == true {
					stroke.is_selected = false;
				}
			}
		}
	}

	pub fn set_retraction_count_at_save(&mut self) {
		self.retraction_count_at_save = Some(self.retractions.len());
	}

	pub fn is_saved(&self) -> bool {
		self.retraction_count_at_save.map_or(false, |x| x == self.retractions.len())
	}

	pub fn select_all(&mut self, is_selected: bool) {
		for image in self.images.iter_mut() {
			image.is_selected = is_selected;
		}

		for stroke in self.strokes.iter_mut() {
			stroke.is_selected = is_selected;
		}
	}

	pub fn push_texture(&mut self, renderer: &Renderer, dimensions: [u32; 2], image: Vec<u8>) -> usize {
		self.textures.push(renderer.create_texture(dimensions, image));
		self.textures.len() - 1
	}
}
