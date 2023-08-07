struct ViewportUniform {
	position: vec2<f32>,
	size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: ViewportUniform;

struct VertexInput {
	@location(0) position: vec2<f32>,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) depth: f32,
}

struct VertexOutput {
	@builtin(position) position: vec4<f32>,
	@location(0) center: vec2<f32>,
	@location(1) radius_major: f32,
	@location(2) radius_minor: f32,
	@location(3) color: vec4<f32>,
	@location(4) instance_index: u32,
}

var<private> vertices: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
	vec2<f32>(0.0, 0.0),
	vec2<f32>(2.0, 0.0),
	vec2<f32>(2.0, 2.0),
	vec2<f32>(0.0, 2.0),
);

@vertex
fn vs_main(shape: VertexInput, @builtin(vertex_index) index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
	var out: VertexOutput;
	let position = shape.position;
	out.position = vec4<f32>((vertices[index] * shape.radius_major + position) / viewport.size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), shape.depth, 1.0);
	out.center = position + vec2<f32>(shape.radius_major, shape.radius_major);
	out.radius_major = shape.radius_major;
	out.radius_minor = shape.radius_minor;
	out.instance_index = instance_index;
	return out;
}

fn hue(h: f32) -> vec3<f32> {
	return saturate(vec3(abs(h * 6.0 - 3.0) - 1.0, 2.0 - abs(h * 6.0 - 2.0), 2.0 - abs(h * 6.0 - 4.0)));
}

fn hsv_to_srgb(color: vec3<f32>) -> vec3<f32> {
	return ((hue(color.x) - 1.0) * color.y + 1.0) * color.z;
}

// IEC 61966-2-1
fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
  return mix(pow((color + 0.055) * (1.0 / 1.055), vec3(2.4)), color * (1.0 / 12.92), step(color, vec3(0.04045)));
}

fn cross2(u: vec2<f32>, v: vec2<f32>) -> f32 {
	return u.x * v.y - u.y * v.x;
}

const PI: f32 = 3.141592653589793238462643383279;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let vector = in.position.xy - in.center;
	let distance_from_center = length(vector);
	let color_hsv = vec3(atan2(vector.y, vector.x) / (2.0 * PI) + 0.5, 1.0, 1.0);
	let color = srgb_to_linear(hsv_to_srgb(color_hsv));
	return vec4(color, smoothstep(in.radius_minor - 1.0/sqrt(2.0), in.radius_minor, distance_from_center) * (1.0 - smoothstep(in.radius_major, in.radius_major + 1.0/sqrt(2.0), distance_from_center))) ;
}