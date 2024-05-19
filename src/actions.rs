// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::num::NonZero;

use enumset::EnumSet;

use crate::{
	app::{App, ClipboardContents, PreFullscreenState},
	canvas::{Canvas, Image, Operation, Stroke},
	clipboard::ClipboardData,
	file::{load_canvas_from_file, save_canvas_to_file},
	input::{
		keymap::{Action, Keymap},
		Key,
	},
	tools::TransientModeSwitch,
	utility::{Px, Vex, Vx},
};

pub fn default_keymap() -> Keymap {
	let mut keymap = Keymap::new();
	const NONE: EnumSet<Key> = EnumSet::EMPTY;
	use Key::*;

	keymap.insert(NONE, F3, false, trigger(toggle_debug_mode));
	keymap.insert(Control | Shift, S, false, trigger(save_as_file));
	keymap.insert(Control, S, false, trigger(save_file));
	keymap.insert(Control, O, false, trigger(load_from_file));
	keymap.insert(Control, N, false, trigger(new_file));
	keymap.insert(Control, W, false, trigger(close_tab));
	keymap.insert(Control, LeftArrow, false, trigger(switch_tab_left));
	keymap.insert(Control, RightArrow, false, trigger(switch_tab_right));
	keymap.insert(NONE, B, false, trigger(choose_draw_tool));
	keymap.insert(NONE, Backspace, false, trigger(delete_selected_items));
	keymap.insert(Control | Shift, F, false, trigger(toggle_fullscreen));
	keymap.insert(Control, F, false, trigger(toggle_maximized));
	keymap.insert(Control, X, false, trigger(cut));
	keymap.insert(Control, C, false, trigger(copy));
	keymap.insert(Control, V, false, trigger(paste));
	keymap.insert(NONE, A, false, trigger(select_all));
	keymap.insert(Shift, A, false, trigger(select_none));
	keymap.insert(Tab, R, false, trigger(recolor_selection));
	keymap.insert(NONE, S, false, trigger(choose_select_tool));
	keymap.insert(NONE, T, false, trigger(choose_move_tool));
	keymap.insert(Shift, R, false, trigger(choose_rotate_tool));
	keymap.insert(Control, R, false, trigger(choose_resize_tool));
	keymap.insert(NONE, Z, true, trigger(undo));
	keymap.insert(Shift, Z, true, trigger(redo));
	keymap.insert(NONE, Escape, false, trigger(discard_draft));

	keymap.insert(NONE, Space, false, discovery(hold_pan_tool, release_pan_tool));
	keymap.insert(NONE, Control | Space, false, discovery(hold_zoom_tool, release_zoom_tool));
	keymap.insert(NONE, Shift | Space, false, discovery(hold_orbit_tool, release_orbit_tool));
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

fn toggle_debug_mode(app: &mut App) {
	app.is_debug_mode_on ^= true;
}

fn save_as_file(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		if let Some(file_path) = rfd::FileDialog::new().add_filter("Inksy", &["inksy"]).save_file() {
			if save_canvas_to_file(canvas, &app.renderer, &file_path).is_some() {
				canvas.file_path = Some(file_path).into();
				canvas.set_retraction_count_at_save();
			}
		}
	}
}

fn save_file(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		if let Some(file_path) = canvas.file_path.as_ref().as_ref() {
			if save_canvas_to_file(canvas, &app.renderer, file_path).is_some() {
				canvas.set_retraction_count_at_save();
			}
		} else {
			save_as_file(app);
		}
	}
}

fn load_from_file(app: &mut App) {
	app.current_canvas_index.map(|current_canvas_index| app.canvases.get_mut(current_canvas_index).map(Canvas::invalidate));
	if let Some(file_path) = rfd::FileDialog::new().add_filter("Inksy", &["inksy"]).pick_file() {
		if let Some(canvas) = load_canvas_from_file(&mut app.renderer, file_path) {
			let new_canvas_index = app.current_canvas_index.map_or(0, |x| x + 1);
			app.canvases.insert(new_canvas_index, canvas);
			app.current_canvas_index = Some(new_canvas_index);
		}
	}
	app.update_window_title();
}

fn new_file(app: &mut App) {
	app.current_canvas_index.map(|current_canvas_index| app.canvases.get_mut(current_canvas_index).map(Canvas::invalidate));
	let new_canvas_index = app.current_canvas_index.map_or(0, |x| x + 1);
	app.canvases.insert(new_canvas_index, Canvas::new(&app.config));
	app.current_canvas_index = Some(new_canvas_index);
	app.update_window_title();
}

fn close_tab(app: &mut App) {
	if let Some(current_canvas_index) = app.current_canvas_index {
		app.canvases.get_mut(current_canvas_index).map(Canvas::invalidate);
		app.canvases.remove(current_canvas_index);
		if current_canvas_index > 0 {
			app.current_canvas_index = Some(current_canvas_index - 1);
		} else if app.canvases.is_empty() {
			app.current_canvas_index = None;
		}
	}
	app.update_window_title();
}

fn switch_tab_left(app: &mut App) {
	if let Some(current_canvas_index) = app.current_canvas_index {
		app.canvases.get_mut(current_canvas_index).map(Canvas::invalidate);
		if !app.canvases.is_empty() {
			app.current_canvas_index = Some(current_canvas_index.checked_sub(1).unwrap_or(app.canvases.len() - 1));
		}
	}
	app.update_window_title();
}

fn switch_tab_right(app: &mut App) {
	if let Some(current_canvas_index) = app.current_canvas_index {
		app.canvases.get_mut(current_canvas_index).map(Canvas::invalidate);
		if !app.canvases.is_empty() {
			app.current_canvas_index = Some((current_canvas_index + 1) % app.canvases.len());
		}
	}
	app.update_window_title();
}

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

fn choose_rotate_tool(app: &mut App) {
	app.mode_stack.switch_rotate();
}

fn choose_resize_tool(app: &mut App) {
	app.mode_stack.switch_resize();
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
			Vex([app.renderer.config.width as f32 / 2., app.renderer.config.height as f32 / 2.].map(Px))
		}),
	});
}

fn release_color_picker_tool(app: &mut App) {
	app.mode_stack.switch_transient(TransientModeSwitch::Color { center: None });
}

fn delete_selected_items(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		let selected_image_indices = canvas.images().iter().enumerate().filter_map(|(index, image)| if image.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

		let selected_stroke_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

		if !selected_image_indices.is_empty() || !selected_stroke_indices.is_empty() {
			canvas.perform_operation(Operation::DeleteObjects {
				monotone_image_indices: selected_image_indices,
				monotone_stroke_indices: selected_stroke_indices,
			});
		}
	}
}

fn toggle_fullscreen(app: &mut App) {
	// On Windows, we enable fullscreen this way to allow the window to gracefully handle defocusing.
	#[cfg(target_os = "windows")]
	{
		if let Some(pre_fullscreen_state) = app.pre_fullscreen_state {
			app.pre_fullscreen_state = None;
			crate::windows::set_unfullscreen(crate::windows::window_hwnd(app.window).into(), pre_fullscreen_state);
			if let PreFullscreenState::Normal(outer_position, inner_size) = pre_fullscreen_state {
				app.window.set_outer_position(outer_position);
				let _ = app.window.request_inner_size(inner_size);
			}
		} else {
			app.pre_fullscreen_state = Some(if app.window.is_maximized() {
				PreFullscreenState::Maximized
			} else {
				PreFullscreenState::Normal(app.window.outer_position().unwrap_or_default(), app.window.inner_size())
			});
			crate::windows::set_fullscreen(crate::windows::window_hwnd(app.window).into());
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
	} else if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		canvas.undo();
	}
}

fn redo(app: &mut App) {
	if app.mode_stack.is_drafting() {
		app.mode_stack.discard_draft();
	} else if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		canvas.redo();
	}
}

fn cut(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		let semidimensions = Vex([app.renderer.config.width as f32 / 2., app.renderer.config.height as f32 / 2.].map(Px)).s(app.scale).z(canvas.view.zoom);
		let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(canvas.view.zoom) - semidimensions).rotate(-canvas.view.tilt);
		let offset = cursor_virtual_position + canvas.view.position;

		let (image_indices, images): (Vec<_>, Vec<_>) = canvas
			.images()
			.iter()
			.enumerate()
			.filter_map(|(index, image)| {
				if image.is_selected {
					Some((
						index,
						Image {
							position: image.position - offset,
							..(*image).clone()
						},
					))
				} else {
					None
				}
			})
			.unzip();

		let (stroke_indices, strokes): (Vec<_>, Vec<_>) = canvas
			.strokes()
			.iter()
			.enumerate()
			.filter_map(|(index, stroke)| {
				if stroke.is_selected {
					Some((
						index,
						Stroke {
							position: stroke.position - offset,
							..(*stroke).clone()
						},
					))
				} else {
					None
				}
			})
			.unzip();

		if !stroke_indices.is_empty() {
			canvas.perform_operation(Operation::DeleteObjects {
				monotone_image_indices: image_indices,
				monotone_stroke_indices: stroke_indices,
			});
		}

		app.clipboard_contents = Some(ClipboardContents::Subcanvas(images, strokes));
		app.clipboard.write(ClipboardData::Custom);
	}
}

fn copy(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get(x)) {
		let semidimensions = Vex([app.renderer.config.width as f32 / 2., app.renderer.config.height as f32 / 2.].map(Px)).s(app.scale).z(canvas.view.zoom);
		let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(canvas.view.zoom) - semidimensions).rotate(-canvas.view.tilt);
		let offset = cursor_virtual_position + canvas.view.position;

		let images: Vec<_> = canvas
			.images()
			.iter()
			.filter_map(|image| {
				if image.is_selected {
					Some(Image {
						position: image.position - offset,
						..(*image).clone()
					})
				} else {
					None
				}
			})
			.collect();

		let strokes: Vec<_> = canvas
			.strokes()
			.iter()
			.filter_map(|stroke| {
				if stroke.is_selected {
					Some(Stroke {
						position: stroke.position - offset,
						..(*stroke).clone()
					})
				} else {
					None
				}
			})
			.collect();

		app.clipboard_contents = Some(ClipboardContents::Subcanvas(images, strokes));
		app.clipboard.write(ClipboardData::Custom);
	}
}

fn paste(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		match app.clipboard.read() {
			Some(ClipboardData::Custom) => {
				if let Some(ClipboardContents::Subcanvas(images, strokes)) = app.clipboard_contents.as_ref() {
					let semidimensions = Vex([app.renderer.config.width as f32 / 2., app.renderer.config.height as f32 / 2.].map(Px)).s(app.scale).z(canvas.view.zoom);
					let cursor_virtual_position = (app.cursor_physical_position.s(app.scale).z(canvas.view.zoom) - semidimensions).rotate(-canvas.view.tilt);

					canvas.select_all(false);

					let offset = cursor_virtual_position + canvas.view.position;

					if !images.is_empty() {
						canvas.perform_operation(Operation::CommitImages {
							images: images
								.iter()
								.map(|image| {
									Image {
										position: image.position + offset,
										is_selected: true,
										..image.clone()
									}
									.into()
								})
								.collect(),
						})
					}

					if !strokes.is_empty() {
						canvas.perform_operation(Operation::CommitStrokes {
							strokes: strokes
								.iter()
								.map(|stroke| {
									Stroke {
										position: stroke.position + offset,
										is_selected: true,
										..stroke.clone()
									}
									.into()
								})
								.collect(),
						})
					}
				}
			},
			Some(ClipboardData::Image { dimensions, data }) => 'empty: {
				let Ok(width) = NonZero::try_from(dimensions[0]) else { break 'empty };
				let Ok(height) = NonZero::try_from(dimensions[1]) else { break 'empty };
				let dimensions = [width, height];
				let texture_index = canvas.push_texture(&app.renderer, dimensions, data);

				canvas.perform_operation(Operation::CommitImages {
					images: vec![Image {
						texture_index,
						dimensions: Vex(dimensions.map(|x| Vx(x.get() as f32))),
						position: canvas.view.position,
						orientation: canvas.view.tilt,
						dilation: 1.,
						is_selected: false,
					}
					.into()],
				});
			},
			_ => {},
		}
	}
}

fn select_all(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		canvas.select_all(true);
	}
}

fn select_none(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		canvas.select_all(false);
	}
}

fn recolor_selection(app: &mut App) {
	if let Some(canvas) = app.current_canvas_index.and_then(|x| app.canvases.get_mut(x)) {
		let selected_indices = canvas.strokes().iter().enumerate().filter_map(|(index, stroke)| if stroke.is_selected { Some(index) } else { None }).collect::<Vec<_>>();

		if !selected_indices.is_empty() {
			canvas.perform_operation(Operation::RecolorStrokes {
				indices: selected_indices,
				new_color: canvas.stroke_color.to_srgb().to_srgb8().opaque(),
			});
		}
	}
}
