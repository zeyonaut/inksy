// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub struct Texture {
	rgba: Option<Vec<u8>>,
	texture: wgpu::Texture,
	pub texture_size: wgpu::Extent3d,
	bind_group: wgpu::BindGroup,
}

fn create_bind_group(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, dimensions: [u32; 2]) -> (wgpu::Texture, wgpu::Extent3d, wgpu::BindGroup) {
	let texture_size = wgpu::Extent3d {
		width: dimensions[0],
		height: dimensions[1],
		depth_or_array_layers: 1,
	};
	let texture = device.create_texture(&wgpu::TextureDescriptor {
		label: None,
		size: texture_size,
		mip_level_count: 1,
		sample_count: 1,
		dimension: wgpu::TextureDimension::D2,
		format: wgpu::TextureFormat::Rgba8UnormSrgb,
		usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		view_formats: &[],
	});
	let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
	let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
		label: None,
		layout: &bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding: 0,
				resource: wgpu::BindingResource::TextureView(&texture_view),
			},
			wgpu::BindGroupEntry {
				binding: 1,
				resource: wgpu::BindingResource::Sampler(&sampler),
			},
		],
	});
	(texture, texture_size, bind_group)
}

impl Texture {
	pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
		device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: None,
			entries: &[
				wgpu::BindGroupLayoutEntry {
					binding: 0,
					visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
					ty: wgpu::BindingType::Texture {
						multisampled: false,
						view_dimension: wgpu::TextureViewDimension::D2,
						sample_type: wgpu::TextureSampleType::Float { filterable: true },
					},
					count: None,
				},
				wgpu::BindGroupLayoutEntry {
					binding: 1,
					visibility: wgpu::ShaderStages::FRAGMENT,
					ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
					count: None,
				},
			],
		})
	}

	pub fn new(device: &wgpu::Device, dimensions: [u32; 2], image: Vec<u8>, bind_group_layout: &wgpu::BindGroupLayout) -> Self {
		let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
			address_mode_u: wgpu::AddressMode::ClampToEdge,
			address_mode_v: wgpu::AddressMode::ClampToEdge,
			address_mode_w: wgpu::AddressMode::ClampToEdge,
			mag_filter: wgpu::FilterMode::Linear,
			min_filter: wgpu::FilterMode::Linear,
			mipmap_filter: wgpu::FilterMode::Linear,
			..Default::default()
		});
		let (texture, texture_size, bind_group) = create_bind_group(device, &bind_group_layout, &sampler, dimensions);
		Self {
			rgba: Some(image),
			texture,
			texture_size,
			bind_group,
		}
	}

	pub fn prepare(&mut self, queue: &wgpu::Queue) {
		if let Some(rgba) = self.rgba.take() {
			queue.write_texture(
				wgpu::ImageCopyTexture {
					texture: &self.texture,
					mip_level: 0,
					origin: wgpu::Origin3d::ZERO,
					aspect: wgpu::TextureAspect::All,
				},
				&rgba,
				wgpu::ImageDataLayout {
					offset: 0,
					bytes_per_row: Some(4 * self.texture_size.width),
					rows_per_image: Some(self.texture_size.height),
				},
				self.texture_size,
			);
		}
	}

	pub fn activate<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, group_index: u32) {
		render_pass.set_bind_group(group_index, &self.bind_group, &[]);
	}
}
