// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{borrow::Cow, ops::Range};

use super::{dynamic_buffer::DynamicBuffer, dynamic_storage_buffer::DynamicStorageBuffer, instance_renderer::InstanceRenderer, texture::Texture, uniform_buffer::UniformBuffer, vertex_attributes::VertexAttributes, ViewportUniform};
use crate::{
	canvas::{Canvas, IncompleteStroke},
	utility::{Tracked, Vex, Vx, Zero},
};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SelectionTransformation {
	pub translation: Vex<2, Vx>,
	pub center_of_transformation: Vex<2, Vx>,
	pub rotation: f32,
	pub dilation: f32,
}

impl Default for SelectionTransformation {
	fn default() -> Self {
		Self {
			translation: Vex::ZERO,
			center_of_transformation: Vex::ZERO,
			rotation: 0.,
			dilation: 1.,
		}
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageInstance {
	pub position: Vex<2, Vx>,
	pub orientation: f32,
	pub dilation: f32,
	pub dimensions: Vex<2, Vx>,
	pub sprite_position: [f32; 2],
	pub sprite_dimensions: [f32; 2],
	pub is_selected: f32,
}

impl VertexAttributes<7> for ImageInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x2, 4 => Float32x2, 5 => Float32x2, 6 => Float32,];
}

pub struct CanvasRenderer {
	pub selection_transformation_uniform_buffer: UniformBuffer<SelectionTransformation>,
	image_instance_renderer: InstanceRenderer<ImageInstance>,
	stroke_renderer: StrokeRenderer,
	image_instance_assembly: Vec<ImageInstance>,
}

impl CanvasRenderer {
	pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat, viewport_buffer: &UniformBuffer<ViewportUniform>, sample_count: u32) -> Self {
		let selection_transformation_uniform_buffer = UniformBuffer::new(device, 0, Default::default());

		Self {
			image_instance_renderer: InstanceRenderer::new(
				device,
				texture_format,
				include_str!("shaders/canvas_image.wgsl"),
				"vs_main",
				"fs_main",
				&[&viewport_buffer.bind_group_layout, &selection_transformation_uniform_buffer.bind_group_layout, &Texture::bind_group_layout(device)],
				sample_count,
			),
			stroke_renderer: StrokeRenderer::new(
				device,
				texture_format,
				include_str!("shaders/stroke_trigon.wgsl"),
				"vs_main",
				"fs_main",
				viewport_buffer,
				&selection_transformation_uniform_buffer,
				sample_count,
			),
			selection_transformation_uniform_buffer,
			image_instance_assembly: Vec::new(),
		}
	}

	pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, canvas: &mut Canvas, current_stroke: Option<&IncompleteStroke>) -> (Vec<usize>, Range<u32>) {
		// We update the selection transformation uniform if necessary.
		if let Some(selection_transformation) = canvas.selection_transformation.read_if_dirty() {
			self.selection_transformation_uniform_buffer.write(queue, *selection_transformation);
		}

		let mut image_texture_indices = Vec::new();

		// Then, we iterate through the uninvalidated images and update their instances one at at time, only if necessary.
		let instance_offset = canvas.base_dirty_image_index.min(canvas.images.len());
		for (i, image) in canvas.images[0..instance_offset].iter_mut().enumerate() {
			let image_texture_index = if let Some(image) = image.read_if_dirty() {
				if let Some(texture) = canvas.textures.get(image.texture_index) {
					self.image_instance_renderer.prepare(
						device,
						queue,
						i,
						&[ImageInstance {
							position: image.position,
							orientation: image.orientation,
							dilation: image.dilation,
							dimensions: image.dimensions,
							sprite_position: [0.; 2],
							sprite_dimensions: [texture.extent.width as f32, texture.extent.height as f32],
							is_selected: if image.is_selected { 1. } else { 0. },
						}],
					)
				}
				image.texture_index
			} else {
				image.read().texture_index
			};
			image_texture_indices.push(image_texture_index);
		}

		// Next, we iterate through the invalidated images and update their instances in one go.
		let invalidated_images = canvas.images[instance_offset..].as_mut();

		self.image_instance_assembly.clear();

		for image in invalidated_images.iter_mut().map(Tracked::read) {
			let texture = &canvas.textures[image.texture_index];

			self.image_instance_assembly.push(ImageInstance {
				position: image.position,
				orientation: image.orientation,
				dilation: image.dilation,
				dimensions: image.dimensions,
				sprite_position: [0.; 2],
				sprite_dimensions: [texture.extent.width as f32, texture.extent.height as f32],
				is_selected: image.is_selected as u8 as _,
			});

			image_texture_indices.push(image.texture_index);
		}

		self.image_instance_renderer.prepare(device, queue, instance_offset, &self.image_instance_assembly);

		// We mark the entire image array as uninvalidated.
		canvas.base_dirty_image_index = canvas.images.len();

		// Finally, we prepare the stroke renderer.
		let stroke_index_range = self.stroke_renderer.prepare(device, queue, canvas, current_stroke);

		(image_texture_indices, stroke_index_range)
	}

	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, textures: &'r [Texture], (image_texture_indices, stroke_index_range): (Vec<usize>, Range<u32>)) {
		self.selection_transformation_uniform_buffer.activate(render_pass, 1);

		for (i, texture_index) in image_texture_indices.iter().copied().enumerate() {
			if let Some(texture) = textures.get(texture_index) {
				texture.activate(render_pass, 2);
				self.image_instance_renderer.render(render_pass, i as _..i as u32 + 1);
			}
		}

		self.stroke_renderer.render(render_pass, stroke_index_range);
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StrokeExtension {
	pub translation: [Vx; 2],
	pub rotation: f32,
	pub dilation: f32,
	pub color: [f32; 3],
	pub is_selected: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StrokeVertex {
	pub position: [Vx; 2],
	pub polarity: f32,
	pub extension_index: u32,
}

impl VertexAttributes<3> for StrokeVertex {
	const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Uint32];
}

pub struct StrokeRenderer {
	render_pipeline: wgpu::RenderPipeline,
	vertex_buffer: DynamicBuffer<StrokeVertex>,
	index_buffer: DynamicBuffer<u32>,
	extension_storage_buffer: DynamicStorageBuffer<StrokeExtension>,
	vertex_assembly: Vec<StrokeVertex>,
	index_assembly: Vec<u32>,
	extension_assembly: Vec<StrokeExtension>,
}

impl StrokeRenderer {
	pub fn new<'a>(
		device: &wgpu::Device,
		texture_format: wgpu::TextureFormat,
		shader_source: impl Into<Cow<'a, str>>,
		vertex_main: &str,
		fragment_main: &str,
		viewport_buffer: &UniformBuffer<ViewportUniform>,
		selection_transformation_uniform_buffer: &UniformBuffer<SelectionTransformation>,
		sample_count: u32,
	) -> Self {
		let vertex_buffer = DynamicBuffer::<StrokeVertex>::new(device, wgpu::BufferUsages::VERTEX, 1 << 16);
		let index_buffer = DynamicBuffer::<u32>::new(device, wgpu::BufferUsages::INDEX, 1 << 16);
		let extension_storage_buffer = DynamicStorageBuffer::<StrokeExtension>::new(device, 1 << 16);

		let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: None,
			source: wgpu::ShaderSource::Wgsl(shader_source.into()),
		});

		let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: None,
			bind_group_layouts: &[&viewport_buffer.bind_group_layout, &selection_transformation_uniform_buffer.bind_group_layout, &extension_storage_buffer.bind_group_layout],
			push_constant_ranges: &[],
		});

		let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: None,
			layout: Some(&pipeline_layout),
			vertex: wgpu::VertexState {
				module: &shader_module,
				entry_point: vertex_main,
				compilation_options: Default::default(),
				buffers: &[StrokeVertex::buffer_layout(wgpu::VertexStepMode::Vertex)],
			},
			fragment: Some(wgpu::FragmentState {
				module: &shader_module,
				entry_point: fragment_main,
				compilation_options: Default::default(),
				targets: &[Some(wgpu::ColorTargetState {
					format: texture_format,
					blend: Some(wgpu::BlendState::ALPHA_BLENDING),
					write_mask: wgpu::ColorWrites::ALL,
				})],
			}),
			primitive: wgpu::PrimitiveState {
				topology: wgpu::PrimitiveTopology::TriangleList,
				strip_index_format: None,
				front_face: wgpu::FrontFace::Ccw,
				cull_mode: None,
				polygon_mode: wgpu::PolygonMode::Fill,
				unclipped_depth: false,
				conservative: false,
			},
			depth_stencil: None,
			multisample: wgpu::MultisampleState {
				count: sample_count,
				mask: !0,
				alpha_to_coverage_enabled: false,
			},
			multiview: None,
		});

		Self {
			render_pipeline,
			vertex_buffer,
			index_buffer,
			extension_storage_buffer,
			vertex_assembly: Vec::new(),
			index_assembly: Vec::new(),
			extension_assembly: Vec::new(),
		}
	}

	pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, canvas: &mut Canvas, current_stroke: Option<&IncompleteStroke>) -> Range<u32> {
		// First, we iterate through the uninvalidated strokes and update their extensions if necessary.
		let mut vertex_offset = 0;
		let mut index_offset = 0;
		for (i, stroke) in canvas.strokes[0..canvas.base_dirty_stroke_index].iter_mut().enumerate() {
			if let Some(stroke) = stroke.read_if_dirty() {
				let lrgba = stroke.color.to_lrgba();
				let color = [0, 1, 2].map(|n: usize| lrgba.0[n]);
				self.extension_storage_buffer.write(
					device,
					queue,
					i,
					&[StrokeExtension {
						translation: stroke.position.0,
						rotation: stroke.orientation,
						dilation: stroke.dilation,
						color,
						is_selected: stroke.is_selected as u8 as _,
					}],
				);
			}
			vertex_offset += stroke.vertices.len();
			index_offset += stroke.relative_indices.len();
		}

		// Then, we iterate through the invalidated strokes and generate everything: vertices/indices/extensions.
		let extension_offset = canvas.base_dirty_stroke_index;

		let invalidated_strokes = canvas.strokes[extension_offset..].as_mut();

		self.vertex_assembly.clear();
		self.index_assembly.clear();
		self.extension_assembly.clear();
		self.extension_assembly.reserve(invalidated_strokes.len());

		for (i, invalidated_stroke) in invalidated_strokes.iter_mut().map(Tracked::read).enumerate() {
			let current_extension_index = (extension_offset + i) as u32;
			let current_index_base = (vertex_offset + self.vertex_assembly.len()) as u32;
			let lrgba = invalidated_stroke.color.to_lrgba();
			let color = [0, 1, 2].map(|n: usize| lrgba.0[n]);
			self.vertex_assembly.extend(invalidated_stroke.vertices.iter().map(|(position, polarity)| StrokeVertex {
				position: position.0,
				polarity: *polarity,
				extension_index: current_extension_index,
			}));
			self.index_assembly.extend(invalidated_stroke.relative_indices.iter().map(|n| current_index_base + n));
			self.extension_assembly.push(StrokeExtension {
				translation: invalidated_stroke.position.0,
				rotation: invalidated_stroke.orientation,
				dilation: invalidated_stroke.dilation,
				color,
				is_selected: if invalidated_stroke.is_selected { 1. } else { 0. },
			});
		}

		// In addition, we append the generated vertices/indices/extension of the current stroke to the assembly buffers.
		if let Some(current_stroke) = current_stroke {
			let stroke = current_stroke.preview();
			let current_extension_index = (extension_offset + invalidated_strokes.len()) as u32;
			let current_index_offset = (vertex_offset + self.vertex_assembly.len()) as u32;
			let lrgba = stroke.color.to_lrgba();
			let color = [0, 1, 2].map(|n: usize| lrgba.0[n]);
			self.vertex_assembly.extend(stroke.vertices.iter().map(|(position, polarity)| StrokeVertex {
				position: position.0,
				polarity: *polarity,
				extension_index: current_extension_index,
			}));
			self.index_assembly.extend(stroke.relative_indices.iter().map(|n| current_index_offset + n));
			self.extension_assembly.push(StrokeExtension {
				translation: stroke.position.0,
				rotation: stroke.orientation,
				dilation: stroke.dilation,
				color,
				is_selected: if stroke.is_selected { 1. } else { 0. },
			});
		}

		// Finally, we write the assembly buffers to the device buffers.
		self.vertex_buffer.write(device, queue, vertex_offset, &self.vertex_assembly);
		self.index_buffer.write(device, queue, index_offset, &self.index_assembly);
		self.extension_storage_buffer.write(device, queue, extension_offset, &self.extension_assembly);

		// We mark the entire stroke array as uninvalidated.
		canvas.base_dirty_stroke_index = canvas.strokes.len();

		// We return the range of indices to be rendered.
		0..(self.index_assembly.len() + index_offset) as u32
	}

	// Precondition: bind group 0 is set to the viewport.
	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, index_range: Range<u32>) {
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_bind_group(2, &self.extension_storage_buffer.bind_group, &[]);
		render_pass.set_vertex_buffer(0, self.vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.draw_indexed(index_range, 0, 0..1)
	}
}
