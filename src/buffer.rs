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
	size: u64,
	usage: wgpu::BufferUsages,
	pub buffer: wgpu::Buffer,
}

impl<T> DynamicBuffer<T> {
	pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, mut capacity: u64) -> Self {
		while (std::mem::size_of::<T>() as u64 * capacity & wgpu::COPY_BUFFER_ALIGNMENT as u64 - 1) != 0 {
			capacity = (capacity + 1).next_power_of_two();
		}
		let size = std::mem::size_of::<T>() as u64 * capacity;
		let usage = usage | wgpu::BufferUsages::COPY_DST;
		Self {
			_base: PhantomData,
			size,
			usage,
			buffer: device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size,
				usage,
				mapped_at_creation: false,
			}),
		}
	}

	pub fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mut source: Vec<T>, default: T)
	where
		T: Clone + Pod,
	{
		let mut new_source_len = source.len();
		while (std::mem::size_of::<T>() * new_source_len & wgpu::COPY_BUFFER_ALIGNMENT as usize - 1) != 0 {
			new_source_len = (new_source_len + 1).next_power_of_two();
		}

		source.resize(new_source_len, default);

		if self.size < (std::mem::size_of::<T>() * source.len()) as u64 {
			self.size = (std::mem::size_of::<T>() * source.len()).next_power_of_two() as u64;
			self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
				label: None,
				size: self.size,
				usage: self.usage,
				mapped_at_creation: false,
			});
		}

		queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&source));
	}
}
