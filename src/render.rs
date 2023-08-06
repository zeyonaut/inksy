use std::ops::Range;

use fast_srgb8::srgb8_to_f32;
use pollster::FutureExt;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use wgpu::{util::DeviceExt, SurfaceTexture, VertexBufferLayout};
use wgpu_glyph::{ab_glyph, GlyphBrushBuilder, Section, Text};

use crate::stroke::Stroke;

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
	// Returns the layout of buffers composed of instances of Vertex.
	pub fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Vertex,
			attributes: &[
				wgpu::VertexAttribute {
					offset: 0,
					shader_location: 0,
					format: wgpu::VertexFormat::Float32x3,
				},
				wgpu::VertexAttribute {
					offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
					shader_location: 1,
					format: wgpu::VertexFormat::Float32x4,
				},
			],
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
	// Returns the layout of buffers composed of instances of Vertex.
	pub fn buffer_layout<'a>() -> VertexBufferLayout<'a> {
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode: wgpu::VertexStepMode::Instance,
			attributes: &[
				wgpu::VertexAttribute {
					offset: 0,
					shader_location: 0,
					format: wgpu::VertexFormat::Float32x2,
				},
				wgpu::VertexAttribute {
					offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
					shader_location: 1,
					format: wgpu::VertexFormat::Float32x2,
				},
				wgpu::VertexAttribute {
					offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
					shader_location: 2,
					format: wgpu::VertexFormat::Float32x4,
				},
				wgpu::VertexAttribute {
					offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
					shader_location: 3,
					format: wgpu::VertexFormat::Float32,
				},
				wgpu::VertexAttribute {
					offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
					shader_location: 4,
					format: wgpu::VertexFormat::Float32,
				},
			],
		}
	}
}

// This struct stores the current state of the WGPU renderer.
pub struct Renderer {
	surface: wgpu::Surface,
	device: wgpu::Device,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	//staging_belt: wgpu::util::StagingBelt,
	pub width: u32,
	pub height: u32,
	pub position: [f32; 2],
	pub is_pending_resize: bool,
	pub clear_color: wgpu::Color,
	render_pipeline: wgpu::RenderPipeline,
	viewport_buffer: wgpu::Buffer,
	viewport_bind_group: wgpu::BindGroup,
	rect_render_pipeline: wgpu::RenderPipeline,
	//dejavu_sans_glyph_brush: wgpu_glyph::GlyphBrush<()>,
	strokes_vertex_buffer_size: u64,
	strokes_vertex_buffer: wgpu::Buffer,
	strokes_index_buffer_size: u64,
	strokes_index_buffer: wgpu::Buffer,
	selections_vertex_buffer_size: u64,
	selections_vertex_buffer: wgpu::Buffer,
	rect_index_buffer: wgpu::Buffer,
	rect_index_range: Range<u32>,
	/*
	vertex_buffer: wgpu::Buffer,
	index_buffer: wgpu::Buffer,
	index_range: Range<u32>,
	*/
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

		//let staging_belt = wgpu::util::StagingBelt::new(1024);

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

		let strokes_vertex_buffer_size = (std::mem::size_of::<Vertex>() as u64 * (1 << 16)).next_power_of_two();

		let strokes_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("strokes_vertex_buffer"),
			size: strokes_vertex_buffer_size,
			usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let strokes_index_buffer_size = (std::mem::size_of::<u16>() as u64 * (1 << 16)).next_power_of_two();

		let strokes_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("strokes_index_buffer"),
			size: strokes_index_buffer_size,
			usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		// FIXME: Probably too large.
		let selections_vertex_buffer_size = (std::mem::size_of::<CardInstance>() as u64 * (1 << 8)).next_power_of_two();

		let selections_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("selections_vertex_buffer"),
			size: selections_vertex_buffer_size,
			usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		// This index buffer will be used for any roundrect draw calls.
		const RECT_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

		let rect_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("rect_index_buffer"),
			contents: bytemuck::cast_slice(RECT_INDICES),
			usage: wgpu::BufferUsages::INDEX,
		});
		let rect_index_range = 0..RECT_INDICES.len() as u32;

		/*
		let dejavu_sans = ab_glyph::FontArc::try_from_slice(include_bytes!("dejavu-sans-2.37/DejaVuSans.ttf")).unwrap();

		let dejavu_sans_glyph_brush = GlyphBrushBuilder::using_font(dejavu_sans).build(&device, texture_format);

		*/
		// We return a new instance of our renderer state.
		Self {
			surface,
			device,
			queue,
			config,
			//staging_belt,
			width,
			height,
			position,
			is_pending_resize: false,
			clear_color: wgpu::Color::BLACK,
			render_pipeline,
			viewport_buffer,
			viewport_bind_group,
			rect_render_pipeline,
			strokes_vertex_buffer_size,
			strokes_vertex_buffer,
			strokes_index_buffer_size,
			strokes_index_buffer,
			selections_vertex_buffer,
			selections_vertex_buffer_size,
			rect_index_buffer,
			rect_index_range,
			//dejavu_sans_glyph_brush,
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

	pub fn render(&mut self, selection_card_instances: &[CardInstance], strokes: &[Stroke]) -> Result<(), wgpu::SurfaceError> {
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

		// Write strokes data into strokes buffers.
		let mut strokes_vertices = vec![];
		let mut strokes_indices = vec![];

		for stroke in strokes {
			let (stroke_vertices, stroke_indices) = stroke.build();
			let current_index = u16::try_from(strokes_vertices.len()).unwrap();
			strokes_vertices.extend(stroke_vertices.into_iter());
			strokes_indices.extend(stroke_indices.into_iter().map(|n| n + current_index));
		}

		let strokes_index_range = 0..strokes_indices.len() as u32;

		let mut new_strokes_vertices_len = strokes_vertices.len();
		while (std::mem::size_of::<Vertex>() * new_strokes_vertices_len & wgpu::COPY_BUFFER_ALIGNMENT as usize - 1) != 0 {
			new_strokes_vertices_len = (new_strokes_vertices_len + 1).next_power_of_two();
		}

		strokes_vertices.resize(new_strokes_vertices_len, Vertex { position: [0., 0., 0.], color: [0., 0., 0., 0.] });

		let mut new_strokes_indices_len = strokes_indices.len();
		while (std::mem::size_of::<u16>() * new_strokes_indices_len & wgpu::COPY_BUFFER_ALIGNMENT as usize - 1) != 0 {
			new_strokes_indices_len = (new_strokes_indices_len + 1).next_power_of_two();
		}

		strokes_indices.resize(new_strokes_indices_len, 0);

		if self.strokes_vertex_buffer_size < (std::mem::size_of::<Vertex>() * strokes_vertices.len()) as u64 {
			self.strokes_vertex_buffer_size = (std::mem::size_of::<Vertex>() * strokes_vertices.len()).next_power_of_two() as u64;
			self.strokes_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
				label: Some("strokes_vertex_buffer"),
				size: self.strokes_vertex_buffer_size,
				usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
				mapped_at_creation: false,
			});
		}

		if self.strokes_index_buffer_size < (std::mem::size_of::<u16>() * strokes_indices.len()) as u64 {
			self.strokes_index_buffer_size = (std::mem::size_of::<u16>() * strokes_indices.len()).next_power_of_two() as u64;
			self.strokes_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
				label: Some("strokes_index_buffer"),
				size: self.strokes_index_buffer_size,
				usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
				mapped_at_creation: false,
			});
		}

		self.queue.write_buffer(&self.strokes_vertex_buffer, 0, bytemuck::cast_slice(&strokes_vertices));
		self.queue.write_buffer(&self.strokes_index_buffer, 0, bytemuck::cast_slice(&strokes_indices));

		let mut selection_card_instances = selection_card_instances.to_vec();
		let mut new_selection_card_instances_len = selection_card_instances.len();
		while (std::mem::size_of::<CardInstance>() * new_selection_card_instances_len & wgpu::COPY_BUFFER_ALIGNMENT as usize - 1) != 0 {
			new_selection_card_instances_len = (new_selection_card_instances_len + 1).next_power_of_two();
		}

		selection_card_instances.resize(
			new_selection_card_instances_len,
			CardInstance {
				position: [0.; 2],
				dimensions: [0.; 2],
				color: [0.; 4],
				depth: 0.,
				radius: 0.,
			},
		);

		if self.selections_vertex_buffer_size < (std::mem::size_of::<CardInstance>() * selection_card_instances.len()) as u64 {
			self.selections_vertex_buffer_size = (std::mem::size_of::<CardInstance>() * selection_card_instances.len()).next_power_of_two() as u64;
			self.selections_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
				label: Some("selections_vertex_buffer"),
				size: self.selections_vertex_buffer_size,
				usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
				mapped_at_creation: false,
			});
		}

		self.queue.write_buffer(&self.selections_vertex_buffer, 0, bytemuck::cast_slice(&selection_card_instances));

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
		render_pass.set_vertex_buffer(0, self.strokes_vertex_buffer.slice(..));
		render_pass.set_index_buffer(self.strokes_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(strokes_index_range.clone(), 0, 0..1);

		render_pass.set_pipeline(&self.rect_render_pipeline);
		render_pass.set_vertex_buffer(0, self.selections_vertex_buffer.slice(..));
		render_pass.set_index_buffer(self.rect_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(self.rect_index_range.clone(), 0, 0..selection_card_instances.len() as u32);

		drop(render_pass);

		/*
		self.dejavu_sans_glyph_brush.queue(Section {
			screen_position: (30.0, 30.0),
			bounds: (self.width as f32, self.height as f32),
			text: vec![Text::new("wgpu_glyph text 0").with_color([0.0, 0.0, 0.0, 1.0]).with_scale(40.0)],
			..Section::default()
		});

		self.dejavu_sans_glyph_brush.queue(Section {
			screen_position: (30.0, 90.0),
			bounds: (self.width as f32, self.height as f32),
			text: vec![Text::new("wgpu_glyph text 1").with_color([1.0, 1.0, 1.0, 1.0]).with_scale(40.0)],
			..Section::default()
		});

		// Draw the text.
		self.dejavu_sans_glyph_brush.draw_queued(&self.device, &mut self.staging_belt, &mut encoder, &view, self.width, self.height).expect("Draw queued");

		// Submit the work.
		self.staging_belt.finish();
		*/

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
