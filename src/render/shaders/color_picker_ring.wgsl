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

struct Instance {
	@location(0) position: vec2f,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) saturation_value: vec2f,
}

struct ClipVertex {
	@builtin(position) position: vec4f,
	@location(0) center: vec2f,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) saturation_value: vec2f,
}

var<private> vertices: array<vec2f, 4> = array<vec2f, 4>(
	vec2f(0., 0.),
	vec2f(2., 0.),
	vec2f(2., 2.),
	vec2f(0., 2.),
);

@vertex
fn vs_main(instance: Instance, @builtin(vertex_index) index: u32) -> ClipVertex {
	var out: ClipVertex;
	let position = instance.position;
	out.position = vec4((vertices[index] * (instance.radius_major + 4.) - 2. + position) / viewport.size * vec2(2., -2.) + vec2(-1., 1.), 0., 1.);
	out.center = position + instance.radius_major;
	out.radius_major = instance.radius_major;
	out.radius_minor = instance.radius_minor;
	out.saturation_value = instance.saturation_value;
	return out;
}

fn hue(h: f32) -> vec3f {
	return saturate(vec3(abs(h * 6. - 3.) - 1., 2. - abs(h * 6. - 2.), 2. - abs(h * 6. - 4.)));
}

fn hsv_to_srgb(color: vec3f) -> vec3f {
	return ((hue(color.x) - 1.) * color.y + 1.) * color.z;
}

// IEC 61966-2-1
fn srgb_to_linear(color: vec3f) -> vec3f {
  return mix(pow((color + 0.055) * (1. / 1.055), vec3(2.4)), color * (1. / 12.92), step(color, vec3(0.04045)));
}

const PI: f32 = 3.141592653589793238462643383279;

fn blurred_step(edge: f32, value: f32) -> f32 {
	let radius = 1./sqrt(2.) * length(vec2(dpdx(value), dpdy(value)));
	return smoothstep(edge - radius, edge + radius, value);
}

@fragment
fn fs_main(in: ClipVertex) -> @location(0) vec4f {
	let vector = in.position.xy - in.center;
	let distance_from_center = length(vector);
	let color_hsv = vec3(atan2(vector.y, vector.x) / (2. * PI) + 0.5, in.saturation_value);
	let color = srgb_to_linear(hsv_to_srgb(color_hsv));
	return vec4(color, blurred_step(in.radius_minor, distance_from_center) * (1. - blurred_step(in.radius_major, distance_from_center)));
}
