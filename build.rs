// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(target_os = "windows")]
extern crate embed_resource;

fn main() {
	// Create the application icon for Windows.
	#[cfg(target_os = "windows")]
	embed_resource::compile("res/icon.rc", embed_resource::NONE);
}
