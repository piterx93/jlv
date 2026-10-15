use proptest::prelude::*;

use super::*;

fn key(code: KeyCode) -> Key {
    Key {
        code,
        modifiers: KeyModifiers::NONE,
    }
}

fn ctrl(code: KeyCode) -> Key {
    Key {
        code,
        modifiers: KeyModifiers::CONTROL,
    }
}

fn binding_key(binding: &Binding) -> Key {
    Key {
        code: binding.code,
        modifiers: binding.modifiers,
    }
}

fn remapped(remaps: &[(Action, Vec<Key>)]) -> Keymap {
    Keymap::remapped(remaps).unwrap_or_else(|err| panic!("{remaps:?}: {err}"))
}

// --- key syntax ---

#[test]
fn parses_every_key_of_the_syntax() {
    let cases = [
        ("x", key(KeyCode::Char('x'))),
        ("F", key(KeyCode::Char('F'))),
        ("?", key(KeyCode::Char('?'))),
        ("F1", key(KeyCode::F(1))),
        ("F12", key(KeyCode::F(12))),
        ("Ctrl-x", ctrl(KeyCode::Char('x'))),
        ("Ctrl-Up", ctrl(KeyCode::Up)),
        ("Enter", key(KeyCode::Enter)),
        ("Esc", key(KeyCode::Esc)),
        ("Space", key(KeyCode::Char(' '))),
        ("Tab", key(KeyCode::Tab)),
        ("Up", key(KeyCode::Up)),
        ("Down", key(KeyCode::Down)),
        ("Left", key(KeyCode::Left)),
        ("Right", key(KeyCode::Right)),
        ("PgUp", key(KeyCode::PageUp)),
        ("PgDn", key(KeyCode::PageDown)),
        ("Home", key(KeyCode::Home)),
        ("End", key(KeyCode::End)),
    ];
    for (text, expected) in cases {
        assert_eq!(text.parse::<Key>(), Ok(expected), "{text:?}");
    }
}

/// `Ctrl-X` is rejected because terminals report Ctrl with a letter as the lower-case
/// letter, so a binding to the upper-case one would never fire.
#[test]
fn rejects_text_outside_the_syntax_as_unknown_key() {
    for text in [
        "", "xy", "F0", "F13", "enter", "Ctrl-", "Ctrl-xy", "ctrl-x", "Ctrl-X", "Alt-x", "\t",
    ] {
        let err = text.parse::<Key>().expect_err(text);
        assert!(err.contains("unknown key"), "{text:?}: {err}");
    }
}

proptest! {
    #[test]
    fn key_parser_never_panics(text in prop_oneof![
        any::<String>(),
        "(Ctrl-)?(F[0-9]{0,3}|[A-Za-z]{0,6}|.)",
    ]) {
        let _ = text.parse::<Key>();
    }
}

// --- action names ---

#[test]
fn actions_are_named_in_snake_case() {
    let names = [
        (Action::Up, "up"),
        (Action::Down, "down"),
        (Action::PageUp, "page_up"),
        (Action::PageDown, "page_down"),
        (Action::HalfPageUp, "half_page_up"),
        (Action::HalfPageDown, "half_page_down"),
        (Action::Top, "top"),
        (Action::Bottom, "bottom"),
        (Action::ScrollLeft, "scroll_left"),
        (Action::ScrollRight, "scroll_right"),
        (Action::CycleFocus, "cycle_focus"),
        (Action::Confirm, "confirm"),
        (Action::Cancel, "cancel"),
        (Action::ToggleFollow, "toggle_follow"),
        (Action::CyclePreset, "cycle_preset"),
        (Action::SearchForward, "search_forward"),
        (Action::SearchBackward, "search_backward"),
        (Action::NextMatch, "next_match"),
        (Action::PreviousMatch, "previous_match"),
        (Action::LinePrompt, "line_prompt"),
        (Action::TimePrompt, "time_prompt"),
        (Action::ToggleLevel(Level::Trace), "toggle_level_trace"),
        (Action::ToggleLevel(Level::Debug), "toggle_level_debug"),
        (Action::ToggleLevel(Level::Info), "toggle_level_info"),
        (Action::ToggleLevel(Level::Warn), "toggle_level_warn"),
        (Action::ToggleLevel(Level::Error), "toggle_level_error"),
        (Action::ToggleLevel(Level::Fatal), "toggle_level_fatal"),
        (Action::LevelPanel, "level_panel"),
        (Action::TextPanel, "text_panel"),
        (Action::TimePanel, "time_panel"),
        (Action::ClassPanel, "class_panel"),
        (Action::OnlyClass, "only_class"),
        (Action::HideClass, "hide_class"),
        (Action::ClearFilters, "clear_filters"),
        (Action::ToggleFold, "toggle_fold"),
        (Action::FoldAll, "fold_all"),
        (Action::UnfoldAll, "unfold_all"),
        (Action::NextException, "next_exception"),
        (Action::PreviousException, "previous_exception"),
        (Action::ToggleBookmark, "toggle_bookmark"),
        (Action::NextBookmark, "next_bookmark"),
        (Action::PreviousBookmark, "previous_bookmark"),
        (Action::Copy, "copy"),
        (Action::Export, "export"),
        (Action::ToggleThread, "toggle_thread"),
        (Action::ToggleLineNumbers, "toggle_line_numbers"),
        (Action::ToggleHistogram, "toggle_histogram"),
        (Action::ToggleWrap, "toggle_wrap"),
        (Action::Help, "help"),
        (Action::Quit, "quit"),
    ];
    for (action, expected) in names {
        assert_eq!(name(action), expected);
        assert_eq!(named(expected), Some(action), "{expected}");
    }
    for binding in Keymap::default().bindings() {
        assert!(
            names.iter().any(|&(action, _)| action == binding.action),
            "{binding:?} has no name in this table"
        );
    }
}

// --- remapping ---

/// Every action of the default keymap, once.
fn default_actions() -> Vec<Action> {
    let mut actions = Vec::new();
    for binding in Keymap::default().bindings() {
        if !actions.contains(&binding.action) {
            actions.push(binding.action);
        }
    }
    actions
}

/// A few keys, most of them bound by default, so that remaps often collide.
const KEY_POOL: [Key; 10] = [
    Key {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Char('F'),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Char('v'),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::F(10),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::F(11),
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Up,
        modifiers: KeyModifiers::NONE,
    },
    Key {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
    },
    Key {
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::CONTROL,
    },
];

/// Up to four distinct actions, each with one to three keys from [`KEY_POOL`].
fn any_remaps() -> impl Strategy<Value = Vec<(Action, Vec<Key>)>> {
    let actions = default_actions();
    let count = actions.len();
    prop::collection::btree_map(
        0..count,
        prop::collection::vec(prop::sample::select(KEY_POOL.to_vec()), 1..=3),
        0..=4,
    )
    .prop_map(move |remaps| {
        remaps
            .into_iter()
            .map(|(i, keys)| (actions[i], keys))
            .collect()
    })
}

/// What a keymap with `remaps` binds: every default binding of an action `remaps`
/// leaves alone and every key `remaps` gives an action, `None` if that binds a key
/// to two actions.
fn naive_bindings(remaps: &[(Action, Vec<Key>)]) -> Option<Vec<(Key, Action)>> {
    let mut pairs: Vec<(Key, Action)> = Vec::new();
    for binding in Keymap::default().bindings() {
        if !remaps.iter().any(|(action, _)| *action == binding.action) {
            pairs.push((binding_key(binding), binding.action));
        }
    }
    for (action, keys) in remaps {
        for &key in keys {
            if !pairs.contains(&(key, *action)) {
                pairs.push((key, *action));
            }
        }
    }
    let conflict = pairs
        .iter()
        .any(|&(k, a)| pairs.iter().any(|&(other, b)| other == k && b != a));
    (!conflict).then_some(pairs)
}

proptest! {
    #[test]
    fn remapped_keymap_binds_what_the_remaps_say(remaps in any_remaps()) {
        let keymap = Keymap::remapped(&remaps);
        let Some(expected) = naive_bindings(&remaps) else {
            prop_assert!(keymap.is_err(), "{:?}", keymap);
            return Ok(());
        };
        let keymap = keymap.unwrap();
        let bound: Vec<(Key, Action)> = keymap
            .bindings()
            .iter()
            .map(|b| (binding_key(b), b.action))
            .collect();
        prop_assert_eq!(bound.len(), expected.len(), "{:?}", bound);
        for pair in &expected {
            prop_assert!(bound.contains(pair), "{:?} missing from {:?}", pair, bound);
        }
        let defaults = Keymap::default();
        let probes = KEY_POOL.iter().copied().chain(defaults.bindings().iter().map(binding_key));
        for probe in probes {
            let wanted = expected.iter().find(|(k, _)| *k == probe).map(|&(_, a)| a);
            let event = KeyEvent::new(probe.code, probe.modifiers);
            prop_assert_eq!(keymap.action(event), wanted, "{:?}", probe);
        }
    }
}

// --- help ---

/// A remapped action's keys take the place of its first default key, so the overlay
/// keeps its order.
#[test]
fn help_lists_remapped_keys_where_the_default_keys_were() {
    let keymap = remapped(&[
        (
            Action::ToggleFollow,
            vec![key(KeyCode::Char('f')), ctrl(KeyCode::Char('f'))],
        ),
        (
            Action::ToggleLevel(Level::Info),
            vec![key(KeyCode::Char('i'))],
        ),
    ]);
    let mut expected = Keymap::default().help();
    for (keys, description) in &mut expected {
        match *description {
            "Toggle follow" => *keys = "f Ctrl-f".to_string(),
            "Toggle level (TRACE to FATAL)" => *keys = "1 2 i 4 5 6".to_string(),
            _ => {}
        }
    }
    assert_eq!(keymap.help(), expected);
}
