// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use enumset::EnumSet;

use crate::{
	app::{App, ClipboardContents, PreFullscreenState},
	canvas::{Image, Operation, Stroke},
	clipboard::ClipboardData,
	input::{
		keymap::{Action, Keymap},
		Key,
	},
	pixel::{Px, Vex, Vx},
	tools::TransientModeSwitch,
	utility::hsv_to_srgba8,
};

pub fn default_keymap() -> Keymap {
	let mut keymap = Keymap::new();
	const NONE: EnumSet<Key> = EnumSet::EMPTY;
	use Key::*;

	keymap.insert(NONE, B, false, trigger(choose_draw_tool));
	keymap.insert(NONE, Backspace, false, trigger(delete_selected_items));
	keymap.insert(LControl | LShift, F, false, trigger(toggle_fullscreen));
	keymap.insert(LControl, F, false, trigger(toggle_maximized));
	keymap.insert(LControl, X, false, trigger(cut));
	keymap.insert(LControl, C, false, trigger(copy));
	keymap.insert(LControl, V, false, trigger(paste));
	keymap.insert(NONE, A, false, trigger(select_all));
	keymap.insert(LShift, A, false, trigger(select_none));
	keymap.insert(Tab, R, false, trigger(recolor_selection));
	keymap.insert(NONE, S, false, trigger(choose_select_tool));
	keymap.insert(NONE, T, false, trigger(choose_move_tool));
	keymap.insert(NONE, Z, true, trigger(undo));
	keymap.insert(LShift, Z, true, trigger(redo));
	keymap.insert(NONE, Escape, false, trigger(discard_draft));

	keymap.insert(NONE, Space, false, discovery(hold_pan_tool, release_pan_tool));
	keymap.insert(NONE, LControl | Space, false, discovery(hold_zoom_tool, release_zoom_tool));
	keymap.insert(NONE, LShift | Space, false, discovery(hold_orbit_tool, release_orbit_tool));
	keymap.insert(NONE, Tab, false, discovery(hold_color_picker_tool, release_color_picker_tool));

	keymap
}

pub fn trigger(on_trigger: fn(&mut App)) -> Action {
	Action::Trigger { on_trigger }
}

pub fn discovery(on_press: fn(&mut App), on_release: fn(&mut App)) -> Action {
	Action::Discovery { on_press, on_release }
}

// Actions:

fn discard_draft(app: &mut App) {
	app.mode_stack.discard_draft();
}

fn choose_draw_tool(app: &mut App) {
	app.mode_stack.switch_draw();
}

fn choose_select_tool(app: &mut App) {
	app.mode_stack.switch_select();
}

fn choose_move_tool(app: &mut App) {
	app.mode_stack.switch_move();
}

fn hold_pan_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Pan { should_pan: true });
}

fn release_pan_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Pan { should_pan: false });
}

fn hold_zoom_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Zoom { should_zoom: true });
}

fn release_zoom_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Zoom { should_zoom: false });
}

fn hold_orbit_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Orbit { should_orbit: true });
}

fn release_orbit_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Orbit { should_orbit: false });
}

fn hold_color_picker_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Color {
		center: Some(if app.is_cursor_relevant {
			app.cursor_physical_position
		} else {
			Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px))
		}),
	});
}

fn release_color_picker_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Color { center: None });
}

fn delete_selected_items(app: &mut App) {
	let selected_indices = app.canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

	if selected_indices.len() > 0 {
		app.canvas.perform_operation(Operation::DeleteStrokes { monotone_indices: selected_indices });
	}
}

fn toggle_fullscreen(app: &mut App) {
	// On Windows, we enable fullscreen this way to allow the window to gracefully handle defocusing.
	#[cfg(target_os = "windows")]
	{
		use winit::platform::windows::WindowExtWindows;
		if let Some(pre_fullscreen_state) = app.pre_fullscreen_state {
			app.pre_fullscreen_state = None;
			crate::windows::set_unfullscreen(app.window.hwnd(), pre_fullscreen_state);
		} else {
			app.pre_fullscreen_state = Some(if app.window.is_maximized() { PreFullscreenState::Maximized } else { PreFullscreenState::Normal });
			crate::windows::set_fullscreen(app.window.hwnd());
		}
	}

	#[cfg(not(target_os = "windows"))]
	app.window.set_fullscreen(if app.window.fullscreen().is_some() { None } else { Some(winit::window::Fullscreen::Borderless(None)) });
}

fn toggle_maximized(app: &mut App) {
	app.pre_fullscreen_state = None;
	app.window.set_maximized(!app.window.is_maximized());
}

fn undo(app: &mut App) {
	if app.mode_stack.is_drafting() {
		app.mode_stack.discard_draft();
	} else {
		app.canvas.undo();
	}
}

fn redo(app: &mut App) {
	if app.mode_stack.is_drafting() {
		app.mode_stack.discard_draft();
	} else {
		app.canvas.redo();
	}
}

fn cut(app: &mut App) {
	let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
	let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);
	let offset = cursor_virtual_position + app.position;

	let (indices, strokes): (Vec<_>, Vec<_>) = app
		.canvas
		.strokes()
		.iter()
		.enumerate()
		.filter_map(|(index, stroke)| {
			if stroke.is_selected {
				Some((
					index,
					Stroke {
						origin: stroke.origin - offset,
						is_selected: true,
						..stroke.clone()
					},
				))
			} else {
				None
			}
		})
		.unzip();

	if indices.len() > 0 {
		app.canvas.perform_operation(Operation::DeleteStrokes { monotone_indices: indices });
	}

	app.clipboard_contents = Some(ClipboardContents::Subcanvas(strokes));
	app.clipboard.write(ClipboardData::Custom);
}

fn copy(app: &mut App) {
	let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
	let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);
	let offset = cursor_virtual_position + app.position;

	let strokes: Vec<_> = app
		.canvas
		.strokes()
		.iter()
		.filter_map(|stroke| {
			if stroke.is_selected {
				Some(Stroke {
					origin: stroke.origin - offset,
					is_selected: true,
					..stroke.clone()
				})
			} else {
				None
			}
		})
		.collect();

	app.clipboard_contents = Some(ClipboardContents::Subcanvas(strokes));
	app.clipboard.write(ClipboardData::Custom);
}

fn paste(app: &mut App) {
	match app.clipboard.read() {
		Some(ClipboardData::Custom) => {
			if let Some(ClipboardContents::Subcanvas(strokes)) = app.clipboard_contents.as_ref() {
				let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
				let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);

				app.canvas.select_all(false);

				let offset = cursor_virtual_position + app.position;

				if strokes.len() > 0 {
					app.canvas.perform_operation(Operation::CommitStrokes {
						strokes: strokes
							.iter()
							.map(|stroke| Stroke {
								origin: stroke.origin + offset,
								is_selected: true,
								..stroke.clone()
							})
							.collect(),
					})
				}
			}
		},
		Some(ClipboardData::Image { dimensions, data }) => {
			let texture_index = app.renderer.push_texture(dimensions, data);

			app.canvas.perform_operation(Operation::PasteImage {
				image: Image {
					texture_index,
					position: app.position,
					dimensions: Vex(dimensions.map(|x| Vx(x as f32))),
				},
			});
		},
		_ => {},
	}
}

fn select_all(app: &mut App) {
	app.canvas.select_all(true);
}

fn select_none(app: &mut App) {
	app.canvas.select_all(false);
}

fn recolor_selection(app: &mut App) {
	let selected_indices = app.canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

	if selected_indices.len() > 0 {
		app.canvas.perform_operation(Operation::RecolorStrokes {
			indices: selected_indices,
			new_color: hsv_to_srgba8(app.current_color),
		});
	}
}
