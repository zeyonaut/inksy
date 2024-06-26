// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{canvas::*, utility::*};

pub struct PanOrigin {
	pub cursor: Vex<2, Vx>,
	pub position: Vex<2, Vx>,
}

pub struct ZoomOrigin {
	pub initial_zoom: f32,
	pub initial_y_ratio: f32,
}

pub struct OrbitInitial {
	pub tilt: f32,
	pub cursor_angle: f32,
}

pub struct RotateDraft {
	pub center: Vex<2, Vx>,
	pub initial_position: Vex<2, Vx>,
}

pub struct ResizeDraft {
	pub center: Vex<2, Vx>,
	pub initial_distance: Vx,
}

pub enum ColorSelectionPart {
	Hue,
	SaturationValue,
}

pub enum Tool {
	Draw { current_stroke: Option<IncompleteStroke> },
	Select { origin: Option<Vex<2, Vx>> },
	Pan { origin: Option<PanOrigin> },
	Zoom { origin: Option<ZoomOrigin> },
	Orbit { initial: Option<OrbitInitial> },
	Move { origin: Option<Vex<2, Vx>> },
	Rotate { origin: Option<RotateDraft> },
	Resize { origin: Option<ResizeDraft> },
	PickColor { cursor_physical_origin: Vex<2, Px>, part: Option<ColorSelectionPart> },
}

pub enum TransientModeSwitch {
	Pan { should_pan: bool },
	Zoom { should_zoom: bool },
	Orbit { should_orbit: bool },
	Color { center: Option<Vex<2, Px>> },
}

pub struct ModeStack {
	pub base_mode: Tool,
	pub transient_mode: Option<Tool>,
	pub discarded_transformation_draft: Tracked<()>,
}

impl ModeStack {
	pub fn new(mode: Tool) -> Self {
		Self {
			base_mode: mode,
			transient_mode: None,
			discarded_transformation_draft: ().into(),
		}
	}

	pub fn get(&self) -> &Tool {
		self.transient_mode.as_ref().unwrap_or(&self.base_mode)
	}

	pub fn get_mut(&mut self) -> &mut Tool {
		self.transient_mode.as_mut().unwrap_or(&mut self.base_mode)
	}

	pub fn switch_transient(&mut self, switch: TransientModeSwitch) {
		match switch {
			TransientModeSwitch::Pan { should_pan } => {
				if should_pan {
					if !matches!(self.get(), &Tool::Pan { .. }) {
						self.transient_mode = Some(Tool::Pan { origin: None });
					}
				} else if matches!(self.get(), &Tool::Pan { .. }) {
					self.transient_mode = None;
				}
			},
			TransientModeSwitch::Zoom { should_zoom } => {
				if should_zoom {
					if !matches!(self.get(), &Tool::Zoom { .. }) {
						self.transient_mode = Some(Tool::Zoom { origin: None });
					}
				} else if matches!(self.get(), &Tool::Zoom { .. }) {
					self.transient_mode = None;
				}
			},
			TransientModeSwitch::Orbit { should_orbit } => {
				if should_orbit {
					if !matches!(self.get(), &Tool::Orbit { .. }) {
						self.transient_mode = Some(Tool::Orbit { initial: None });
					}
				} else if matches!(self.get(), &Tool::Orbit { .. }) {
					self.transient_mode = None;
				}
			},
			TransientModeSwitch::Color { center } => {
				if let Some(center) = center {
					if !matches!(self.get(), &Tool::PickColor { .. }) {
						self.transient_mode = Some(Tool::PickColor { cursor_physical_origin: center, part: None });
					}
				} else if matches!(self.get(), &Tool::PickColor { .. }) {
					self.transient_mode = None;
				}
			},
		}
	}

	pub fn switch_select(&mut self) {
		if !matches!(self.base_mode, Tool::Select { .. }) {
			self.invalidate_base_transformation_draft();
			self.base_mode = Tool::Select { origin: None }
		}
	}

	pub fn switch_draw(&mut self) {
		if !matches!(self.base_mode, Tool::Draw { .. }) {
			self.invalidate_base_transformation_draft();
			self.base_mode = Tool::Draw { current_stroke: None }
		}
	}

	pub fn switch_move(&mut self) {
		if !matches!(self.base_mode, Tool::Move { .. }) {
			self.invalidate_base_transformation_draft();
			self.base_mode = Tool::Move { origin: None }
		}
	}

	pub fn switch_rotate(&mut self) {
		if !matches!(self.base_mode, Tool::Rotate { .. }) {
			self.invalidate_base_transformation_draft();
			self.base_mode = Tool::Rotate { origin: None }
		}
	}

	pub fn switch_resize(&mut self) {
		if !matches!(self.base_mode, Tool::Resize { .. }) {
			self.invalidate_base_transformation_draft();
			self.base_mode = Tool::Resize { origin: None }
		}
	}

	pub fn invalidate_base_transformation_draft(&mut self) {
		match self.get() {
			Tool::Move { .. } | Tool::Rotate { .. } | Tool::Resize { .. } => self.discarded_transformation_draft.invalidate(),
			_ => {},
		}
	}

	pub fn is_drafting(&mut self) -> bool {
		match self.get_mut() {
			Tool::Draw { current_stroke } => current_stroke.is_some(),
			Tool::Select { origin } => origin.is_some(),
			Tool::Move { origin } => origin.is_some(),
			Tool::Rotate { origin } => origin.is_some(),
			Tool::Resize { origin } => origin.is_some(),
			_ => false,
		}
	}

	pub fn discard_draft(&mut self) {
		match self.get_mut() {
			Tool::Draw { current_stroke } => *current_stroke = None,
			Tool::Select { origin } => *origin = None,
			Tool::Move { origin } => *origin = None,
			Tool::Rotate { origin } => *origin = None,
			Tool::Resize { origin } => *origin = None,
			_ => {},
		}
		self.invalidate_base_transformation_draft();
	}

	pub fn current_stroke(&self) -> Option<&IncompleteStroke> {
		if let Tool::Draw { current_stroke } = &self.base_mode {
			current_stroke.as_ref()
		} else {
			None
		}
	}
}
