// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{borrow::Cow, ops::Range};

use super::{dynamic_buffer::DynamicBuffer, uniform_buffer::UniformBuffer, vertex_attributes::VertexAttributes, ViewportUniform};

pub struct VertexRenderer<Vertex> {
	render_pipeline: wgpu::RenderPipeline,
	vertex_buffer: DynamicBuffer<Vertex>,
	index_buffer: DynamicBuffer<u32>,
}

impl<Vertex> VertexRenderer<Vertex> {
	pub fn new<'a, const N: usize>(device: &wgpu::Device, texture_format: wgpu::TextureFormat, shader_source: impl Into<Cow<'a, str>>, vertex_main: &str, fragment_main: &str, viewport_buffer: &UniformBuffer<ViewportUniform>, sample_count: u32) -> Self
	where
		Vertex: VertexAttributes<N>,
	{
		let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: None,
			source: wgpu::ShaderSource::Wgsl(shader_source.into()),
		});

		let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: None,
			bind_group_layouts: &[&viewport_buffer.bind_group_layout],
			push_constant_ranges: &[],
		});

		let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: None,
			layout: Some(&pipeline_layout),
			vertex: wgpu::VertexState {
				module: &shader_module,
				entry_point: vertex_main,
				compilation_options: Default::default(),
				buffers: &[Vertex::buffer_layout(wgpu::VertexStepMode::Vertex)],
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

		let vertex_buffer = DynamicBuffer::<Vertex>::new(device, wgpu::BufferUsages::VERTEX, 1 << 16);
		let index_buffer = DynamicBuffer::<u32>::new(device, wgpu::BufferUsages::INDEX, 1 << 16);

		Self { render_pipeline, vertex_buffer, index_buffer }
	}
	pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[Vertex], indices: &[u32])
	where
		Vertex: bytemuck::Pod,
	{
		self.vertex_buffer.write(device, queue, 0, vertices);
		self.index_buffer.write(device, queue, 0, indices);
	}

	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, index_range: Range<u32>) {
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_vertex_buffer(0, self.vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
		render_pass.draw_indexed(index_range, 0, 0..1);
	}
}
