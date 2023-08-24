// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{borrow::Cow, ops::Range};

use super::{dynamic_buffer::DynamicBuffer, dynamic_storage_buffer::DynamicStorageBuffer, uniform_buffer::UniformBuffer, vertex_attributes::VertexAttributes, ViewportUniform};
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
	pub selection_transformation_uniform_buffer: UniformBuffer<SelectionTransformation>,

	// Regeneration buffers.
	generated_vertices: Vec<StrokeVertex>,
	generated_indices: Vec<u32>,
	generated_extensions: Vec<StrokeExtension>,
}

impl StrokeRenderer {
	pub fn new<'a>(device: &wgpu::Device, texture_format: wgpu::TextureFormat, shader_source: impl Into<Cow<'a, str>>, vertex_main: &str, fragment_main: &str, viewport_buffer: &UniformBuffer<ViewportUniform>, sample_count: u32) -> Self {
		let vertex_buffer = DynamicBuffer::<StrokeVertex>::new(&device, wgpu::BufferUsages::VERTEX, 1 << 16);
		let index_buffer = DynamicBuffer::<u32>::new(&device, wgpu::BufferUsages::INDEX, 1 << 16);
		let extension_storage_buffer = DynamicStorageBuffer::<StrokeExtension>::new(&device, 1 << 16);
		let selection_transformation_uniform_buffer = UniformBuffer::new(device, 0, Default::default());

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
				buffers: &[StrokeVertex::buffer_layout(wgpu::VertexStepMode::Vertex)],
			},
			fragment: Some(wgpu::FragmentState {
				module: &shader_module,
				entry_point: fragment_main,
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
			selection_transformation_uniform_buffer,
			generated_vertices: Vec::new(),
			generated_indices: Vec::new(),
			generated_extensions: Vec::new(),
		}
	}

	pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, canvas: &mut Canvas, current_stroke: Option<&IncompleteStroke>) -> Range<u32> {
		// First, we update the selection transformation uniform if necessary.
		if let Some(selection_transformation) = canvas.selection_transformation.read_if_dirty() {
			self.selection_transformation_uniform_buffer.write(queue, *selection_transformation);
		}

		// Then, we iterate through the uninvalidated strokes and update their extensions if necessary.
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
						is_selected: if stroke.is_selected { 1. } else { 0. },
					}],
				);
			}
			vertex_offset += stroke.vertices.len();
			index_offset += stroke.relative_indices.len();
		}

		// Then, we iterate through the invalidated strokes and generate everything: vertices/indices/extensions.
		let extension_offset = canvas.base_dirty_stroke_index;

		let invalidated_strokes = canvas.strokes[extension_offset..].as_mut();

		self.generated_vertices.clear();
		self.generated_indices.clear();
		self.generated_extensions.clear();
		self.generated_extensions.reserve(invalidated_strokes.len());

		for (i, invalidated_stroke) in invalidated_strokes.iter_mut().map(Tracked::read).enumerate() {
			let current_extension_index = (extension_offset + i) as u32;
			let current_index_base = (vertex_offset + self.generated_vertices.len()) as u32;
			let lrgba = invalidated_stroke.color.to_lrgba();
			let color = [0, 1, 2].map(|n: usize| lrgba.0[n]);
			self.generated_vertices.extend(invalidated_stroke.vertices.iter().map(|(position, polarity)| StrokeVertex {
				position: position.0,
				polarity: *polarity,
				extension_index: current_extension_index,
			}));
			self.generated_indices.extend(invalidated_stroke.relative_indices.iter().map(|n| current_index_base + n));
			self.generated_extensions.push(StrokeExtension {
				translation: invalidated_stroke.position.0,
				rotation: invalidated_stroke.orientation,
				dilation: invalidated_stroke.dilation,
				color,
				is_selected: if invalidated_stroke.is_selected { 1. } else { 0. },
			});
		}

		// In addition, we append the generated vertices/indices/extension of the current stroke to the regeneration buffers.
		if let Some(current_stroke) = current_stroke {
			let stroke = current_stroke.preview();
			let current_extension_index = (extension_offset + invalidated_strokes.len()) as u32;
			let current_index_offset = (vertex_offset + self.generated_vertices.len()) as u32;
			let lrgba = stroke.color.to_lrgba();
			let color = [0, 1, 2].map(|n: usize| lrgba.0[n]);
			self.generated_vertices.extend(stroke.vertices.iter().map(|(position, polarity)| StrokeVertex {
				position: position.0,
				polarity: *polarity,
				extension_index: current_extension_index,
			}));
			self.generated_indices.extend(stroke.relative_indices.iter().map(|n| current_index_offset + n));
			self.generated_extensions.push(StrokeExtension {
				translation: stroke.position.0,
				rotation: stroke.orientation,
				dilation: stroke.dilation,
				color,
				is_selected: if stroke.is_selected { 1. } else { 0. },
			});
		}

		// Finally, we write the regeneration buffers to the device buffers.
		self.vertex_buffer.write(device, queue, vertex_offset, &self.generated_vertices);
		self.index_buffer.write(device, queue, index_offset, &self.generated_indices);
		self.extension_storage_buffer.write(device, queue, extension_offset, &self.generated_extensions);

		// We mark the entire stroke array as uninvalidated.
		canvas.base_dirty_stroke_index = canvas.strokes.len();

		// We return the range of indices to be rendered.
		0..(self.generated_indices.len() + index_offset) as u32
	}

	// Precondition: bind group 0 is set to the viewport.
	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, index_range: Range<u32>) {
		render_pass.set_pipeline(&self.render_pipeline);
		self.selection_transformation_uniform_buffer.activate(render_pass, 1);
		render_pass.set_bind_group(2, &self.extension_storage_buffer.bind_group, &[]);
		render_pass.set_vertex_buffer(0, self.vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.draw_indexed(index_range, 0, 0..1)
	}
}
