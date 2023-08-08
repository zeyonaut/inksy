// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

struct PhysicalSize {
	width: u32,
	height: u32,
}

trait Component {
	fn minimum_dimensions() -> PhysicalSize;
	fn maximum_dimensions() -> PhysicalSize;
}


pub enum Widget {
	HorizontalGroup(HorizontalGroup),
}

pub struct Container {
	components: Vec<Widget>,
	// A component in a container can be fixed (size does not distribute when resizing, so it stays the same size)
	// or distributive (so size distributes proportional to the proportion it took up with respect to the size of the other)
	// distributive components before the resize.
}