// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::marker::PhantomData;

use bytemuck::Pod;

// A growable buffer.
pub struct DynamicBuffer<T> {
	_base: PhantomData<T>,
	pub buffer: wgpu::Buffer,
}

impl<T> DynamicBuffer<T> {
	pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, mut capacity: u64) -> Self {
		while (std::mem::size_of::<T>() as u64 * capacity & wgpu::COPY_BUFFER_ALIGNMENT as u64 - 1) != 0 {
			capacity = (capacity + 1).next_power_of_two();
		}
		let size = std::mem::size_of::<T>() as u64 * capacity;
		let usage = usage | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
		Self {
			_base: PhantomData,
			buffer: device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size,
				usage,
				mapped_at_creation: false,
			}),
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

			let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
			encoder.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, self.buffer.size());
			queue.submit(Some(encoder.finish()));

			self.buffer = buffer;
		}

		queue.write_buffer(&self.buffer, (std::mem::size_of::<T>() * offset) as wgpu::BufferAddress, bytemuck::cast_slice(&source));
	}
}
