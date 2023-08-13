// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use enumset::EnumSet;

use crate::{
	app::{App, ClipboardContents},
	input::{
		keymap::{Action, Keymap},
		Key,
	},
	pixel::{Px, Vex},
	stroke::Stroke,
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
	for _ in app.canvas.strokes.extract_if(|x| x.is_selected) {}
}

fn toggle_fullscreen(app: &mut App) {
	if app.is_fullscreen {
		app.window.set_fullscreen(None);
	} else {
		app.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(app.window.current_monitor())));
	}
	app.is_fullscreen = !app.is_fullscreen;
}

fn toggle_maximized(app: &mut App) {
	app.window.set_maximized(!app.window.is_maximized());
}

fn undo(app: &mut App) {
	if app.mode_stack.is_drafting() {
		app.mode_stack.discard_draft();
	} else {
		app.canvas.strokes.pop();
	}
}

fn cut(app: &mut App) {
	let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
	let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);
	let offset = cursor_virtual_position + app.position;
	app.clipboard_contents = Some(ClipboardContents::Subcanvas(
		app.canvas
			.strokes
			.extract_if(|x| {
				if x.is_selected {
					x.origin = x.origin - offset;
					x.is_selected = false;
					true
				} else {
					false
				}
			})
			.collect(),
	));
}

fn copy(app: &mut App) {
	let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
	let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);
	let offset = cursor_virtual_position + app.position;
	app.clipboard_contents = Some(ClipboardContents::Subcanvas(
		app.canvas
			.strokes
			.iter()
			.filter(|x| x.is_selected)
			.map(|stroke| Stroke {
				origin: stroke.origin - offset,
				color: stroke.color,
				points: stroke.points.clone(),
				is_selected: false,
				max_pressure: stroke.max_pressure,
			})
			.collect::<Vec<_>>(),
	))
}

fn paste(app: &mut App) {
	if let Some(ClipboardContents::Subcanvas(strokes)) = app.clipboard_contents.as_ref() {
		let semidimensions = Vex([app.renderer.width as f32 / 2., app.renderer.height as f32 / 2.].map(Px)).s(app.scale).z(app.zoom);
		let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(app.zoom) - semidimensions).rotate(-app.tilt);
		for stroke in app.canvas.strokes.iter_mut() {
			stroke.is_selected = false;
		}
		let offset = cursor_virtual_position + app.position;
		app.canvas.strokes.extend(strokes.iter().map(|stroke| Stroke {
			origin: stroke.origin + offset,
			color: stroke.color,
			points: stroke.points.clone(),
			is_selected: true,
			max_pressure: stroke.max_pressure,
		}));
	}
}

fn select_all(app: &mut App) {
	for stroke in app.canvas.strokes.iter_mut() {
		stroke.is_selected = true;
	}
}

fn select_none(app: &mut App) {
	for stroke in app.canvas.strokes.iter_mut() {
		stroke.is_selected = false;
	}
}

fn recolor_selection(app: &mut App) {
	for stroke in app.canvas.strokes.iter_mut().filter(|stroke| stroke.is_selected) {
		stroke.color = hsv_to_srgba8(app.current_color);
	}
}
