// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::mem::MaybeUninit;

use windows_sys::Win32::{
	Foundation::{HWND, WPARAM},
	System::LibraryLoader::GetModuleHandleW,
	UI::WindowsAndMessaging::{
		GetWindowLongPtrW, GetWindowPlacement, LoadIconW, SendMessageW, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, GWL_STYLE, ICON_BIG, ICON_SMALL, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWMAXIMIZED, SW_SHOWNORMAL,
		WINDOWPLACEMENT, WM_SETICON, WS_CAPTION, WS_MAXIMIZE,
	},
};

use crate::app::PreFullscreenState;

pub fn set_window_icon(hwnd: HWND) {
	unsafe {
		// NOTE: This value should be synchronized with the definition of APP_ICON in the resource file.
		const APP_ICON: u16 = 1;
		let icon = LoadIconW(GetModuleHandleW(0 as _), APP_ICON as _);
		SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, icon);
		SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, icon);
	}
}

pub fn set_fullscreen(hwnd: HWND) {
	unsafe {
		let window_style = GetWindowLongPtrW(hwnd, GWL_STYLE) & !(WS_CAPTION | WS_MAXIMIZE) as isize;

		let mut window_placement = MaybeUninit::<WINDOWPLACEMENT>::uninit();
		GetWindowPlacement(hwnd, window_placement.as_mut_ptr());
		let mut window_placement = window_placement.assume_init();
		window_placement.showCmd = SW_SHOWMAXIMIZED as _;

		SetWindowLongPtrW(hwnd, GWL_STYLE, window_style);
		SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
		SetWindowPlacement(hwnd, &window_placement);
	}
}

pub fn set_unfullscreen(hwnd: HWND, pre_fullscreen_state: PreFullscreenState) {
	unsafe {
		let window_style = GetWindowLongPtrW(hwnd, GWL_STYLE) | WS_CAPTION as isize;

		let positioning_flag = match pre_fullscreen_state {
			PreFullscreenState::Maximized => SWP_FRAMECHANGED,
			PreFullscreenState::Normal(..) => 0,
		};

		let mut window_placement = MaybeUninit::<WINDOWPLACEMENT>::uninit();
		GetWindowPlacement(hwnd, window_placement.as_mut_ptr());
		let mut window_placement = window_placement.assume_init();
		window_placement.showCmd = match pre_fullscreen_state {
			PreFullscreenState::Maximized => SW_SHOWMAXIMIZED,
			PreFullscreenState::Normal(..) => SW_SHOWNORMAL,
		} as _;

		SetWindowLongPtrW(hwnd, GWL_STYLE, window_style);
		SetWindowPos(hwnd, 0, 0, 0, 0, 0, positioning_flag | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
		SetWindowPlacement(hwnd, &window_placement);
	}
}

#[cfg(target_os = "windows")]
pub fn window_hwnd(window: &winit::window::Window) -> std::num::NonZero<isize> {
	use raw_window_handle::HasWindowHandle;
	let raw_window_handle::RawWindowHandle::Win32(rwh) = window.window_handle().unwrap().as_raw() else { unreachable!() };
	rwh.hwnd
}
