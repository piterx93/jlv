use std::collections::HashSet;

use proptest::prelude::*;
use ratatui::crossterm::event::KeyEvent;

use super::*;
use crate::app::action;

#[test]
fn every_binding_maps_through_action_to_its_action() {
    for binding in bindings() {
        let key = KeyEvent::new(binding.code, binding.modifiers);
        assert_eq!(action(key), Some(binding.action), "{binding:?}");
    }
}

#[test]
fn no_key_is_bound_twice() {
    let mut seen = HashSet::new();
    for binding in bindings() {
        assert!(
            seen.insert((binding.code, binding.modifiers)),
            "{binding:?}"
        );
    }
}

fn any_code() -> impl Strategy<Value = KeyCode> {
    prop_oneof![
        prop::char::range(' ', '~').prop_map(KeyCode::Char),
        any::<char>().prop_map(KeyCode::Char),
        (1u8..=12).prop_map(KeyCode::F),
        prop::sample::select(vec![
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
        ]),
    ]
}

fn any_modifiers() -> impl Strategy<Value = KeyModifiers> {
    prop::sample::select(vec![
        KeyModifiers::NONE,
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ])
}

proptest! {
    #[test]
    fn action_binds_exactly_the_keys_of_the_keymap(code in any_code(), modifiers in any_modifiers()) {
        let bound = bindings()
            .iter()
            .find(|b| b.code == code && b.modifiers == modifiers)
            .map(|b| b.action);
        prop_assert_eq!(action(KeyEvent::new(code, modifiers)), bound);
    }
}

#[test]
fn help_lists_every_binding_once_under_its_description() {
    let help = help();
    for binding in bindings() {
        let name = key_name(binding.code, binding.modifiers);
        let description = description(binding.action);
        assert!(
            help.iter()
                .any(|(keys, d)| *d == description && keys.split(' ').any(|k| k == name)),
            "{name} {description} missing from {help:?}"
        );
    }
    let listed: usize = help.iter().map(|(keys, _)| keys.split(' ').count()).sum();
    assert_eq!(listed, bindings().len());
}

#[test]
fn help_has_one_line_per_description_in_keymap_order() {
    let mut expected = Vec::new();
    for binding in bindings() {
        let description = description(binding.action);
        if !expected.contains(&description) {
            expected.push(description);
        }
    }
    let lines: Vec<&str> = help().iter().map(|&(_, d)| d).collect();
    assert_eq!(lines, expected);
}
