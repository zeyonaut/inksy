// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{num::NonZeroU32, path::PathBuf};

use crate::{
	config::Config,
	input::{Button, InputMonitor, Key},
	render::{stroke_renderer::SelectionTransformation, text_renderer::Align, texture::Texture, DrawCommand, Prerender, Renderer},
	tools::{ColorSelectionPart, ModeStack, OrbitInitial, PanOrigin, ResizeDraft, RotateDraft, Tool, ZoomOrigin},
	ui::Widget,
	utility::{Hsv, Lx, Px, Scale, Srgb8, Srgba8, Tracked, Vex, Vx, Vx2, Zero, Zoom},
};

#[derive(Clone)]
pub struct Point {
	pub position: Vex<2, Vx>,
	pub pressure: f32,
}

#[derive(Clone)]
pub struct Image {
	// Local coordinate system.
	pub position: Vex<2, Vx>,
	pub orientation: f32,
	pub dilation: f32,

	// Modifiable data.
	pub is_selected: bool,

	// Stable data.
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
	pub color: Srgba8,

	// Geometry parameters.
	pub stroke_radius: Vx,
	pub points: Vec<Point>,

	// Cached geometry.
	pub vertices: Vec<(Vex<2, Vx>, f32)>,
	pub relative_indices: Vec<u32>,
}

impl Stroke {
	pub fn new(color: Srgba8, stroke_radius: Vx, points: Vec<Point>, position: Vex<2, Vx>, orientation: f32, dilation: f32) -> Self {
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
					Vex([forward[1], -forward[0]]).normalized() * stroke_radius
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
					#[allow(clippy::identity_op)]
					indices.extend([2, 4 + 0, 4 + 1].map(|n| n + i * 4));
				} else if cross_product < Vx2(0.) {
					/* Counterclockwise */
					#[allow(clippy::identity_op)]
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
	pub color: Srgba8,
	pub radius: Vx,
	pub points: Vec<Point>,
	pub max_pressure: f32,
}

impl IncompleteStroke {
	pub fn new(position: Vex<2, Vx>, canvas: &Canvas) -> Self {
		Self {
			position,
			color: canvas.stroke_color.to_srgb().to_srgb8().opaque(),
			radius: canvas.stroke_radius,
			points: Vec::new(),
			max_pressure: 0.,
		}
	}

	pub fn add_point(&mut self, position: Vex<2, Vx>, pressure: f32) {
		let threshold = if self.points.len() < 2 {
			(self.max_pressure.max(pressure) * self.radius).max(Vx(1.))
		} else {
			self.max_pressure.max(pressure) * self.radius.min(Vx(1.))
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

		Stroke::new(self.color, self.radius, self.points, self.position + local_centroid, 0., 1.)
	}

	pub fn preview(&self) -> Stroke {
		let points = if self.points.len() != 1 { self.points.clone() } else { Vec::new() };

		Stroke::new(self.color, self.radius, points, self.position, 0., 1.)
	}
}

enum Retraction {
	CommitStrokes(usize),
	CommitImages(usize),
	DeleteObjects {
		antitone_index_image_pairs: Vec<(usize, Image)>,
		antitone_index_stroke_pairs: Vec<(usize, Stroke)>,
	},
	RecolorStrokes {
		index_color_pairs: Vec<(usize, Srgba8)>,
		new_color: Srgba8,
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
	CommitImages { images: Vec<Tracked<Image>> },
	DeleteObjects { monotone_image_indices: Vec<usize>, monotone_stroke_indices: Vec<usize> },
	RecolorStrokes { indices: Vec<usize>, new_color: Srgba8 },
	TranslateObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, vector: Vex<2, Vx> },
	RotateObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, center: Vex<2, Vx>, angle: f32 },
	ResizeObjects { image_indices: Vec<usize>, stroke_indices: Vec<usize>, center: Vex<2, Vx>, dilation: f32 },
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

// TODO: Move this somewhere saner.
// Color selector constants in logical pixels/points.
const TRIGON_RADIUS: Lx = Lx(68.);
const HOLE_RADIUS: Lx = Lx(80.);
const RING_WIDTH: Lx = Lx(28.);
const OUTLINE_WIDTH: Lx = Lx(2.);
const SATURATION_VALUE_WINDOW_DIAMETER: Lx = Lx(8.);

pub struct Multicanvas {
	pub is_debug_mode_on: bool,
	pub canvases: Vec<Canvas>,
	// Should only be `None` iff `canvases` is empty.
	pub current_canvas_index: Option<usize>,
	pub was_canvas_saved: bool,
	pub mode_stack: ModeStack,
}

impl Multicanvas {
	pub fn new() -> Self {
		Self {
			is_debug_mode_on: false,
			canvases: Vec::new(),
			current_canvas_index: None,
			was_canvas_saved: false,
			mode_stack: ModeStack::new(Tool::Draw { current_stroke: None }),
		}
	}

	pub fn current_canvas(&self) -> Option<&Canvas> {
		self.current_canvas_index.and_then(|x| self.canvases.get(x))
	}

	pub fn current_canvas_mut(&mut self) -> Option<&mut Canvas> {
		self.current_canvas_index.and_then(|x| self.canvases.get_mut(x))
	}
}

impl Widget for Multicanvas {
	fn update(&mut self, window: &winit::window::Window, renderer: &Renderer, input_monitor: &InputMonitor, is_cursor_relevant: bool, pressure: Option<f64>, cursor_physical_position: Vex<2, Px>, scale: Scale) {
		use Button::*;
		use Key::*;
		if let Some(canvas) = self.current_canvas_index.and_then(|x| self.canvases.get_mut(x)) {
			let semidimensions = Vex([renderer.config.width as f32 / 2., renderer.config.height as f32 / 2.].map(Px)).s(scale).z(canvas.view.zoom);
			let cursor_virtual_position = (cursor_physical_position.s(scale).z(canvas.view.zoom) - semidimensions).rotate(canvas.view.tilt);

			match self.mode_stack.get_mut() {
				Tool::Draw { current_stroke } => {
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Default);
					}
					if input_monitor.active_buttons.contains(Left) {
						if input_monitor.different_buttons.contains(Left) && current_stroke.is_none() {
							*current_stroke = Some(IncompleteStroke::new(cursor_virtual_position, canvas));
						}

						if let Some(current_stroke) = current_stroke {
							let offset = canvas.view.position + cursor_virtual_position - current_stroke.position;
							current_stroke.add_point(
								offset,
								pressure.map_or(1., |pressure| {
									let x = (pressure / 32767.) as f32;
									x * (17. + x * -18. + x * x * 7.) / 6.
								}),
							)
						}
					} else if let Some(stroke) = current_stroke.take() {
						canvas.perform_operation(Operation::CommitStrokes { strokes: vec![stroke.finalize().into()] });
					}
				},
				Tool::Select { origin } => {
					let offset = cursor_virtual_position + canvas.view.position;
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Crosshair);
					}

					if input_monitor.active_buttons.contains(Left) {
						if input_monitor.different_buttons.contains(Left) && origin.is_none() {
							*origin = Some(offset);
						}
					} else if let Some(origin) = origin.take() {
						let offset = cursor_virtual_position.rotate(-canvas.view.tilt);
						let origin = (origin - canvas.view.position).rotate(-canvas.view.tilt);
						let min = Vex([offset[0].min(origin[0]), offset[1].min(origin[1])]);
						let max = Vex([offset[0].max(origin[0]), offset[1].max(origin[1])]);
						canvas.select(min, max, canvas.view.tilt, canvas.view.position, input_monitor.active_keys.contains(Shift));
					}
				},
				Tool::Pan { origin } => {
					if input_monitor.active_buttons.contains(Left) {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
						}
						if origin.is_none() {
							*origin = Some(PanOrigin {
								cursor: cursor_virtual_position,
								position: canvas.view.position,
							});
						}
					} else {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::Grab);
						}
						if origin.is_some() {
							*origin = None;
						}
					}

					if let Some(origin) = origin {
						canvas.view.position = origin.position - (cursor_virtual_position - origin.cursor);
					}
				},
				Tool::Zoom { origin } => {
					if input_monitor.active_buttons.contains(Left) {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::ZoomIn);
						}
						if origin.is_none() {
							let window_height = Px(renderer.config.height as f32);
							*origin = Some(ZoomOrigin {
								initial_zoom: canvas.view.zoom.0,
								initial_y_ratio: cursor_physical_position[1] / window_height,
							});
						}
					} else {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::ZoomIn);
						}
						if origin.is_some() {
							*origin = None;
						}
					}

					if let Some(origin) = origin {
						let window_height = Px(renderer.config.height as f32);
						let y_ratio = cursor_physical_position[1] / window_height;
						let zoom_ratio = f32::powf(8., origin.initial_y_ratio - y_ratio);
						canvas.view.zoom = Zoom(origin.initial_zoom * zoom_ratio);
					}
				},
				Tool::Orbit { initial } => {
					if input_monitor.active_buttons.contains(Left) {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::Grabbing);
						}
						if initial.is_none() {
							let semidimensions = Vex([renderer.config.width as f32 / 2., renderer.config.height as f32 / 2.].map(Px));
							let vector = cursor_physical_position - semidimensions;
							let angle = vector.angle();
							*initial = Some(OrbitInitial { tilt: canvas.view.tilt, cursor_angle: angle });
						}
					} else {
						if is_cursor_relevant {
							window.set_cursor_icon(winit::window::CursorIcon::Grab);
						}
						*initial = None;
					}

					if let Some(OrbitInitial { tilt, cursor_angle }) = initial {
						let semidimensions = Vex([renderer.config.width as f32 / 2., renderer.config.height as f32 / 2.].map(Px));
						let vector = cursor_physical_position - semidimensions;
						let angle = vector.angle();
						canvas.view.tilt = *tilt - angle + *cursor_angle;
					}
				},
				Tool::Move { origin } => {
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if input_monitor.active_buttons.contains(Left) {
						if input_monitor.different_buttons.contains(Left) && origin.is_none() {
							*origin = Some(canvas.view.position + cursor_virtual_position);
						}
					} else if let Some(origin) = origin.take() {
						let selection_offset = canvas.view.position + cursor_virtual_position - origin;

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| image.is_selected.then_some(index)).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| stroke.is_selected.then_some(index)).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::TranslateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								vector: selection_offset,
							});
						}

						canvas.selection_transformation = Default::default();
					}
				},
				Tool::Rotate { origin } => {
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if input_monitor.active_buttons.contains(Left) {
						if input_monitor.different_buttons.contains(Left) && origin.is_none() {
							// Compute the centroid.
							let (sum, count) = canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

							let (sum, count) = canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

							let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

							*origin = Some({
								RotateDraft {
									center,
									initial_position: canvas.view.position + cursor_virtual_position - center,
								}
							});
						}
					} else if let Some(RotateDraft { center, initial_position }) = origin.take() {
						let selection_offset = canvas.view.position + cursor_virtual_position - center;
						let angle = initial_position.angle_to(selection_offset);

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::RotateObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								angle,
							});
						}

						canvas.selection_transformation = Default::default();
					}
				},
				Tool::Resize { origin } => {
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Move);
					}

					if input_monitor.active_buttons.contains(Left) {
						if input_monitor.different_buttons.contains(Left) && origin.is_none() {
							// Compute the centroid.
							let (sum, count) = canvas.strokes().iter().fold((Vex::ZERO, 0), |(sum, count), stroke| if stroke.is_selected { (sum + stroke.position, count + 1) } else { (sum, count) });

							let (sum, count) = canvas.images().iter().fold((sum, count), |(sum, count), image| if image.is_selected { (sum + image.position, count + 1) } else { (sum, count) });

							let center = if count > 0 { sum / count as f32 } else { Vex::ZERO };

							*origin = Some({
								ResizeDraft {
									center,
									initial_distance: (canvas.view.position + cursor_virtual_position - center).norm(),
								}
							});
						}
					} else if let Some(ResizeDraft { center, initial_distance }) = origin.take() {
						let selection_distance = (canvas.view.position + cursor_virtual_position - center).norm();
						let dilation = selection_distance / initial_distance;

						let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

						if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
							canvas.perform_operation(Operation::ResizeObjects {
								image_indices: selected_image_indices,
								stroke_indices: selected_stroke_indices,
								center,
								dilation,
							});
						}

						canvas.selection_transformation = Default::default();
					}
				},
				Tool::PickColor { cursor_physical_origin, part } => {
					if is_cursor_relevant {
						window.set_cursor_icon(winit::window::CursorIcon::Crosshair);
					}

					if input_monitor.active_buttons.contains(Left) {
						let cursor = cursor_physical_position;
						let vector = cursor - *cursor_physical_origin;
						if part.is_none() && input_monitor.different_buttons.contains(Left) {
							let magnitude = vector.norm();
							if magnitude >= HOLE_RADIUS.s(scale) && magnitude <= (HOLE_RADIUS + RING_WIDTH).s(scale) {
								*part = Some(ColorSelectionPart::Hue);
							} else if 2. * vector[1] < TRIGON_RADIUS.s(scale) && -(3.0f32.sqrt()) * vector[0] - vector[1] < TRIGON_RADIUS.s(scale) && (3.0f32.sqrt()) * vector[0] - vector[1] < TRIGON_RADIUS.s(scale) {
								*part = Some(ColorSelectionPart::SaturationValue);
							}
						}

						match part {
							Some(ColorSelectionPart::Hue) => {
								canvas.stroke_color[0] = vector.angle() / (2.0 * std::f32::consts::PI) + 0.5;
							},
							Some(ColorSelectionPart::SaturationValue) => {
								let scaled_vector = vector / TRIGON_RADIUS.s(scale);
								let other = Vex([-(3.0f32.sqrt()) / 2., -1. / 2.]);
								let dot = other.dot(scaled_vector);
								let scaled_vector = scaled_vector + -other * (dot - dot.min(0.5));
								let scaled_vector = Vex([scaled_vector[0].max(-(3.0f32.sqrt()) / 2.), scaled_vector[1].min(0.5)]);
								let s = (1. - 2. * scaled_vector[1]) / (2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]);
								canvas.stroke_color[1] = if s.is_nan() { 0. } else { s.clamp(0., 1.) };
								canvas.stroke_color[2] = ((2. + 3.0f32.sqrt() * scaled_vector[0] - scaled_vector[1]) / 3.).clamp(0., 1.);
							},
							None => {},
						}
					} else {
						*part = None;
					}
				},
			}
		}
	}

	fn prepare<'a>(&'a mut self, renderer: &mut Renderer, scale: Scale, cursor_physical_position: Vex<2, Px>, prerender: &mut Prerender<'a>) {
		let mut current_canvas = self.current_canvas_index.and_then(|x| self.canvases.get_mut(x));

		if let Some(canvas) = current_canvas.as_mut() {
			let semidimensions = Vex([renderer.config.width as f32 / 2., renderer.config.height as f32 / 2.].map(Px)).s(scale).z(canvas.view.zoom);
			let cursor_virtual_position = (cursor_physical_position.s(scale).z(canvas.view.zoom) - semidimensions).rotate(canvas.view.tilt);

			// TODO: Move this somwhere else; it's more related to input handling than rendering.
			if self.mode_stack.discarded_transformation_draft.read_if_dirty().is_some() {
				canvas.selection_transformation.reset_to_default();
			}
			match &self.mode_stack.base_mode {
				Tool::Move { origin: Some(origin) } => {
					*canvas.selection_transformation = SelectionTransformation {
						translation: canvas.view.position + cursor_virtual_position - *origin,
						..Default::default()
					};
				},
				Tool::Rotate {
					origin: Some(RotateDraft { center, initial_position }),
				} => {
					let selection_offset = canvas.view.position + cursor_virtual_position - center;
					let angle = initial_position.angle_to(selection_offset);
					*canvas.selection_transformation = SelectionTransformation {
						center_of_transformation: *center,
						rotation: angle,
						..Default::default()
					};
				},
				Tool::Resize {
					origin: Some(ResizeDraft { center, initial_distance }),
				} => {
					let selection_distance = (canvas.view.position + cursor_virtual_position - center).norm();
					let dilation = selection_distance / initial_distance;
					*canvas.selection_transformation = SelectionTransformation {
						center_of_transformation: *center,
						dilation,
						..Default::default()
					};
				},
				_ => {},
			}

			match &self.mode_stack.get() {
				Tool::Select { origin: Some(origin) } => {
					let current = (cursor_virtual_position.rotate(-canvas.view.tilt) + semidimensions).z(canvas.view.zoom).s(scale);
					let origin = ((origin - canvas.view.position).rotate(-canvas.view.tilt) + semidimensions).z(canvas.view.zoom).s(scale);
					let topleft = Vex([current[0].min(origin[0]), current[1].min(origin[1])]);
					prerender.draw_commands.push(DrawCommand::Card {
						position: topleft,
						dimensions: (current - origin).map(|n| n.abs()),
						color: [0x22, 0xae, 0xd1, 0x33],
						radius: Px(0.),
					});
				},
				Tool::Orbit { .. } => {
					let center = Vex([renderer.config.width as f32 / 2., renderer.config.height as f32 / 2.].map(Px));
					let hue_outline_width = (SATURATION_VALUE_WINDOW_DIAMETER + 4. * OUTLINE_WIDTH).s(scale);
					let hue_frame_width = (SATURATION_VALUE_WINDOW_DIAMETER + 2. * OUTLINE_WIDTH).s(scale);
					let hue_window_width = SATURATION_VALUE_WINDOW_DIAMETER.s(scale);
					prerender.draw_commands.push(DrawCommand::Card {
						position: center.map(|x| x - hue_outline_width / 2.),
						dimensions: Vex([hue_outline_width; 2]),
						color: [0xff; 4],
						radius: hue_outline_width / 2.,
					});
					prerender.draw_commands.push(DrawCommand::Card {
						position: center.map(|x| x - hue_frame_width / 2.),
						dimensions: Vex([hue_frame_width; 2]),
						color: [0x00, 0x00, 0x00, 0xff],
						radius: hue_frame_width / 2.,
					});
					let srgba8 = canvas.stroke_color.to_srgb().to_srgb8().opaque();
					prerender.draw_commands.push(DrawCommand::Card {
						position: center.map(|x| x - hue_window_width / 2.),
						dimensions: Vex([hue_window_width; 2]),
						color: srgba8.0,
						radius: hue_window_width / 2.,
					});
				},
				Tool::PickColor { cursor_physical_origin: cursor_origin, .. } => {
					prerender.draw_commands.push(DrawCommand::ColorSelector {
						position: cursor_origin.map(|x| x - (HOLE_RADIUS + RING_WIDTH).s(scale)),
						hsv: canvas.stroke_color.0,
						trigon_radius: TRIGON_RADIUS.s(scale),
						hole_radius: HOLE_RADIUS.s(scale),
						ring_width: RING_WIDTH.s(scale),
					});

					let srgba8 = canvas.stroke_color.to_srgb().to_srgb8().opaque();

					let ring_position = cursor_origin
						+ Vex([
							(HOLE_RADIUS + RING_WIDTH / 2.).s(scale) * -(canvas.stroke_color[0] * 2. * core::f32::consts::PI).cos(),
							(HOLE_RADIUS + RING_WIDTH / 2.).s(scale) * -(canvas.stroke_color[0] * 2. * core::f32::consts::PI).sin(),
						]);

					let hue_outline_width = (RING_WIDTH + 4. * OUTLINE_WIDTH).s(scale);
					let hue_frame_width = (RING_WIDTH + 2. * OUTLINE_WIDTH).s(scale);
					let hue_window_width = RING_WIDTH.s(scale);
					prerender.draw_commands.push(DrawCommand::Card {
						position: ring_position.map(|x| x - hue_outline_width / 2.),
						dimensions: Vex([hue_outline_width; 2]),
						color: [0xff; 4],
						radius: hue_outline_width / 2.,
					});
					prerender.draw_commands.push(DrawCommand::Card {
						position: ring_position.map(|x| x - hue_frame_width / 2.),
						dimensions: Vex([hue_frame_width; 2]),
						color: [0x00, 0x00, 0x00, 0xff],
						radius: hue_frame_width / 2.,
					});
					prerender.draw_commands.push(DrawCommand::Card {
						position: ring_position.map(|x| x - hue_window_width / 2.),
						dimensions: Vex([hue_window_width; 2]),
						color: srgba8.0,
						radius: hue_window_width / 2.,
					});

					let trigon_position = cursor_origin
						+ Vex([
							3.0f32.sqrt() * (canvas.stroke_color[2] - 0.5 * (canvas.stroke_color[1] * canvas.stroke_color[2] + 1.)),
							0.5 * (1. - 3. * canvas.stroke_color[1] * canvas.stroke_color[2]),
						]) * TRIGON_RADIUS.s(scale);

					let sv_outline_width = (SATURATION_VALUE_WINDOW_DIAMETER + (4. * OUTLINE_WIDTH)).s(scale);
					let sv_frame_width = (SATURATION_VALUE_WINDOW_DIAMETER + (2. * OUTLINE_WIDTH)).s(scale);
					let sv_window_width = SATURATION_VALUE_WINDOW_DIAMETER.s(scale);
					prerender.draw_commands.push(DrawCommand::Card {
						position: trigon_position.map(|x| x - sv_outline_width / 2.),
						dimensions: Vex([sv_outline_width; 2]),
						color: [0xff; 4],
						radius: sv_outline_width / 2.,
					});
					prerender.draw_commands.push(DrawCommand::Card {
						position: trigon_position.map(|x| x - sv_frame_width / 2.),
						dimensions: Vex([sv_frame_width; 2]),
						color: [0x00, 0x00, 0x00, 0xff],
						radius: sv_frame_width / 2.,
					});
					prerender.draw_commands.push(DrawCommand::Card {
						position: trigon_position.map(|x| x - sv_window_width / 2.),
						dimensions: Vex([sv_window_width; 2]),
						color: srgba8.0,
						radius: sv_window_width / 2.,
					});
				},
				_ => {},
			}

			if self.is_debug_mode_on {
				let [x, y] = canvas.view.position.0.map(|Vx(a)| a);
				let zoom = canvas.view.zoom.0;
				let tilt = canvas.view.tilt;
				prerender.draw_commands.push(DrawCommand::Text {
					text: format!("position: ({x:.0}, {y:.0})\nzoom: {zoom:.2}\ntilt: {tilt:.2}").into(),
					align: Some(Align::Right),
					position: Vex([Px(renderer.config.width as f32 - scale.0 * 4.), Px(scale.0 * 4.)]),
					anchors: [1., 0.],
				});
			}
		}

		prerender.canvas = current_canvas;
		prerender.current_stroke = self.mode_stack.current_stroke();
	}
}

pub struct Canvas {
	pub file_path: Tracked<Option<PathBuf>>,
	pub background_color: Srgb8,
	pub stroke_color: Hsv,
	pub stroke_radius: Vx,
	pub view: Tracked<View>,
	pub images: Vec<Tracked<Image>>,
	pub strokes: Vec<Tracked<Stroke>>,
	// Tracks the smallest indices of an invalidated image/stroke.
	pub base_dirty_image_index: usize,
	pub base_dirty_stroke_index: usize,
	retractions: Vec<Retraction>,
	operations: Vec<Operation>,
	pub textures: Vec<Texture>,
	pub retraction_count_at_save: Option<usize>,
	pub selection_transformation: Tracked<SelectionTransformation>,
}

impl Canvas {
	pub fn new(config: &Config) -> Self {
		Self {
			file_path: None.into(),
			background_color: config.default_canvas_color,
			stroke_color: config.default_stroke_color.to_hsv(),
			stroke_radius: config.default_stroke_radius,
			view: View::new().into(),
			images: Vec::new(),
			strokes: Vec::new(),
			base_dirty_image_index: 0,
			base_dirty_stroke_index: 0,
			retractions: Vec::new(),
			operations: Vec::new(),
			textures: Vec::new(),
			retraction_count_at_save: None,
			selection_transformation: Default::default(),
		}
	}

	#[allow(clippy::too_many_arguments)]
	pub fn from_file(file_path: PathBuf, background_color: Srgb8, stroke_color: Srgb8, stroke_radius: Vx, view: View, images: Vec<Tracked<Image>>, strokes: Vec<Tracked<Stroke>>, textures: Vec<Texture>) -> Self {
		Self {
			file_path: Some(file_path).into(),
			background_color,
			stroke_color: stroke_color.to_hsv(),
			stroke_radius,
			view: view.into(),
			images,
			strokes,
			base_dirty_image_index: 0,
			base_dirty_stroke_index: 0,
			retractions: Vec::new(),
			operations: Vec::new(),
			textures,
			retraction_count_at_save: Some(0),
			selection_transformation: Default::default(),
		}
	}

	pub fn invalidate(&mut self) {
		self.view.invalidate();
		self.base_dirty_image_index = 0;
		self.base_dirty_stroke_index = 0;
	}

	pub fn images(&self) -> &[Tracked<Image>] {
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
						antitone_index_image_pairs.push((index, image.take()));
					}

					if let Some(index) = monotone_image_indices.first() {
						self.base_dirty_image_index = self.base_dirty_image_index.min(*index);
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
						debug_assert!(index <= self.images.len());
						self.images.insert(index, image.into());
						monotone_image_indices.push(index);
					}

					if let Some(index) = monotone_image_indices.first() {
						self.base_dirty_image_index = self.base_dirty_image_index.min(*index);
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
		let selection_center = ((max + min) / 2.).rotate(tilt) + screen_center;
		let selection_semidimensions = (max - min) / 2.;
		let alpha = selection_corners[1] - selection_corners[0];
		let beta = selection_corners[3] - selection_corners[0];
		let alpha_hat = alpha.normalized();
		let beta_hat = beta.normalized();
		for image in self.images.iter_mut() {
			let image_corners = [-image.dimensions, image.dimensions.flip::<1>(), image.dimensions, image.dimensions.flip::<0>()].map(|v| ((v * 0.5).rotate(image.orientation) * image.dilation) + image.position);
			let image_semidimensions = image.dimensions * 0.5 * image.dilation;
			let gamma_hat = (image_corners[1] - image_corners[0]).normalized();
			let delta_hat = (image_corners[3] - image_corners[0]).normalized();

			let no_overlap = [alpha_hat, beta_hat].into_iter().enumerate().any(|(i, axis)| {
				let projected_image_corners = image_corners.map(|corner| (corner - selection_center).dot(axis));
				projected_image_corners.iter().all(|corner| corner <= &-selection_semidimensions[i]) || projected_image_corners.iter().all(|corner| corner >= &selection_semidimensions[i])
			}) || [gamma_hat, delta_hat].into_iter().enumerate().any(|(i, axis)| {
				let projected_selection_corners = selection_corners.map(|corner| (corner - image.position).dot(axis));
				projected_selection_corners.iter().all(|corner| corner <= &-image_semidimensions[i]) || projected_selection_corners.iter().all(|corner| corner >= &image_semidimensions[i])
			});

			if should_aggregate {
				image.is_selected ^= !no_overlap;
			} else {
				image.is_selected = !no_overlap;
			}
		}

		'strokes: for stroke in self.strokes.iter_mut() {
			for point in stroke.points.iter() {
				let point_position = (stroke.position + point.position.rotate(stroke.orientation) * stroke.dilation - screen_center).rotate(-tilt);
				if point_position[0] >= min[0] && point_position[1] >= min[1] && point_position[0] <= max[0] && point_position[1] <= max[1] {
					stroke.is_selected = !should_aggregate || !stroke.is_selected;
					continue 'strokes;
				}
			}
			if !should_aggregate && stroke.is_selected {
				stroke.is_selected = false;
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

	pub fn push_texture(&mut self, renderer: &Renderer, dimensions: [NonZeroU32; 2], image: Vec<u8>) -> usize {
		self.textures.push(renderer.create_texture(dimensions, image));
		self.textures.len() - 1
	}
}
