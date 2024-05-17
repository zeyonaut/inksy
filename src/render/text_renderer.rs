// Copyright (C) 2024 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::Arc;

use crate::utility::{Px, Vex};

pub struct TextRenderer {
	swash_cache: glyphon::SwashCache,
	text_renderer: glyphon::TextRenderer,
	font_system: glyphon::FontSystem,
	text_atlas: glyphon::TextAtlas,
}

pub struct TextInstance {
	buffer: glyphon::Buffer,
	default_color: glyphon::Color,
	pub position: Vex<2, Px>,
	pub anchors: [f32; 2],
}

#[derive(Clone, Copy)]
pub enum Align {
	Left,
	Center,
	Right,
}

impl TextRenderer {
	pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat, sample_count: u32) -> Self {
		let mut font_system = glyphon::FontSystem::new_with_fonts([glyphon::fontdb::Source::Binary(Arc::new(include_bytes!("../../ext/dejavu-sans-2.37/DejaVuSans.ttf").as_slice()))]);
		font_system.db_mut().set_sans_serif_family("DejaVu Sans");
		let swash_cache = glyphon::SwashCache::new();
		let mut text_atlas = glyphon::TextAtlas::new(device, queue, surface_format);
		let text_renderer = glyphon::TextRenderer::new(
			&mut text_atlas,
			device,
			wgpu::MultisampleState {
				count: sample_count,
				mask: !0,
				alpha_to_coverage_enabled: false,
			},
			None,
		);

		Self {
			swash_cache,
			text_renderer,
			font_system,
			text_atlas,
		}
	}

	pub fn prepare<'i>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: impl IntoIterator<Item = &'i TextInstance>, width: u32, height: u32, scale_factor: f32) {
		let instances = instances.into_iter();
		let mut text_areas = Vec::with_capacity(instances.size_hint().0);
		for instance in instances {
			let buffer_size = instance.buffer.size();
			let buffer_size = [buffer_size.0, buffer_size.1].map(|x| x * scale_factor);

			let [left, top] = [0, 1].map(|i| instance.position[i].0 - buffer_size[i] * instance.anchors[i]);
			text_areas.push(glyphon::TextArea {
				buffer: &instance.buffer,
				left,
				top,
				scale: scale_factor,
				bounds: glyphon::TextBounds {
					left: left.trunc() as i32,
					top: top.trunc() as i32,
					right: (left + buffer_size[0]).ceil() as i32,
					bottom: (top + buffer_size[1]).ceil() as i32,
				},
				default_color: instance.default_color,
			})
		}

		self.text_renderer
			.prepare(device, queue, &mut self.font_system, &mut self.text_atlas, glyphon::Resolution { width, height }, text_areas, &mut self.swash_cache)
			.unwrap();
	}

	pub fn render<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
		self.text_renderer.render(&self.text_atlas, render_pass).unwrap();
	}
}

impl TextInstance {
	pub fn new(renderer: &mut TextRenderer, text: &str, font_size: f32, line_height_factor: f32, align: Option<Align>, position: Vex<2, Px>, anchors: [f32; 2]) -> Self {
		let line_height = line_height_factor * font_size;
		let mut buffer = glyphon::Buffer::new(&mut renderer.font_system, glyphon::Metrics::new(font_size, line_height));
		// Set the text, and resize the buffer to fit it perfectly.
		buffer.set_text(&mut renderer.font_system, text, glyphon::Attrs::new().stretch(glyphon::Stretch::Condensed), glyphon::Shaping::Basic);
		for line in &mut buffer.lines {
			line.set_align(align.map(|align| match align {
				Align::Left => glyphon::cosmic_text::Align::Left,
				Align::Center => glyphon::cosmic_text::Align::Center,
				Align::Right => glyphon::cosmic_text::Align::Right,
			}));
		}
		buffer.set_wrap(&mut renderer.font_system, glyphon::Wrap::None);
		{
			let number_of_lines = buffer.lines.len();
			let w = (0..number_of_lines).fold(f32::MIN, |a, i| a.max(buffer.line_layout(&mut renderer.font_system, i).unwrap().iter().fold(0., |a, x| a + x.w)));
			let h = line_height * number_of_lines as f32;
			buffer.set_size(&mut renderer.font_system, w, h);
		}
		buffer.shape_until_scroll(&mut renderer.font_system, true);

		Self {
			buffer,
			default_color: glyphon::Color::rgb(0xff, 0xff, 0xff),
			position,
			anchors,
		}
	}
}
