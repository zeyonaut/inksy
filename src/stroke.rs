use fast_srgb8::srgb8_to_f32;
use vek::Vec2;

use crate::render::Vertex;

pub struct Point {
	position: Vec2<f32>,
	pressure: f32,
}

pub struct Stroke {
	pub points: Vec<Point>,
}

const STROKE_RADIUS: f32 = 8.;

impl Stroke {
	pub fn new() -> Self {
		Self { points: Vec::new() }
	}

	pub fn add_point(&mut self, x: f32, y: f32, pressure: f32) {
		if self.points.last().map_or(true, |point| (Vec2::new(x, y) - point.position).magnitude() > 2.) {
			self.points.push(Point { position: Vec2::new(x, y), pressure });
		}
	}

	pub fn build(&self) -> (Vec<Vertex>, Vec<u16>) {
		let mut positions = vec![];
		let mut indices = vec![];
		let perpendiculars = self
			.points
			.array_windows::<2>()
			.map(|[a, b]| {
				let forward = b.position - a.position;
				Vec2::new(forward.y, -forward.x).normalized() * STROKE_RADIUS
			})
			.collect::<Vec<_>>();

		for ([a, b], p) in self.points.array_windows::<2>().zip(&perpendiculars) {
			let current_index = u16::try_from(positions.len()).unwrap();
			positions.extend([a.position + p * a.pressure, a.position - p * a.pressure, b.position + p * b.pressure, b.position - p * b.pressure]);
			indices.extend([0, 2, 3, 0, 3, 1].map(|n| current_index + n));
		}

		for (i, ([_, b, _], [p, q])) in self.points.array_windows::<3>().zip(perpendiculars.array_windows::<2>()).enumerate() {
			let i = u16::try_from(i).unwrap();
			let cross_product = p.x * q.y - p.y * q.x;

			if cross_product > 0. {
				/* Clockwise */
				indices.extend([2, 4 + 0, 4 + 1].map(|n| n + i * 4));
			} else if cross_product < 0. {
				/* Counterclockwise */
				indices.extend([3, 4 + 1, 4 + 0].map(|n| n + i * 4));
			}
		}

		let vertices = positions
			.into_iter()
			.map(|position| Vertex {
				position: [position.x, position.y, 0.],
				color: [0xfb, 0xfb, 0xff, 0xff].map(srgb8_to_f32),
			})
			.collect();
		(vertices, indices)
	}
}
