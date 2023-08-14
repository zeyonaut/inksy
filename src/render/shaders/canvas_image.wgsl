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

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;
@group(1) @binding(0) var atlas_texture: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct Instance {
	@location(0) position: vec2f,
	@location(1) dimensions: vec2f,
	@location(2) sprite_position: vec2f,
	@location(3) sprite_dimensions: vec2f,
}

struct ClipVertex {
	@builtin(position) position: vec4f,
	@location(0) texture_coordinates: vec2f,
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

@vertex
fn vs_main(instance: Instance, @builtin(vertex_index) index: u32) -> ClipVertex {
	var out: ClipVertex;
	let offset = vertices[index] - vec2(0.5);
	let position = instance.position + offset * instance.dimensions;
	out.position = vec4(rotate((position - viewport.position) * viewport.scale, viewport.tilt) / viewport.size * vec2(2., -2.), 0., 1.);
	out.texture_coordinates = (instance.sprite_position + vertices[index] * instance.sprite_dimensions) / vec2f(textureDimensions(atlas_texture)) ;
	return out;
}

@fragment
fn fs_main(in: ClipVertex) -> @location(0) vec4f {
	return textureSample(atlas_texture, atlas_sampler, in.texture_coordinates);
}
