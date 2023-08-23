// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// NOTE: Ideally, the N should be an associated constant and not a parameter, but that isn't possible right now.
pub trait VertexAttributes<const N: usize> {
	const ATTRIBUTES: [wgpu::VertexAttribute; N];

	fn buffer_layout<'a>(step_mode: wgpu::VertexStepMode) -> wgpu::VertexBufferLayout<'a>
	where
		Self: Sized,
	{
		wgpu::VertexBufferLayout {
			array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode,
			attributes: &Self::ATTRIBUTES,
		}
	}
}
