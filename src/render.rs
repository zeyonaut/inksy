use fast_srgb8::srgb8_to_f32;
use pollster::FutureExt;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use wgpu::{util::DeviceExt, VertexBufferLayout};
use wgpu_glyph::{ab_glyph, GlyphBrushBuilder, Section, Text};

enum DrawCommand {
	Card { position: [u32; 2], dimensions: [u32; 2], color: [u8; 4], radius: u32 },
	Triangle { vertices: [u32; 3] },
	Text { string: String },
}

// This struct stores the data of each vertex to be rendered.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
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

// TODO: this is temporary, remove later.
const VERTICES: &[Vertex] = &[
	Vertex {
		position: [(-0.0868241 + 1.) * 400., (0.49240386 + 1.) * 300., 0.0],
		color: [0.0, 0.0, 0.0, 0.5],
	},
	Vertex {
		position: [(-0.49513406 + 1.) * 400., (0.06958647 + 1.) * 300., 0.0],
		color: [0.0, 1.0, 0.0, 0.5],
	},
	Vertex {
		position: [(-0.21918549 + 1.) * 400., (-0.44939706 + 1.) * 300., 0.0],
		color: [0.0, 1.0, 1.0, 0.5],
	},
	Vertex {
		position: [(0.35966998 + 1.) * 400., (-0.3473291 + 1.) * 300., 0.0],
		color: [1.0, 0.0, 1.0, 1.0],
	},
	Vertex {
		position: [(0.44147372 + 1.) * 400., (0.2347359 + 1.) * 300., 0.0],
		color: [1.0, 0.0, 0.0, 1.0],
	},
];

const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4];

const RECT_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
	pub size: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CardInstance {
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
	staging_belt: wgpu::util::StagingBelt,

	pub width: u32,
	pub height: u32,
	pub clear_color: wgpu::Color,
	render_pipeline: wgpu::RenderPipeline,
	viewport_buffer: wgpu::Buffer,
	viewport_bind_group: wgpu::BindGroup,
	rect_render_pipeline: wgpu::RenderPipeline,
	dejavu_sans_glyph_brush: wgpu_glyph::GlyphBrush<()>,
	/*
	vertex_buffer: wgpu::Buffer,
	index_buffer: wgpu::Buffer,
	index_range: Range<u32>,
	*/
}

impl Renderer {
	// Create an instance of the renderer.
	pub fn new<W>(window: &W, width: u32, height: u32) -> Self
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

		let staging_belt = wgpu::util::StagingBelt::new(1024);

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
			contents: bytemuck::cast_slice(&[ViewportUniform { size: [width as f32, height as f32] }]),
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

		let dejavu_sans = ab_glyph::FontArc::try_from_slice(include_bytes!("dejavu-sans-2.37/DejaVuSans.ttf")).unwrap();

		let dejavu_sans_glyph_brush = GlyphBrushBuilder::using_font(dejavu_sans).build(&device, texture_format);

		// We return a new instance of our renderer state.
		Self {
			surface,
			device,
			queue,
			config,
			staging_belt,
			width,
			height,
			clear_color: wgpu::Color { r: 0., g: 0., b: 0., a: 1.0 },
			render_pipeline,
			viewport_buffer,
			viewport_bind_group,
			rect_render_pipeline,
			dejavu_sans_glyph_brush,
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

			// We write the new size to the viewport buffer.
			self.queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[ViewportUniform { size: [width as f32, height as f32] }]))
		}
	}

	pub fn update(&mut self) {}

	pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
		// Create a vertex buffer and index buffer.
		let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("vertex_buffer"),
			contents: bytemuck::cast_slice(VERTICES),
			usage: wgpu::BufferUsages::VERTEX,
		});
		let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("index_buffer"),
			contents: bytemuck::cast_slice(INDICES),
			usage: wgpu::BufferUsages::INDEX,
		});
		let index_range = 0..INDICES.len() as u32;

		let card_instances = vec![
			CardInstance {
				position: [10.0, 10.0],
				dimensions: [202.0, 102.0],
				color: [srgb8_to_f32(0x2a), srgb8_to_f32(0xda), srgb8_to_f32(0xfa), 1.0],
				depth: 0.0,
				radius: 5.5,
			},
			CardInstance {
				position: [11.0, 11.0],
				dimensions: [200.0, 100.0],
				color: [srgb8_to_f32(0x1e), srgb8_to_f32(0x1e), srgb8_to_f32(0x1e), 1.0],
				depth: 0.0,
				radius: 4.5,
			},
		];

		let card_instance_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("Instance Buffer"),
			contents: bytemuck::cast_slice(card_instances.as_slice()),
			usage: wgpu::BufferUsages::VERTEX,
		});

		let rect_index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
			label: Some("rect_index_buffer"),
			contents: bytemuck::cast_slice(RECT_INDICES),
			usage: wgpu::BufferUsages::INDEX,
		});
		let rect_index_range = 0..RECT_INDICES.len() as u32;

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

		// We activate our pipeline and supply a single vertex buffer, as promised.
		render_pass.set_pipeline(&self.render_pipeline);
		render_pass.set_bind_group(0, &self.viewport_bind_group, &[]);
		render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
		render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(index_range.clone(), 0, 0..1);

		render_pass.set_pipeline(&self.rect_render_pipeline);
		render_pass.set_vertex_buffer(0, card_instance_buffer.slice(..));
		render_pass.set_index_buffer(rect_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
		render_pass.draw_indexed(rect_index_range.clone(), 0, 0..card_instances.len() as u32);

		drop(render_pass);

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

		// Submit our commands and schedule the resultant texture for presentation.
		self.queue.submit(std::iter::once(encoder.finish()));
		output.present();

		// Return successfully.
		Ok(())
	}
}
