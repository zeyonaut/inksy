// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![cfg(target_os = "windows")]

use std::{
	ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void},
	mem::size_of,
};

use bitflags::bitflags;
use winit::platform::windows::WindowExtWindows;

/*
char : c_char
UINT : c_uint
WTPKT, DWORD, FIX32 : c_ulong
LONG : c_long
BOOL, int : c_int
HWND: isize
*/

bitflags! {
	#[repr(C)]
	pub struct ContextOptions: c_uint {
		const SYSTEM      = 0x0001;
		const PEN         = 0x0002;
		const MESSAGES    = 0x0004;
		const MARGIN      = 0x8000;
		const MGINSIDE    = 0x4000;
		const CSRMESSAGES = 0x0008;
	}

	#[repr(C)]
	pub struct PacketFields: c_ulong {
		const CONTEXT          = 0x0001;
		const STATUS           = 0x0002;
		const TIME             = 0x0004;
		const CHANGED          = 0x0008;
		const SERIAL_NUMBER    = 0x0010;
		const CURSOR           = 0x0020;
		const BUTTONS          = 0x0040;
		const X                = 0x0080;
		const Y                = 0x0100;
		const Z                = 0x0200;
		const NORMAL_PRESSURE  = 0x0400;
		const TANGENT_PRESSURE = 0x0800;
		const ORIENTATION      = 0x1000;
		const ROTATION         = 0x2000;
	}
}

#[repr(C)]
pub struct LogicalContext {
	pub name: [c_char; 40],
	pub options: ContextOptions,
	pub status: c_uint,
	pub locks: c_uint,
	pub msg_base: c_uint,
	pub device: c_uint,
	pub pkt_rate: c_uint,
	pub pkt_data: PacketFields,
	pub pkt_mode: PacketFields,
	pub move_mask: PacketFields,
	pub btn_dn_mask: c_ulong,
	pub btn_up_mask: c_ulong,
	pub in_org_x: c_long,
	pub in_org_y: c_long,
	pub in_org_z: c_long,
	pub in_ext_x: c_long,
	pub in_ext_y: c_long,
	pub in_ext_z: c_long,
	pub out_org_x: c_long,
	pub out_org_y: c_long,
	pub out_org_z: c_long,
	pub out_ext_x: c_long,
	pub out_ext_y: c_long,
	pub out_ext_z: c_long,
	pub sens_x: c_ulong,
	pub sens_y: c_ulong,
	pub sens_z: c_ulong,
	pub sys_mode: c_int,
	pub sys_org_x: c_int,
	pub sys_org_y: c_int,
	pub sys_ext_x: c_int,
	pub sys_ext_y: c_int,
	pub sys_sens_x: c_ulong,
	pub sys_sens_y: c_ulong,
}

impl LogicalContext {
	const WTI_DEFSYSCTX: c_uint = 4; // Sets CXO_SYSTEM; see https://developer-docs.wacom.com/intuos-cintiq-business-tablets/docs/wintab-faqs for details

	pub fn default_system() -> Option<Self> {
		unsafe {
			let mut logical_context: Self = std::mem::zeroed();
			let lib = libloading::Library::new("wintab32.dll").unwrap();
			#[allow(non_snake_case)]
			let WTInfoA: libloading::Symbol<unsafe extern "C" fn(wCategory: c_uint, nIndex: c_uint, lpOutput: *mut LogicalContext) -> c_uint> = lib.get(b"WTInfoA").unwrap();
			match WTInfoA(Self::WTI_DEFSYSCTX, 0, &mut logical_context) {
				0 => None,
				x => {
					assert!(x as usize == size_of::<LogicalContext>());
					Some(logical_context)
				},
			}
		}
	}
}

#[repr(C)]
pub struct Packet {
	pub normal_pressure: c_uint,
}

impl Packet {
	const DATA: PacketFields = PacketFields::NORMAL_PRESSURE;
}

macro_rules! impl_interface {
	{$Name:ident: $($function:ident: fn($($parameter:ident: $factor:ty),*) -> $codomain:ty),* $(,)?} => {
		#[allow(non_snake_case, dead_code)]
		struct $Name {
			$($function: unsafe extern "C" fn($($parameter: $factor),*) -> $codomain),*
		}

		impl $Name {
			fn new(library: &libloading::Library) -> Option<Self> {
				Some(Self {
					$($function: *(unsafe { library.get(concat!(stringify!($function), "\0").as_bytes()) }.ok())?),*
				})
			}
		}
	}
}

impl_interface! {
	WintabInterface:
	WTOpenA: fn(hWnd: isize, lpLogCtx: *const LogicalContext, fEnable: c_uint) -> *const c_void,
	WTEnable: fn(hCtx: *const c_void, fEnable: c_uint) -> c_uint,
	WTQueueSizeGet: fn(hCtx: *const c_void) -> c_int,
	WTPacketsGet: fn(hCtx: *const c_void, cMaxPkts: c_int, lpPkts: *mut c_void) -> c_int,
	WTGetA: fn(hCtx: *const c_void, lpLogCtx: *mut LogicalContext) -> c_int,
	WTClose: fn(hCtx: *const c_void) -> c_int,
}

pub struct TabletContext {
	_wintab_library: libloading::Library,
	wintab: WintabInterface,
	pub handle: *const c_void,
}

impl TabletContext {
	pub fn new(window: &winit::window::Window) -> Option<Self> {
		let mut logical_context = LogicalContext::default_system()?;
		logical_context.pkt_data = Packet::DATA;
		logical_context.options &= !ContextOptions::MESSAGES;
		logical_context.btn_up_mask = logical_context.btn_dn_mask;

		let wintab_library = unsafe { libloading::Library::new("wintab32.dll").ok()? };
		let wintab = WintabInterface::new(&wintab_library)?;

		let handle = unsafe { (wintab.WTOpenA)(window.hwnd(), &logical_context, false as c_uint) };
		if handle.is_null() {
			None
		} else {
			Some(Self { _wintab_library: wintab_library, wintab, handle })
		}
	}

	pub fn enable(&mut self, enable: bool) -> Result<(), ()> {
		unsafe {
			let is_request_satisfied = (self.wintab.WTEnable)(self.handle, enable as c_uint);
			if is_request_satisfied != 0 {
				Ok(())
			} else {
				Err(())
			}
		}
	}

	pub fn get_queue_size(&self) -> isize {
		unsafe { (self.wintab.WTQueueSizeGet)(self.handle) as isize }
	}

	pub fn get_packets(&mut self, num: usize) -> Box<[Packet]> {
		unsafe {
			let mut buf = Vec::with_capacity(num);
			let len = (self.wintab.WTPacketsGet)(self.handle, num as c_int, buf.as_mut_ptr() as *mut c_void) as usize;
			buf.set_len(len);
			buf.into_boxed_slice()
		}
	}

	pub fn get(&self) -> Option<LogicalContext> {
		unsafe {
			let mut logical_context: LogicalContext = std::mem::zeroed();
			let success = (self.wintab.WTGetA)(self.handle, &mut logical_context);
			if success != 0 {
				Some(logical_context)
			} else {
				None
			}
		}
	}
}

impl Drop for TabletContext {
	fn drop(&mut self) {
		unsafe {
			(self.wintab.WTClose)(self.handle);
		}
	}
}
