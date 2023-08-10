// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

struct ViewportUniform {
	position: vec2<f32>,
	size: vec2<f32>,
	scale: f32,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;

struct VertexInput {
	@location(0) position: vec2<f32>,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) depth: f32,
	@location(4) saturation_value: vec2<f32>,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	@location(0) center: vec2<f32>,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) saturation_value: vec2<f32>,
}

var<private> vertices: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
	vec2<f32>(0., 0.),
	vec2<f32>(2., 0.),
	vec2<f32>(2., 2.),
	vec2<f32>(0., 2.),
);

@vertex
fn vs_main(shape: VertexInput, @builtin(vertex_index) index: u32) -> VertexOutput {
	var out: VertexOutput;
	let position = shape.position;
	out.position = vec4((vertices[index] * (shape.radius_major + 4.) - 2. + position) / viewport.size * vec2(2., -2.) + vec2(-1., 1.), shape.depth, 1.);
	out.center = position + shape.radius_major;
	out.radius_major = shape.radius_major;
	out.radius_minor = shape.radius_minor;
	out.saturation_value = shape.saturation_value;
	return out;
}

fn hue(h: f32) -> vec3<f32> {
	return saturate(vec3(abs(h * 6. - 3.) - 1., 2. - abs(h * 6. - 2.), 2. - abs(h * 6. - 4.)));
}

fn hsv_to_srgb(color: vec3<f32>) -> vec3<f32> {
	return ((hue(color.x) - 1.) * color.y + 1.) * color.z;
}

// IEC 61966-2-1
fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
  return mix(pow((color + 0.055) * (1. / 1.055), vec3(2.4)), color * (1. / 12.92), step(color, vec3(0.04045)));
}

const PI: f32 = 3.141592653589793238462643383279;

fn blurred_step(edge: f32, value: f32) -> f32 {
	let radius = 1./sqrt(2.) * length(vec2(dpdx(value), dpdy(value)));
	return smoothstep(edge - radius, edge + radius, value);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let vector = in.position.xy - in.center;
	let distance_from_center = length(vector);
	let color_hsv = vec3(atan2(vector.y, vector.x) / (2. * PI) + 0.5, in.saturation_value);
	let color = srgb_to_linear(hsv_to_srgb(color_hsv));
	return vec4(color, blurred_step(in.radius_minor, distance_from_center) * (1. - blurred_step(in.radius_major, distance_from_center)));
}