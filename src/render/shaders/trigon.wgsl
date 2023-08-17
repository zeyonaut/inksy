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

struct Vertex {
	@location(0) position: vec2f,
	@location(1) color: vec4f,
}

struct ClipVertex {
	@builtin(position) position: vec4f,
	@location(0) color: vec4f
}

fn rotate(v: vec2f, angle: f32) -> vec2f {
	return vec2(cos(angle) * v.x - sin(angle) * v.y, sin(angle) * v.x + cos(angle) * v.y);
}

@vertex
fn vs_main(vertex: Vertex) -> ClipVertex {
	var out: ClipVertex;
	out.position = vec4f(rotate((vertex.position - viewport.position) * viewport.scale, viewport.tilt) / viewport.size * vec2f(2., -2.), 0., 1.);
	out.color = vertex.color;
	return out;
}

@fragment
fn fs_main(in: ClipVertex) -> @location(0) vec4f {
	return in.color;
}
