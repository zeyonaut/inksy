// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::marker::PhantomData;

use bytemuck::Pod;

// A growable storage buffer and bind group.
pub struct DynamicStorageBuffer<T> {
	_base: PhantomData<T>,
	pub buffer: wgpu::Buffer,
	pub bind_group_layout: wgpu::BindGroupLayout,
	pub bind_group: wgpu::BindGroup,
}

impl<T> DynamicStorageBuffer<T> {
	pub fn new(device: &wgpu::Device, mut capacity: u64) -> Self {
		while ((std::mem::size_of::<T>() as u64 * capacity) & (wgpu::COPY_BUFFER_ALIGNMENT - 1)) != 0 {
			capacity = (capacity + 1).next_power_of_two();
		}
		let size = std::mem::size_of::<T>() as u64 * capacity;
		let usage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
		let buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: None,
			size,
			usage,
			mapped_at_creation: false,
		});
		let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: None,
			entries: &[wgpu::BindGroupLayoutEntry {
				binding: 0,
				visibility: wgpu::ShaderStages::VERTEX,
				ty: wgpu::BindingType::Buffer {
					ty: wgpu::BufferBindingType::Storage { read_only: true },
					has_dynamic_offset: false,
					min_binding_size: None,
				},
				count: None,
			}],
		});
		let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			label: None,
			layout: &bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding: 0,
				resource: wgpu::BindingResource::Buffer(buffer.as_entire_buffer_binding()),
			}],
		});

		Self {
			_base: PhantomData,
			buffer,
			bind_group_layout,
			bind_group,
		}
	}

	pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, offset: usize, source: &[T])
	where
		T: Clone + Pod,
	{
		if self.buffer.size() < (std::mem::size_of::<T>() * (offset + source.len())) as wgpu::BufferAddress {
			let buffer = device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size: (std::mem::size_of::<T>() * (offset + source.len())).next_power_of_two() as u64,
				usage: self.buffer.usage(),
				mapped_at_creation: false,
			});
			self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
				label: None,
				layout: &self.bind_group_layout,
				entries: &[wgpu::BindGroupEntry {
					binding: 0,
					resource: wgpu::BindingResource::Buffer(buffer.as_entire_buffer_binding()),
				}],
			});

			let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
			encoder.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, self.buffer.size());
			queue.submit(Some(encoder.finish()));

			self.buffer = buffer;
		}

		queue.write_buffer(&self.buffer, (std::mem::size_of::<T>() * offset) as wgpu::BufferAddress, bytemuck::cast_slice(source));
	}
}
