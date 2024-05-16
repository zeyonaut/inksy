// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{borrow::Cow, ops::Range};

use wgpu::util::DeviceExt;

use super::{dynamic_buffer::DynamicBuffer, vertex_attributes::VertexAttributes};

pub struct InstanceRenderer<Instance> {
	render_pipeline: wgpu::RenderPipeline,
	instance_buffer: DynamicBuffer<Instance>,
	index_buffer: wgpu::Buffer,
	index_range: Range<u32>,
}

impl<Instance> InstanceRenderer<Instance> {
	pub fn new<'a, const N: usize>(device: &wgpu::Device, texture_format: wgpu::TextureFormat, shader_source: impl Into<Cow<'a, str>>, vertex_main: &str, fragment_main: &str, bind_group_layouts: &[&wgpu::BindGroupLayout], sample_count: u32) -> Self
	where
		Instance: VertexAttributes<N>,
	{
		let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: None,
			source: wgpu::ShaderSource::Wgsl(shader_source.into()),
		});

		let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: None,
			bind_group_layouts,
			push_constant_ranges: &[],
		});

		let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: None,
			layout: Some(&pipeline_layout),
			vertex: wgpu::VertexState {
				module: &shader_module,
				entry_point: vertex_main,
				compilation_options: Default::default(),
				buffers: &[Instance::buffer_layout(wgpu::VertexStepMode::Instance)],
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
				front_face: wgpu::FrontFace::Cw,
				cull_mode: Some(wgpu::Face::Back),
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

		let instance_buffer = DynamicBuffer::<Instance>::new(device, wgpu::BufferUsages::VERTEX, 1 << 0);

		const RECT_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

		let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: None,
			contents: bytemuck::cast_slice(RECT_INDICES),
			usage: wgpu::BufferUsages::INDEX,
		});

		let index_range = 0..RECT_INDICES.len() as u32;

		Self {
			render_pipeline,
			instance_buffer,
			index_buffer,
			index_range,
		}
	}

	pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, offset: usize, instances: &[Instance])
	where
		Instance: bytemuck::Pod,
	{
		self.instance_buffer.write(device, queue, offset, instances);
	}

	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, instance_range: Range<u32>) {
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_vertex_buffer(0, self.instance_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(self.index_range.clone(), 0, instance_range);
	}
}
