struct ViewportUniform {
	position: vec2<f32>,
	size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;

struct VertexInput {
	@location(0) position: vec3<f32>,
	@location(1) color: vec4<f32>,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	@location(0) color: vec4<f32>
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
	var out: VertexOutput;
	out.position = vec4<f32>((model.position.xy - viewport.position) / viewport.size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0) , model.position.z, 1.0);
	out.color = model.color;
	return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	return in.color;
}