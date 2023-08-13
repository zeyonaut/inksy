// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::marker::PhantomData;

use wgpu::util::DeviceExt;

pub struct UniformBuffer<Uniform> {
	buffer: wgpu::Buffer,
	bind_group: wgpu::BindGroup,
	pub bind_group_layout: wgpu::BindGroupLayout,
	_phantom_data: PhantomData<Uniform>,
}

impl<Uniform> UniformBuffer<Uniform> {
	pub fn new(device: &wgpu::Device, binding: u32, contents: Uniform) -> Self
	where
		Uniform: bytemuck::Pod,
	{
		let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: None,
			contents: bytemuck::cast_slice(&[contents]),
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		});

		let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: None,
			entries: &[wgpu::BindGroupLayoutEntry {
				binding,
				visibility: wgpu::ShaderStages::VERTEX,
				ty: wgpu::BindingType::Buffer {
					ty: wgpu::BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: None,
				},
				count: None,
			}],
		});

		let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			label: None,
			layout: &bind_group_layout,
			entries: &[wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }],
		});

		Self {
			buffer,
			bind_group_layout,
			bind_group,
			_phantom_data: PhantomData,
		}
	}

	pub fn write(&self, queue: &wgpu::Queue, uniform: Uniform)
	where
		Uniform: bytemuck::Pod,
	{
		queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
	}

	pub fn activate<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, group_index: u32) {
		render_pass.set_bind_group(group_index, &self.bind_group, &[]);
	}
}
