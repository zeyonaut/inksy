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

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(1) @binding(0) var<uniform> selection_transformation: SelectionTransformation;
@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

struct Instance {
	@location(0) position: vec2f,
	@location(1) orientation: f32,
	@location(2) dilation: f32,
	@location(3) dimensions: vec2f,
	@location(4) sprite_position: vec2f,
	@location(5) sprite_dimensions: vec2f,
	@location(6) is_selected: f32,
}

struct ClipVertex {
	@builtin(position) position: vec4f,
	@location(0) texture_coordinates: vec2f,
	@location(1) is_selected: f32,
}

var<private> vertices: array<vec2f, 4> = array<vec2f, 4>(
	vec2f(0., 0.),
	vec2f(1., 0.),
	vec2f(1., 1.),
	vec2f(0., 1.),
);

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
fn vs_main(instance: Instance, @builtin(vertex_index) index: u32) -> ClipVertex {
	var out: ClipVertex;
	let vertex = vertices[index] - vec2(0.5);
	let transformed_position = instance.position + rotate((vertex * instance.dimensions * instance.dilation), instance.orientation);
	let selection_transformed_position = selection_transformation.translation + conform_about(transformed_position, selection_transformation.center_of_transformation, selection_transformation.rotation, selection_transformation.dilation);

	let position = (1. - instance.is_selected) * transformed_position + instance.is_selected * selection_transformed_position;

	out.position = vec4(rotate((position - viewport.position) * viewport.scale, -viewport.tilt) / viewport.size * vec2(2., -2.), 0., 1.);
	out.texture_coordinates = (instance.sprite_position + vertices[index] * instance.sprite_dimensions) / vec2f(textureDimensions(atlas_texture));
	out.is_selected = instance.is_selected;
	
	return out;
}

@fragment
fn fs_main(in: ClipVertex) -> @location(0) vec4f {
	let texture_color = textureSample(atlas_texture, atlas_sampler, in.texture_coordinates);

	return (1. - in.is_selected) * texture_color + in.is_selected * vec4f(0.5 * texture_color.rgb + 0.5 * srgb_to_linear(vec3f(0x28./0xff., 0xc2./0xff., 0xff./0xff.)), texture_color.a);
}
