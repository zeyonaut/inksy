// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use windows_sys::Win32::{
	Foundation::{HWND, WPARAM},
	System::LibraryLoader::GetModuleHandleW,
	UI::WindowsAndMessaging::{LoadIconW, SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON},
};

pub fn set_window_icon(hwnd: HWND) {
	unsafe {
		// NOTE: This value should be synchronized with the definition of APP_ICON in the resource file.
		const APP_ICON: u16 = 1;
		let icon = LoadIconW(GetModuleHandleW(0 as _), APP_ICON as _);
		SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, icon);
		SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, icon);
	}
}
