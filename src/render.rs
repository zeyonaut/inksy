// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::ops::Range;

use fast_srgb8::srgb8_to_f32;
use pollster::FutureExt;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use wgpu::{util::DeviceExt, SurfaceTexture, VertexBufferLayout};
use wgpu_glyph::{ab_glyph, GlyphBrushBuilder, Section, Text};

use crate::{
	buffer::DynamicBuffer,
	stroke::{Canvas, Stroke},
};

const MAX_FRAME_RATE: u16 = 60;

enum DrawCommand {
	Card { position: [u32; 2], dimensions: [u32; 2], color: [u8; 4], radius: u32 },
	Triangle { vertices: [u32; 3] },
	Text { string: String },
}

// This struct stores the data of each vertex to be rendered.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
	pub position: [f32; 3],
	pub color: [f32; 4],
}

impl Vertex {
	const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

	// Returns the layout of buffers composed of instances of Self.
	pub const fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Vertex,
			attributes: &Self::ATTRIBUTES,
		}
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
	pub position: [f32; 2],
	pub size: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardInstance {
	pub position: [f32; 2],
	pub dimensions: [f32; 2],
	pub color: [f32; 4],
	pub depth: f32,
	pub radius: f32,
}

impl CardInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32, 4 => Float32];

	// Returns the layout of buffers composed of instances of Self.
	pub const fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Instance,
			attributes: &Self::ATTRIBUTES,
		}
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorWheelInstance {
	pub position: [f32; 2],
	pub radius_major: f32,
	pub radius_minor: f32,
	pub depth: f32,
	pub saturation_value: [f32; 2],
}

impl ColorWheelInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32, 4 => Float32x2];

	// Returns the layout of buffers composed of instances of Self.
	pub const fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Instance,
			attributes: &Self::ATTRIBUTES,
		}
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SaturationValuePlotInstance {
	pub position: [f32; 2],
	pub radius: f32,
	pub hue: f32,
	pub depth: f32,
}

impl SaturationValuePlotInstance {
	const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32];

	// Returns the layout of buffers composed of instances of Self.
	pub const fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Instance,
			attributes: &Self::ATTRIBUTES,
		}
	}
}

// This struct stores the current state of the WGPU renderer.
pub struct Renderer {
	surface: wgpu::Surface,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	pub width: u32,
	pub height: u32,
	pub position: [f32; 2],
	pub is_pending_resize: bool,
	pub clear_color: wgpu::Color,
	viewport_buffer: wgpu::Buffer,
	viewport_bind_group: wgpu::BindGroup,
	render_pipeline: wgpu::RenderPipeline,
	rect_render_pipeline: wgpu::RenderPipeline,
	colorwheel_render_pipeline: wgpu::RenderPipeline,
	saturation_value_plot_render_pipeline: wgpu::RenderPipeline,
	strokes_vertex_buffer: DynamicBuffer<Vertex>,
	strokes_index_buffer: DynamicBuffer<u16>,
	selections_vertex_buffer: DynamicBuffer<CardInstance>,
	colorwheel_vertex_buffer: DynamicBuffer<ColorWheelInstance>,
	saturation_value_plot_instance_buffer: DynamicBuffer<SaturationValuePlotInstance>,
	rect_index_buffer: wgpu::Buffer,
	rect_index_range: Range<u32>,
}

impl Renderer {
	// Create an instance of the renderer.
	pub fn new<W>(window: &W, position: [f32; 2], width: u32, height: u32) -> Self
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
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
			format: texture_format,
			width,
			height,
			present_mode: *surface_capabilities.present_modes.first().unwrap(),
			alpha_mode: *surface_capabilities.alpha_modes.first().unwrap(),
			view_formats: vec![],
		};
		surface.configure(&device, &config);

		// Create a viewport uniform buffer and bind group layout.
		let viewport_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			entries: &[wgpu::BindGroupLayoutEntry {
				binding: 0,
				visibility: wgpu::ShaderStages::VERTEX,
				ty: wgpu::BindingType::Buffer {
					ty: wgpu::BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: None,
				},
				count: None,
			}],
			label: Some("viewport_bind_group_layout"),
		});

		// We create a render pipeline from the vertex and fragment shaders in src/shader.wgsl.
		let render_pipeline = {
			let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
				label: Some("shader"),
				source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
			});

			let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("render_pipeline_layout"),
				bind_group_layouts: &[&viewport_bind_group_layout],
				push_constant_ranges: &[],
			});

			// We promise to supply a single vertex buffer in each render pass.
			device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
				label: Some("render_pipeline"),
				layout: Some(&render_pipeline_layout),
				vertex: wgpu::VertexState {
					module: &shader,
					entry_point: "vs_main",
					buffers: &[Vertex::buffer_layout()],
				},
				fragment: Some(wgpu::FragmentState {
					module: &shader,
					entry_point: "fs_main",
					targets: &[Some(wgpu::ColorTargetState {
						format: config.format,
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
					count: 1,
					mask: !0,
					alpha_to_coverage_enabled: false,
				},
				multiview: None,
			})
		};

		let rect_render_pipeline = {
			let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
				label: Some("rect_shader"),
				source: wgpu::ShaderSource::Wgsl(include_str!("roundrect.wgsl").into()),
			});

			let rect_render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("rect_render_pipeline_layout"),
				bind_group_layouts: &[&viewport_bind_group_layout],
				push_constant_ranges: &[],
			});

			// We promise to supply a single vertex buffer in each render pass.
			device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
				label: Some("rect_render_pipeline"),
				layout: Some(&rect_render_pipeline_layout),
				vertex: wgpu::VertexState {
					module: &rect_shader,
					entry_point: "vs_main",
					buffers: &[CardInstance::buffer_layout()],
				},
				fragment: Some(wgpu::FragmentState {
					module: &rect_shader,
					entry_point: "fs_main",
					targets: &[Some(wgpu::ColorTargetState {
						format: config.format,
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
					count: 1,
					mask: !0,
					alpha_to_coverage_enabled: false,
				},
				multiview: None,
			})
		};

		let colorwheel_render_pipeline = {
			let colorwheel_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
				label: Some("colorwheel_shader"),
				source: wgpu::ShaderSource::Wgsl(include_str!("colorwheel.wgsl").into()),
			});

			let colorwheel_render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("colorwheel_render_pipeline_layout"),
				bind_group_layouts: &[&viewport_bind_group_layout],
				push_constant_ranges: &[],
			});

			// We promise to supply a single vertex buffer in each render pass.
			device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
				label: Some("colorwheel_render_pipeline"),
				layout: Some(&colorwheel_render_pipeline_layout),
				vertex: wgpu::VertexState {
					module: &colorwheel_shader,
					entry_point: "vs_main",
					buffers: &[ColorWheelInstance::buffer_layout()],
				},
				fragment: Some(wgpu::FragmentState {
					module: &colorwheel_shader,
					entry_point: "fs_main",
					targets: &[Some(wgpu::ColorTargetState {
						format: config.format,
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
					count: 1,
					mask: !0,
					alpha_to_coverage_enabled: false,
				},
				multiview: None,
			})
		};

		let saturation_value_plot_render_pipeline = {
			let saturation_value_plot_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
				label: Some("saturation_value_plot_shader"),
				source: wgpu::ShaderSource::Wgsl(include_str!("saturation_value_plot.wgsl").into()),
			});

			let saturation_value_plot_render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("saturation_value_plot_render_pipeline_layout"),
				bind_group_layouts: &[&viewport_bind_group_layout],
				push_constant_ranges: &[],
			});

			// We promise to supply a single vertex buffer in each render pass.
			device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
				label: Some("saturation_value_plot_render_pipeline"),
				layout: Some(&saturation_value_plot_render_pipeline_layout),
				vertex: wgpu::VertexState {
					module: &saturation_value_plot_shader,
					entry_point: "vs_main",
					buffers: &[SaturationValuePlotInstance::buffer_layout()],
				},
				fragment: Some(wgpu::FragmentState {
					module: &saturation_value_plot_shader,
					entry_point: "fs_main",
					targets: &[Some(wgpu::ColorTargetState {
						format: config.format,
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
					count: 1,
					mask: !0,
					alpha_to_coverage_enabled: false,
				},
				multiview: None,
			})
		};

		let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("viewport_buffer"),
			contents: bytemuck::cast_slice(&[ViewportUniform {
				position: [0., 0.],
				size: [width as f32, height as f32],
			}]),
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		});

		let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &viewport_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding: 0,
				resource: viewport_buffer.as_entire_binding(),
			}],
			label: Some("viewport_bind_group"),
		});

		let strokes_vertex_buffer = DynamicBuffer::<Vertex>::new(&device, wgpu::BufferUsages::VERTEX, 1 << 16);
		let strokes_index_buffer = DynamicBuffer::<u16>::new(&device, wgpu::BufferUsages::INDEX, 1 << 16);
		let selections_vertex_buffer = DynamicBuffer::<CardInstance>::new(&device, wgpu::BufferUsages::VERTEX, 1 << 0);
		let colorwheel_vertex_buffer = DynamicBuffer::<ColorWheelInstance>::new(&device, wgpu::BufferUsages::VERTEX, 1 << 0);
		let saturation_value_plot_instance_buffer = DynamicBuffer::<SaturationValuePlotInstance>::new(&device, wgpu::BufferUsages::VERTEX, 1 << 0);

		// This index buffer will be used for any roundrect and colorwheel draw calls.
		const RECT_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

		let rect_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("rect_index_buffer"),
			contents: bytemuck::cast_slice(RECT_INDICES),
			usage: wgpu::BufferUsages::INDEX,
		});
		let rect_index_range = 0..RECT_INDICES.len() as u32;

		// We return a new instance of our renderer state.
		Self {
			surface,
			device,
			queue,
			config,
			width,
			height,
			position,
			is_pending_resize: false,
			clear_color: wgpu::Color::BLACK,
			viewport_buffer,
			viewport_bind_group,
			render_pipeline,
			rect_render_pipeline,
			colorwheel_render_pipeline,
			saturation_value_plot_render_pipeline,
			strokes_vertex_buffer,
			strokes_index_buffer,
			selections_vertex_buffer,
			colorwheel_vertex_buffer,
			saturation_value_plot_instance_buffer,
			rect_index_buffer,
			rect_index_range,
		}
	}

	// Resize the renderer to a requested size.
	pub fn resize(&mut self, width: u32, height: u32) {
		// We ensure the requested size has nonzero dimensions before applying it.
		if width > 0 && height > 0 {
			self.width = width;
			self.height = height;
			self.config.width = width;
			self.config.height = height;
			self.surface.configure(&self.device, &self.config);
			self.is_pending_resize = true;
		}
	}

	pub fn reposition(&mut self, position: [f32; 2]) {
		if self.position != position {
			self.position = position;
			self.is_pending_resize = true;
		}
	}

	pub fn update(&mut self) {}

	pub fn render(
		&mut self,
		selection_card_instances: &[CardInstance],
		strokes_vertices: Vec<Vertex>,
		strokes_indices: Vec<u16>,
		colorwheel_instances: Vec<ColorWheelInstance>,
		saturation_value_plot_instances: Vec<SaturationValuePlotInstance>,
	) -> Result<(), wgpu::SurfaceError> {
		if self.is_pending_resize {
			// We write the new size to the viewport buffer.
			self.queue.write_buffer(
				&self.viewport_buffer,
				0,
				bytemuck::cast_slice(&[ViewportUniform {
					position: self.position,
					size: [self.width as f32, self.height as f32],
				}]),
			);
			self.is_pending_resize = false;
		}

		let strokes_index_range = 0..strokes_indices.len() as u32;

		self.strokes_vertex_buffer.write(&self.device, &self.queue, strokes_vertices, Vertex { position: [0.; 3], color: [0.; 4] });
		self.strokes_index_buffer.write(&self.device, &self.queue, strokes_indices, 0);
		self.selections_vertex_buffer.write(
			&self.device,
			&self.queue,
			selection_card_instances.to_vec(),
			CardInstance {
				position: [0.; 2],
				dimensions: [0.; 2],
				color: [0.; 4],
				depth: 0.,
				radius: 0.,
			},
		);
		let colorwheel_instance_range = 0..colorwheel_instances.len() as u32;
		self.colorwheel_vertex_buffer.write(
			&self.device,
			&self.queue,
			colorwheel_instances,
			ColorWheelInstance {
				position: [0.; 2],
				radius_major: 0.,
				radius_minor: 0.,
				depth: 0.,
				saturation_value: [0.; 2],
			},
		);

		let saturation_value_plot_instance_range = 0..saturation_value_plot_instances.len() as u32;
		self.saturation_value_plot_instance_buffer.write(
			&self.device,
			&self.queue,
			saturation_value_plot_instances,
			SaturationValuePlotInstance {
				position: [0.; 2],
				radius: 0.,
				hue: 0.,
				depth: 0.,
			},
		);

		// Set up the surface texture we will later render to.
		let output = self.surface.get_current_texture()?;
		let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

		// Set up the command buffer we will later send to the GPU.
		let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

		// Add a render pass to the command buffer.
		// Here, we clear the color.
		let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

		render_pass.set_bind_group(0, &self.viewport_bind_group, &[]);

		// We activate our pipeline and supply a single vertex buffer, as promised.
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_vertex_buffer(0, self.strokes_vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.strokes_index_buffer.buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(strokes_index_range.clone(), 0, 0..1);

		render_pass.set_pipeline(&self.rect_render_pipeline);
		render_pass.set_vertex_buffer(0, self.selections_vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.rect_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(self.rect_index_range.clone(), 0, 0..selection_card_instances.len() as u32);

		render_pass.set_pipeline(&self.colorwheel_render_pipeline);
		render_pass.set_vertex_buffer(0, self.colorwheel_vertex_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.rect_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(self.rect_index_range.clone(), 0, colorwheel_instance_range);

		render_pass.set_pipeline(&self.saturation_value_plot_render_pipeline);
		render_pass.set_vertex_buffer(0, self.saturation_value_plot_instance_buffer.buffer.slice(..));
		render_pass.set_index_buffer(self.rect_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(self.rect_index_range.clone(), 0, saturation_value_plot_instance_range);

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
