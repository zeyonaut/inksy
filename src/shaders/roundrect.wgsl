// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

struct ViewportUniform {
	position: vec2<f32>,
	size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;

struct VertexInput {
	@location(0) position: vec2<f32>,
	@location(1) dimensions: vec2<f32>,
	@location(2) color: vec4<f32>,
	@location(3) depth: f32,
	@location(4) radius: f32,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	@location(0) sposition: vec2<f32>,
	@location(1) dimensions: vec2<f32>,
	@location(2) color: vec4<f32>,
	@location(3) radius: f32,
	@location(4) instance_index: u32,
}

var<private> vertices: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
	vec2<f32>(0.0, 0.0),
	vec2<f32>(1.0, 0.0),
	vec2<f32>(1.0, 1.0),
	vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(shape: VertexInput, @builtin(vertex_index) index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
	var out: VertexOutput;
	let position = shape.position - viewport.position;
	out.position = vec4<f32>((vertices[index] * shape.dimensions + position) / viewport.size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), shape.depth, 1.0);
	out.sposition = position;
	out.dimensions = shape.dimensions;
	out.color = shape.color;
	out.radius = shape.radius;
	out.instance_index = instance_index;
	return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let rect_vertex = vec2(0.5, 0.5) * in.dimensions + vec2(-in.radius);
	let rect_center = vec2(in.radius) + in.sposition + rect_vertex;
	let frag_position = in.position.xy - rect_center;
	return vec4(in.color.rgb, in.color.a * (1.0 - smoothstep(0.0, 1. / sqrt(2.), length(max(abs(frag_position), rect_vertex) - rect_vertex) - in.radius)));
}