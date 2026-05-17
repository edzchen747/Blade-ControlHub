//! Tests for `CycleState<T>` generic cyclic iterator.

use crate::razer::config::CycleState;

// ── Basic initialization ────────────────────────────────────────────────────

#[test]
fn cycle_state_new_starts_at_index_zero() {
    let cs: CycleState<u8> = CycleState::new(vec![10, 20, 30]);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_value_returns_current_item_without_advancing() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    assert_eq!(cs.value(), 10);
    assert_eq!(cs.value(), 10);
    assert_eq!(cs.index, 0);
}

// ── next() advances and returns new item ─────────────────────────────────────

#[test]
fn cycle_state_next_advances_index_and_returns_new_item() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    assert_eq!(cs.next(), 20);
    assert_eq!(cs.index, 1);
    assert_eq!(cs.next(), 30);
    assert_eq!(cs.index, 2);
}

#[test]
fn cycle_state_next_wraps_around_at_end_of_list() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    assert_eq!(cs.next(), 20);
    assert_eq!(cs.next(), 30);
    // Next call should wrap back to index 0
    assert_eq!(cs.next(), 10);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_two_item_list_cycles_correctly() {
    let mut cs = CycleState::new(vec![100, 200]);
    assert_eq!(cs.next(), 200);
    assert_eq!(cs.next(), 100);
    assert_eq!(cs.next(), 200);
}

// ── set() moves index to correct position ────────────────────────────────────

#[test]
fn cycle_state_set_moves_index_to_matching_value() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&20);
    assert_eq!(cs.index, 1);
    assert_eq!(cs.value(), 20);
}

#[test]
fn cycle_state_set_to_first_element() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&10);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_set_to_last_element() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&30);
    assert_eq!(cs.index, 2);
}

#[test]
#[should_panic(expected = "Internal State Error")]
fn cycle_state_set_panics_on_value_not_in_list() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&99);
}

// ── String type ──────────────────────────────────────────────────────────────

#[test]
fn cycle_state_string_new_starts_at_index_zero() {
    let cs = CycleState::new(vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_string_value_returns_current_without_advancing() {
    let mut cs = CycleState::new(vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(cs.value(), "alpha");
    assert_eq!(cs.value(), "alpha");
}

#[test]
fn cycle_state_string_next_advances_correctly() {
    let mut cs = CycleState::new(vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
    ]);
    assert_eq!(cs.next(), "beta");
    assert_eq!(cs.next(), "gamma");
    assert_eq!(cs.next(), "alpha");
}

#[test]
fn cycle_state_string_set_moves_to_correct_position() {
    let mut cs = CycleState::new(vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
    ]);
    cs.set(&"gamma".to_string());
    assert_eq!(cs.index, 2);
    assert_eq!(cs.value(), "gamma");
}

#[test]
#[should_panic(expected = "Internal State Error")]
fn cycle_state_string_set_panics_on_missing_value() {
    let mut cs = CycleState::new(vec!["alpha".to_string(), "beta".to_string()]);
    cs.set(&"not_found".to_string());
}
