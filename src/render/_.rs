// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod buffer;
mod instance_renderer;
mod uniform_buffer;
mod vertex_renderer;

use std::ops::Range;

use fast_srgb8::srgb8_to_f32;
use pollster::FutureExt;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use wgpu::SurfaceTexture;

use self::{
	instance_renderer::{InstanceAttributes, InstanceRenderer},
	uniform_buffer::UniformBuffer,
	vertex_renderer::{VertexAttributes, VertexRenderer},
};
use crate::pixel::{Px, Vex, Vx};

const SHOULD_MULTISAMPLE: bool = false;

pub enum DrawCommand {
	Trimesh { vertices: Vec<Vertex>, indices: Vec<u32> },
	Card { position: Vex<2, Px>, dimensions: Vex<2, Px>, color: [u8; 4], radius: Px },
	ColorSelector { position: Vex<2, Px>, hsv: [f32; 3], trigon_radius: Px, hole_radius: Px, ring_width: Px },
}

pub enum RenderCommand {
	Trimesh(Range<u32>),
	Card(Range<u32>),
	ColorRing(Range<u32>),
	ColorTrigon(Range<u32>),
}

// This struct stores the data of each vertex to be rendered.
#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
	pub position: [Vx; 3],
	pub color: [f32; 4],
}

impl VertexAttributes<2> for Vertex {
	const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewportUniform {
	pub position: [f32; 2],
	pub size: [f32; 2],
	pub scale: f32,
	pub tilt: f32,
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardInstance {
	pub position: [f32; 2],
	pub dimensions: [f32; 2],
	pub color: [f32; 4],
	pub depth: f32,
	pub radius: f32,
}

impl InstanceAttributes<5> for CardInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32, 4 => Float32];
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorRingInstance {
	pub position: [f32; 2],
	pub radius_major: f32,
	pub radius_minor: f32,
	pub depth: f32,
	pub saturation_value: [f32; 2],
}

impl InstanceAttributes<5> for ColorRingInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32, 4 => Float32x2];
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorTrigonInstance {
	pub position: [f32; 2],
	pub radius: f32,
	pub hue: f32,
	pub depth: f32,
}

impl InstanceAttributes<4> for ColorTrigonInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32];
}

// This struct stores the current state of the WGPU renderer.
pub struct Renderer {
	surface: wgpu::Surface,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	pub width: u32,
	pub height: u32,
	pub position: Vex<2, Vx>,
	pub zoom: f32,
	pub tilt: f32,
	pub scale_factor: f32,
	pub is_pending_resize: bool,
	pub clear_color: wgpu::Color,
	viewport_buffer: UniformBuffer<ViewportUniform>,
	trigon_renderer: VertexRenderer<Vertex>,
	card_renderer: InstanceRenderer<CardInstance>,
	color_ring_renderer: InstanceRenderer<ColorRingInstance>,
	color_trigon_renderer: InstanceRenderer<ColorTrigonInstance>,
	multisample_texture: Option<wgpu::Texture>,
	texture_format: wgpu::TextureFormat,
}

impl Renderer {
	// Create an instance of the renderer.
	pub fn new<W>(window: &W, position: Vex<2, Vx>, width: u32, height: u32, zoom: f32, tilt: f32, scale_factor: f32) -> Self
	where
		W: HasRawWindowHandle + HasRawDisplayHandle,
	{
		// We create a WGPU instance and a surface on our window to draw to.
		let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
			backends: wgpu::Backends::all(),
			dx12_shader_compiler: Default::default(),
		});
		let surface = unsafe { instance.create_surface(window) }.unwrap();

		// We request an adapter (a graphics card) that can draw to this surface.
		let adapter = instance
			.request_adapter(&wgpu::RequestAdapterOptions {
				power_preference: wgpu::PowerPreference::LowPower,
				compatible_surface: Some(&surface),
				force_fallback_adapter: false,
			})
			.block_on()
			.unwrap();

		// We use our adapter to create a device and queue.
		let (device, queue) = adapter
			.request_device(
				&wgpu::DeviceDescriptor {
					features: wgpu::Features::empty(),
					limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
					label: None,
				},
				None,
			)
			.block_on()
			.unwrap();

		// We define a configuration for our surface.
		// FIXME: Ensure dimensions are nonzero.
		let surface_capabilities = surface.get_capabilities(&adapter);

		let texture_format = surface_capabilities.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(*surface_capabilities.formats.first().unwrap());

		let config = wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
			format: texture_format,
			width,
			height,
			present_mode: *surface_capabilities.present_modes.first().unwrap(),
			alpha_mode: *surface_capabilities.alpha_modes.first().unwrap(),
			view_formats: vec![],
		};
		surface.configure(&device, &config);

		let multisample_texture = if adapter.get_texture_format_features(texture_format).flags.sample_count_supported(4) && SHOULD_MULTISAMPLE {
			Some(device.create_texture(&wgpu::TextureDescriptor {
				label: None,
				size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
				mip_level_count: 1,
				sample_count: 4,
				dimension: wgpu::TextureDimension::D2,
				format: texture_format,
				usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
				view_formats: vec![].as_slice(),
			}))
		} else {
			None
		};

		let viewport_buffer = UniformBuffer::new(
			&device,
			0,
			ViewportUniform {
				position: [0., 0.],
				size: [width as f32, height as f32],
				scale: zoom * scale_factor,
				tilt,
			},
		);

		let sample_count = multisample_texture.as_ref().map_or_else(|| 1, |_| 4);

		let trigon_renderer = VertexRenderer::new(&device, config.format, include_str!("shaders/trigon.wgsl"), "vs_main", "fs_main", &viewport_buffer, sample_count);
		let card_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/round_rectangle.wgsl"), "vs_main", "fs_main", &viewport_buffer, sample_count);
		let color_ring_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/color_picker_ring.wgsl"), "vs_main", "fs_main", &viewport_buffer, sample_count);
		let color_trigon_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/color_picker_trigon.wgsl"), "vs_main", "fs_main", &viewport_buffer, sample_count);

		// We return a new instance of our renderer state.
		Self {
			surface,
			device,
			queue,
			config,
			width,
			height,
			position,
			zoom,
			tilt,
			scale_factor,
			is_pending_resize: false,
			clear_color: wgpu::Color::BLACK,
			viewport_buffer,
			trigon_renderer,
			card_renderer,
			color_ring_renderer,
			color_trigon_renderer,
			multisample_texture,
			texture_format,
		}
	}

	// Resize the renderer to a requested size.
	pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
		// We ensure the requested size has nonzero dimensions before applying it.
		if width > 0 && height > 0 {
			self.width = width;
			self.height = height;
			self.config.width = width;
			self.config.height = height;
			self.scale_factor = scale_factor;
			self.surface.configure(&self.device, &self.config);
			self.is_pending_resize = true;
			if let Some(multisample_texture) = self.multisample_texture.as_mut() {
				*multisample_texture = self.device.create_texture(&wgpu::TextureDescriptor {
					label: None,
					size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
					mip_level_count: 1,
					sample_count: 4,
					dimension: wgpu::TextureDimension::D2,
					format: self.texture_format,
					usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
					view_formats: vec![].as_slice(),
				})
			}
		}
	}

	pub fn reposition(&mut self, position: Vex<2, Vx>) {
		if self.position != position {
			self.position = position;
			self.is_pending_resize = true;
		}
	}

	pub fn rezoom(&mut self, zoom: f32) {
		if self.zoom != zoom {
			self.zoom = zoom;
			self.is_pending_resize = true;
		}
	}

	pub fn retilt(&mut self, tilt: f32) {
		if self.tilt != tilt {
			self.tilt = tilt;
			self.is_pending_resize = true;
		}
	}

	pub fn update(&mut self) {}

	pub fn render(&mut self, draw_commands: Vec<DrawCommand>) -> Result<(), wgpu::SurfaceError> {
		if self.is_pending_resize {
			// We write the new size to the viewport buffer.
			self.viewport_buffer.write(
				&self.queue,
				ViewportUniform {
					position: self.position.0.map(Into::into),
					size: [self.width as f32, self.height as f32],
					scale: self.zoom * self.scale_factor,
					tilt: self.tilt,
				},
			);
			self.is_pending_resize = false;
		}

		let mut strokes_vertices: Vec<Vertex> = vec![];
		let mut strokes_indices: Vec<u32> = vec![];
		let mut card_instances: Vec<CardInstance> = vec![];
		let mut color_ring_instances: Vec<ColorRingInstance> = vec![];
		let mut color_trigon_instances: Vec<ColorTrigonInstance> = vec![];

		let mut render_commands: Vec<RenderCommand> = vec![];

		for draw_command in draw_commands {
			match draw_command {
				DrawCommand::Trimesh { mut vertices, indices } => {
					let vertex_start = strokes_vertices.len() as u32;
					let index_start = strokes_indices.len() as u32;
					strokes_vertices.append(&mut vertices);
					strokes_indices.extend(indices.into_iter().map(|i| vertex_start + i));
					render_commands.push(RenderCommand::Trimesh(index_start..strokes_indices.len() as u32));
				},
				DrawCommand::Card { position, dimensions, color, radius } => {
					let instance_start = card_instances.len() as u32;
					card_instances.push(CardInstance {
						position: position.0.map(|n| n.0),
						dimensions: dimensions.0.map(|n| n.0),
						color: color.map(srgb8_to_f32),
						depth: 0.,
						radius: radius.0,
					});
					render_commands.push(RenderCommand::Card(instance_start..instance_start + 1));
				},
				DrawCommand::ColorSelector {
					position,
					hsv,
					trigon_radius,
					hole_radius,
					ring_width,
				} => {
					let ring_instance_start = color_ring_instances.len() as u32;
					color_ring_instances.push(ColorRingInstance {
						position: position.0.map(|n| n.0),
						radius_major: (hole_radius + ring_width).0,
						radius_minor: hole_radius.0,
						depth: 0.,
						saturation_value: [hsv[1], hsv[2]],
					});
					render_commands.push(RenderCommand::ColorRing(ring_instance_start..ring_instance_start + 1));

					let trigon_instance_start = color_trigon_instances.len() as u32;
					color_trigon_instances.push(ColorTrigonInstance {
						position: position.map(|n| n + ring_width + hole_radius - trigon_radius).0.map(|n| n.0),
						radius: trigon_radius.0,
						hue: hsv[0],
						depth: 0.,
					});
					render_commands.push(RenderCommand::ColorTrigon(trigon_instance_start..trigon_instance_start + 1));
				},
			}
		}

		self.trigon_renderer.prepare(&self.device, &self.queue, strokes_vertices, strokes_indices);
		self.card_renderer.prepare(&self.device, &self.queue, card_instances);
		self.color_ring_renderer.prepare(&self.device, &self.queue, color_ring_instances);
		self.color_trigon_renderer.prepare(&self.device, &self.queue, color_trigon_instances);

		// Set up the surface texture we will later render to.
		let output = self.surface.get_current_texture()?;

		let output_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
		let multisample_view = self.multisample_texture.as_ref().map(|x| x.create_view(&wgpu::TextureViewDescriptor::default()));

		// Set up the command buffer we will later send to the GPU.
		let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

		// Add a render pass to the command buffer.
		// Here, we clear the color.
		let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
			label: Some("render_pass"),
			color_attachments: &[Some(wgpu::RenderPassColorAttachment {
				view: multisample_view.as_ref().unwrap_or(&output_view),
				resolve_target: multisample_view.as_ref().map(|_| &output_view),
				ops: wgpu::Operations {
					load: wgpu::LoadOp::Clear(self.clear_color),
					store: true,
				},
			})],
			depth_stencil_attachment: None,
		});

		self.viewport_buffer.activate(&mut render_pass, 0);
		for render_command in render_commands {
			match render_command {
				RenderCommand::Trimesh(index_range) => self.trigon_renderer.render(&mut render_pass, index_range),
				RenderCommand::Card(instance_range) => self.card_renderer.render(&mut render_pass, instance_range),
				RenderCommand::ColorRing(instance_range) => self.color_ring_renderer.render(&mut render_pass, instance_range),
				RenderCommand::ColorTrigon(instance_range) => self.color_trigon_renderer.render(&mut render_pass, instance_range),
			}
		}

		drop(render_pass);

		// Submit our commands and schedule the resultant texture for presentation.
		self.queue.submit(std::iter::once(encoder.finish()));
		output.present();

		// Return successfully.
		Ok(())
	}

	pub fn clear(&self) -> Result<SurfaceTexture, wgpu::SurfaceError> {
		// Set up the surface texture we will later render to.
		let output = self.surface.get_current_texture()?;
		let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

		// Set up the command buffer we will later send to the GPU.
		let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

		// Add a render pass to the command buffer.
		// Here, we clear the color.
		let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
			label: Some("render_pass"),
			color_attachments: &[Some(wgpu::RenderPassColorAttachment {
				view: &view,
				resolve_target: None,
				ops: wgpu::Operations {
					load: wgpu::LoadOp::Clear(self.clear_color),
					store: true,
				},
			})],
			depth_stencil_attachment: None,
		});

		drop(render_pass);

		// Submit our commands and schedule the resultant texture for presentation.
		self.queue.submit(std::iter::once(encoder.finish()));

		// Return successfully.
		Ok(output)
	}
}
