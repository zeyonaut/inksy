// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{borrow::Cow, num::NonZeroU32, ops::Range};

use fast_srgb8::srgb8_to_f32;
use pollster::FutureExt;

use super::{
	instance_renderer::InstanceRenderer,
	stroke_renderer::CanvasRenderer,
	text_renderer::{Align, TextInstance, TextRenderer},
	texture::Texture,
	uniform_buffer::UniformBuffer,
	vertex_attributes::VertexAttributes,
};
use crate::{
	canvas::{Canvas, IncompleteStroke},
	config::Config,
	utility::{Px, Vex, Vx},
};

const SHOULD_MULTISAMPLE: bool = false;

pub enum DrawCommand<'a> {
	Text { text: Cow<'a, str>, align: Option<Align>, position: Vex<2, Px>, anchors: [f32; 2] },
	Card { position: Vex<2, Px>, dimensions: Vex<2, Px>, color: [u8; 4], radius: Px },
	ColorSelector { position: Vex<2, Px>, hsv: [f32; 3], trigon_radius: Px, hole_radius: Px, ring_width: Px },
}

pub enum RenderCommand {
	Card(Range<u32>),
	ColorRing(Range<u32>),
	ColorTrigon(Range<u32>),
}

// This struct stores the data of each vertex to be rendered.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
	pub position: [Vx; 2],
	pub polarity: f32,
	pub color: [f32; 4],
}

impl VertexAttributes<3> for Vertex {
	const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32x4];
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
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardInstance {
	pub position: [f32; 2],
	pub dimensions: [f32; 2],
	pub color: [f32; 4],
	pub radius: f32,
}

impl VertexAttributes<4> for CardInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32];
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorRingInstance {
	pub position: [f32; 2],
	pub radius_major: f32,
	pub radius_minor: f32,
	pub saturation_value: [f32; 2],
}

impl VertexAttributes<4> for ColorRingInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x2];
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorTrigonInstance {
	pub position: [f32; 2],
	pub radius: f32,
	pub hue: f32,
}

impl VertexAttributes<3> for ColorTrigonInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32];
}

// This struct stores the current state of the WGPU renderer.
pub struct Renderer<'window> {
	// Rendering machinery.
	surface: wgpu::Surface<'window>,
	pub device: wgpu::Device,
	pub queue: wgpu::Queue,
	// Properties.
	pub config: wgpu::SurfaceConfiguration,
	surface_format: wgpu::TextureFormat,
	pub scale_factor: f32,
	pub is_pending_resize: bool,
	// Text rendering.
	pub text_renderer: TextRenderer,
	pub info_text: TextInstance,
	// Other renderers.
	pub canvas_renderer: CanvasRenderer,
	pub card_renderer: InstanceRenderer<CardInstance>,
	pub color_ring_renderer: InstanceRenderer<ColorRingInstance>,
	pub color_trigon_renderer: InstanceRenderer<ColorTrigonInstance>,
	// Other resource handles.
	pub viewport_buffer: UniformBuffer<ViewportUniform>,
	texture_bind_group_layout: wgpu::BindGroupLayout,
	multisample_texture: Option<wgpu::Texture>,
}

impl<'window> Renderer<'window> {
	// Create an instance of the renderer.
	pub fn new<W>(window: &'window W, width: u32, height: u32, scale_factor: f32) -> Self
	where
		W: wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle + Sync,
	{
		// We create a WGPU instance and a surface on our window to draw to.
		let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
			backends: wgpu::Backends::all(),
			flags: Default::default(),
			dx12_shader_compiler: Default::default(),
			gles_minor_version: Default::default(),
		});
		let surface = instance.create_surface(window).unwrap();

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
					required_features: wgpu::Features::empty(),
					required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
					label: None,
				},
				None,
			)
			.block_on()
			.unwrap();

		// We define a configuration for our surface.
		// FIXME: Ensure dimensions are nonzero.
		let surface_capabilities = surface.get_capabilities(&adapter);

		let surface_format = surface_capabilities.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(*surface_capabilities.formats.first().unwrap());

		let config = wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
			format: surface_format,
			width,
			height,
			present_mode: *surface_capabilities.present_modes.first().unwrap(),
			desired_maximum_frame_latency: 2,
			alpha_mode: *surface_capabilities.alpha_modes.first().unwrap(),
			view_formats: vec![],
		};
		surface.configure(&device, &config);

		let multisample_texture = if SHOULD_MULTISAMPLE && adapter.get_texture_format_features(surface_format).flags.sample_count_supported(4) {
			Some(device.create_texture(&wgpu::TextureDescriptor {
				label: None,
				size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
				mip_level_count: 1,
				sample_count: 4,
				dimension: wgpu::TextureDimension::D2,
				format: surface_format,
				usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
				view_formats: vec![].as_slice(),
			}))
		} else {
			None
		};
		let sample_count = multisample_texture.as_ref().map_or(1, |_| 4);

		let mut text_renderer = TextRenderer::new(&device, &queue, surface_format, sample_count);

		let info_text = TextInstance::new(
			&mut text_renderer,
			"Press Ctrl + N to open a new canvas or Ctrl + O to load an existing canvas.",
			13.,
			1.25,
			Some(Align::Center),
			Vex([width as f32 / 2., height as f32 / 2.].map(Px)),
			[0.5, 0.5],
		);

		let texture_bind_group_layout = Texture::bind_group_layout(&device);

		let viewport_buffer = UniformBuffer::new(
			&device,
			0,
			ViewportUniform {
				position: [0., 0.],
				size: [width as f32, height as f32],
				scale: scale_factor,
				tilt: 0.,
			},
		);

		let sample_count = multisample_texture.as_ref().map_or(1, |_| 4);

		let canvas_renderer = CanvasRenderer::new(&device, config.format, &viewport_buffer, sample_count);
		let card_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/round_rectangle.wgsl"), "vs_main", "fs_main", &[&viewport_buffer.bind_group_layout], sample_count);
		let color_ring_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/color_picker_ring.wgsl"), "vs_main", "fs_main", &[&viewport_buffer.bind_group_layout], sample_count);
		let color_trigon_renderer = InstanceRenderer::new(&device, config.format, include_str!("shaders/color_picker_trigon.wgsl"), "vs_main", "fs_main", &[&viewport_buffer.bind_group_layout], sample_count);

		// We return a new instance of our renderer state.
		Self {
			surface,
			device,
			queue,
			config,
			scale_factor,
			is_pending_resize: false,
			viewport_buffer,
			texture_bind_group_layout,
			text_renderer,
			info_text,
			canvas_renderer,
			card_renderer,
			color_ring_renderer,
			color_trigon_renderer,
			multisample_texture,
			surface_format,
		}
	}

	// Resize the renderer to a requested size.
	pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
		// We ensure the requested size has nonzero dimensions before applying it.
		if width > 0 && height > 0 {
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
					format: self.surface_format,
					usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
					view_formats: vec![].as_slice(),
				})
			}
			self.info_text.position = Vex([width as f32 / 2., height as f32 / 2.].map(Px));
		}
	}
}

pub struct Prerender<'a> {
	pub canvas: Option<&'a mut Canvas>,
	pub current_stroke: Option<&'a IncompleteStroke>,
	pub draw_commands: Vec<DrawCommand<'a>>,
}

impl<'a> Prerender<'a> {
	pub fn new() -> Self {
		Self {
			canvas: None,
			current_stroke: None,
			draw_commands: Vec::new(),
		}
	}
}

impl<'window> Renderer<'window> {
	pub fn render(&mut self, config: &Config, mut prerender: Prerender) -> Result<(), wgpu::SurfaceError> {
		if let Some(canvas) = prerender.canvas.as_mut() {
			if let Some(view) = canvas.view.read_if_with_is_dirty(|is_dirty| is_dirty || self.is_pending_resize) {
				// We write the new size to the viewport buffer.
				self.viewport_buffer.write(
					&self.queue,
					ViewportUniform {
						position: view.position.0.map(Into::into),
						size: [self.config.width as f32, self.config.height as f32],
						scale: view.zoom.0 * self.scale_factor,
						tilt: view.tilt,
					},
				);
				self.is_pending_resize = false;
			}

			for texture in canvas.textures.iter_mut() {
				texture.prepare(&self.queue);
			}
		}

		let canvas_render_key = prerender.canvas.as_mut().map(|canvas| self.canvas_renderer.prepare(&self.device, &self.queue, canvas, prerender.current_stroke));

		// We compute the background color of the canvas.
		let background_color = {
			let [r, g, b, a] = prerender.canvas.as_ref().map_or(config.default_canvas_color, |canvas| canvas.background_color).opaque().to_lrgba().0.map(|x| x as f64);
			wgpu::Color { r, g, b, a }
		};

		let mut card_instances: Vec<CardInstance> = vec![];
		let mut color_ring_instances: Vec<ColorRingInstance> = vec![];
		let mut color_trigon_instances: Vec<ColorTrigonInstance> = vec![];
		let mut text_instances: Vec<TextInstance> = vec![];

		let mut render_commands: Vec<RenderCommand> = vec![];

		for draw_command in prerender.draw_commands {
			match draw_command {
				DrawCommand::Text { text, align, position, anchors } => text_instances.push(TextInstance::new(&mut self.text_renderer, &text, 13., 1.25, align, position, anchors)),
				DrawCommand::Card { position, dimensions, color, radius } => {
					let instance_start = card_instances.len() as u32;
					card_instances.push(CardInstance {
						position: position.0.map(|n| n.0),
						dimensions: dimensions.0.map(|n| n.0),
						color: color.map(srgb8_to_f32),
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
						saturation_value: [hsv[1], hsv[2]],
					});
					render_commands.push(RenderCommand::ColorRing(ring_instance_start..ring_instance_start + 1));

					let trigon_instance_start = color_trigon_instances.len() as u32;
					color_trigon_instances.push(ColorTrigonInstance {
						position: position.map(|n| n + ring_width + hole_radius - trigon_radius).0.map(|n| n.0),
						radius: trigon_radius.0,
						hue: hsv[0],
					});
					render_commands.push(RenderCommand::ColorTrigon(trigon_instance_start..trigon_instance_start + 1));
				},
			}
		}

		// Prepare text.
		let should_render_info_text = prerender.canvas.is_none();
		self.text_renderer.prepare(
			&self.device,
			&self.queue,
			should_render_info_text.then_some(&self.info_text).into_iter().chain(&text_instances),
			self.config.width,
			self.config.height,
			self.scale_factor,
		);

		// Prepare shapes.
		self.card_renderer.prepare(&self.device, &self.queue, 0, &card_instances);
		self.color_ring_renderer.prepare(&self.device, &self.queue, 0, &color_ring_instances);
		self.color_trigon_renderer.prepare(&self.device, &self.queue, 0, &color_trigon_instances);

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
					load: wgpu::LoadOp::Clear(background_color),
					store: wgpu::StoreOp::Store,
				},
			})],
			depth_stencil_attachment: None,
			timestamp_writes: None,
			occlusion_query_set: None,
		});

		self.viewport_buffer.activate(&mut render_pass, 0);

		if let (Some(canvas), Some(canvas_render_key)) = (prerender.canvas, canvas_render_key) {
			self.canvas_renderer.render(&mut render_pass, &canvas.textures, canvas_render_key);
		}

		for render_command in render_commands {
			match render_command {
				RenderCommand::Card(instance_range) => self.card_renderer.render(&mut render_pass, instance_range),
				RenderCommand::ColorRing(instance_range) => self.color_ring_renderer.render(&mut render_pass, instance_range),
				RenderCommand::ColorTrigon(instance_range) => self.color_trigon_renderer.render(&mut render_pass, instance_range),
			}
		}

		self.text_renderer.render(&mut render_pass);

		drop(render_pass);

		// Submit our commands and schedule the resultant texture for presentation.
		self.queue.submit(std::iter::once(encoder.finish()));
		output.present();

		// Return successfully.
		Ok(())
	}

	pub fn clear(&self, clear_color: wgpu::Color) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
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
					load: wgpu::LoadOp::Clear(clear_color),
					store: wgpu::StoreOp::Store,
				},
			})],
			depth_stencil_attachment: None,
			timestamp_writes: None,
			occlusion_query_set: None,
		});

		drop(render_pass);

		// Submit our commands and schedule the resultant texture for presentation.
		self.queue.submit(std::iter::once(encoder.finish()));

		// Return successfully.
		Ok(output)
	}

	pub fn create_texture(&self, dimensions: [NonZeroU32; 2], image: Vec<u8>) -> Texture {
		Texture::new(&self.device, dimensions, image, &self.texture_bind_group_layout)
	}

	// Returns bytes per row.
	pub fn fetch_texture(&self, texture: &Texture) -> Option<(wgpu::Buffer, usize)> {
		let source_bytes_per_row = texture.extent.width as usize * 4;
		let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
		let row_padding = (alignment - source_bytes_per_row % alignment) % alignment;
		let bytes_per_row = (source_bytes_per_row + row_padding) as u32;
		let rows_per_image = texture.extent.height;

		let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
			size: bytes_per_row as u64 * rows_per_image as u64,
			usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
			label: None,
			mapped_at_creation: false,
		});

		let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
		encoder.copy_texture_to_buffer(
			wgpu::ImageCopyTexture {
				aspect: wgpu::TextureAspect::All,
				texture: &texture.texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
			},
			wgpu::ImageCopyBuffer {
				buffer: &output_buffer,
				layout: wgpu::ImageDataLayout {
					offset: 0,
					bytes_per_row: Some(bytes_per_row),
					rows_per_image: Some(rows_per_image),
				},
			},
			texture.extent,
		);

		self.queue.submit(Some(encoder.finish()));

		Some((output_buffer, bytes_per_row as usize))
	}
}
