// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

struct ViewportUniform {
	position: vec2f,
	size: vec2f,
	scale: f32,
	tilt: f32,
}

struct SelectionTransformation {
	translation: vec2f,
	center_of_transformation: vec2f,
	rotation: f32,
	dilation: f32,
}

struct Extension {
	translation: vec2f,
	rotation: f32,
	dilation: f32,
	color: vec3f,
	is_selected: f32,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(1) @binding(0) var<uniform> selection_transformation: SelectionTransformation;
@group(2) @binding(0) var<storage> extensions: array<Extension>;


struct Vertex {
	@location(0) position: vec2f,
	@location(1) polarity: f32,
	@location(2) extension_index: u32,
}

struct ClipVertex {
	@builtin(position) position: vec4f,
	@location(0) color: vec3f,
	@location(1) polarity: f32,
}

fn rotate(v: vec2f, angle: f32) -> vec2f {
	return vec2(cos(angle) * v.x - sin(angle) * v.y, sin(angle) * v.x + cos(angle) * v.y);
}

fn conform_about(v: vec2f, center: vec2f, rotation: f32, dilation: f32) -> vec2f {
	return center + rotate(v - center, rotation) * dilation;
}

// IEC 61966-2-1
fn srgb_to_linear(color: vec3f) -> vec3f {
  return mix(pow((color + 0.055) * (1. / 1.055), vec3(2.4)), color * (1. / 12.92), step(color, vec3(0.04045)));
}

@vertex
fn vs_main(vertex: Vertex) -> ClipVertex {
	var out: ClipVertex;
	let extension = extensions[vertex.extension_index];
	let transformed_position = extension.translation + rotate(vertex.position, extension.rotation) * extension.dilation;
	let selection_transformed_position = selection_transformation.translation + conform_about(transformed_position, selection_transformation.center_of_transformation, selection_transformation.rotation, selection_transformation.dilation);
	let position = (1. - extension.is_selected) * transformed_position + extension.is_selected * selection_transformed_position;
	out.position = vec4f(rotate((position - viewport.position) * viewport.scale, -viewport.tilt) / viewport.size * vec2f(2., -2.), 0., 1.);
	out.color = (1. - extension.is_selected) * extension.color + extension.is_selected * (0.25 * extension.color + 0.75 * srgb_to_linear(vec3f(0x28./0xff., 0xc2./0xff., 0xff./0xff.)));
	out.polarity = vertex.polarity;
	return out;
}

fn blurred_step_negative(value: f32) -> f32 {
	let radius = sqrt(2.) * length(vec2(dpdx(value), dpdy(value)));
	return smoothstep(-1., -1. + radius, value);
}

fn blurred_step_positive(value: f32) -> f32 {
	let radius = sqrt(2.) * length(vec2(dpdx(value), dpdy(value)));
	return smoothstep(1. - radius, 1., value);
}

@fragment
fn fs_main(in: ClipVertex) -> @location(0) vec4f {
	return vec4f(in.color, blurred_step_negative(in.polarity) * (1. - blurred_step_positive(in.polarity)));
}
