// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use enumset::EnumSet;

use super::Key;
use crate::app::App;

pub enum Action {
	Trigger { on_trigger: fn(&mut App) },
	Discovery { on_press: fn(&mut App), on_release: fn(&mut App) },
}

pub struct Keytest {
	triggers: EnumSet<Key>,
	is_repeatable: bool,
	action: Action,
}

pub struct Keymap {
	keytests: HashMap<EnumSet<Key>, Keytest>,
	waiting_releases: Vec<(EnumSet<Key>, fn(&mut App))>,
}

impl Keymap {
	pub fn new() -> Self {
		Self {
			keytests: HashMap::new(),
			waiting_releases: Vec::new(),
		}
	}

	pub fn insert(&mut self, modifiers: impl Into<EnumSet<Key>>, triggers: impl Into<EnumSet<Key>>, is_repeatable: bool, action: Action) {
		let (modifiers, triggers) = (modifiers.into(), triggers.into());
		self.keytests.insert(modifiers.union(triggers), Keytest { triggers, is_repeatable, action });
	}
}

pub fn execute_keymap(app: &mut App, active_keys: EnumSet<Key>, fresh_keys: EnumSet<Key>, different_keys: EnumSet<Key>) {
	let mut release_indices = vec![];

	for (i, (detriggers, _)) in app.keymap.waiting_releases.iter().enumerate() {
		if !detriggers.is_subset(active_keys) && detriggers.is_subset(active_keys.symmetrical_difference(different_keys)) {
			release_indices.push(i);
		}
	}

	for i in release_indices.iter().rev() {
		app.keymap.waiting_releases[*i].1(app);
		app.keymap.waiting_releases.remove(*i);
	}

	if let Some(keytest) = app.keymap.keytests.get(&active_keys) {
		match keytest.action {
			Action::Trigger { on_trigger } => {
				if !keytest.triggers.intersection(if keytest.is_repeatable { fresh_keys } else { different_keys }).is_empty() {
					on_trigger(app);
				}
			},
			Action::Discovery { on_press, on_release } => {
				if !keytest.triggers.intersection(if keytest.is_repeatable { fresh_keys } else { different_keys }).is_empty() || !active_keys.complement().intersection(different_keys).is_empty() {
					on_press(app);
					app.keymap.waiting_releases.push((active_keys, on_release));
				}
			},
		}
	}
}
