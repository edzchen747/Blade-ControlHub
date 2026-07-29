//! Tests for `CycleState<T>` generic cyclic iterator.

use crate::core::shared_state::SHIFT_PRESSED;
use crate::razer::config::CycleState;
use std::sync::atomic::Ordering;

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
    cs.set(&20).unwrap();
    assert_eq!(cs.index, 1);
    assert_eq!(cs.value(), 20);
}

#[test]
fn cycle_state_set_to_first_element() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&10).unwrap();
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_set_to_last_element() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    cs.set(&30).unwrap();
    assert_eq!(cs.index, 2);
}

#[test]
fn cycle_state_set_returns_err_on_value_not_in_list() {
    let mut cs = CycleState::new(vec![10, 20, 30]);
    let result = cs.set(&99);
    assert!(result.is_err(), "expected Err for value not in list");
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
    cs.set(&"gamma".to_string()).unwrap();
    assert_eq!(cs.index, 2);
    assert_eq!(cs.value(), "gamma");
}

#[test]
fn cycle_state_string_set_returns_err_on_missing_value() {
    let mut cs = CycleState::new(vec!["alpha".to_string(), "beta".to_string()]);
    let result = cs.set(&"not_found".to_string());
    assert!(result.is_err(), "expected Err for missing string value");
}

// ── set() Result API ──────────────────────────────────────────────────────────

#[test]
fn cycle_state_set_success_returns_ok() {
    let mut cs = CycleState::new(vec![10u8, 20, 30]);
    assert!(cs.set(&20).is_ok());
    assert_eq!(cs.index, 1);
}

#[test]
fn cycle_state_set_to_current_value_is_ok_and_idempotent() {
    let mut cs = CycleState::new(vec![10u8, 20, 30]);
    assert!(cs.set(&10).is_ok());
    assert_eq!(cs.index, 0);
    assert!(cs.set(&10).is_ok());
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_set_err_does_not_change_index() {
    let mut cs = CycleState::new(vec![10u8, 20, 30]);
    cs.set(&20).unwrap(); // move to index 1
    let _ = cs.set(&99); // should fail
    assert_eq!(cs.index, 1, "index must not change on Err");
}

// ── next() boundary cases ─────────────────────────────────────────────────────

#[test]
fn cycle_state_single_item_next_stays_at_index_zero() {
    let mut cs = CycleState::new(vec![42u8]);
    assert_eq!(cs.next(), 42);
    assert_eq!(cs.index, 0);
    assert_eq!(cs.next(), 42);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_empty_next_returns_default_without_panicking() {
    let mut cs: CycleState<u8> = CycleState::new(vec![]);

    assert_eq!(cs.next(), 0);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_empty_value_returns_default_without_panicking() {
    let mut cs: CycleState<String> = CycleState::new(vec![]);

    assert_eq!(cs.value(), String::default());
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_value_clamps_out_of_range_index() {
    let mut cs = CycleState {
        index: 99,
        items: vec![10u8, 20, 30],
    };

    assert_eq!(cs.value(), 10);
    assert_eq!(cs.index, 0);
}

#[test]
fn cycle_state_next_clamps_out_of_range_index_before_advancing() {
    let mut cs = CycleState {
        index: 99,
        items: vec![10u8, 20, 30],
    };

    assert_eq!(cs.next(), 20);
    assert_eq!(cs.index, 1);
}

#[test]
fn cycle_state_next_full_cycle_returns_to_start() {
    let items = vec![1u8, 2, 3, 4, 5];
    let len = items.len();
    let mut cs = CycleState::new(items.clone());
    for _ in 0..len {
        cs.next();
    }
    assert_eq!(cs.index, 0, "after a full cycle, index must wrap to 0");
    assert_eq!(cs.value(), items[0]);
}

// ── value() does not advance ──────────────────────────────────────────────────

#[test]
fn cycle_state_value_after_next_reflects_new_item() {
    let mut cs = CycleState::new(vec![100u16, 200, 300]);
    cs.next();
    assert_eq!(cs.value(), 200);
    assert_eq!(cs.value(), 200); // calling value() twice does not advance
}

#[test]
fn cycle_state_remove_deletes_value_and_keeps_valid_index() {
    let mut cs = CycleState::new(vec![10u8, 20, 30]);
    cs.set(&30).unwrap();

    assert!(cs.remove(&20));

    assert_eq!(cs.items, vec![10, 30]);
    assert_eq!(cs.value(), 30);
}

#[test]
fn cycle_state_remove_for_cycle_retry_tries_next_forward_item() {
    SHIFT_PRESSED.store(false, Ordering::SeqCst);
    let mut cs = CycleState::new(vec![10u8, 20, 30]);

    assert_eq!(cs.next(), 20);
    assert!(cs.remove_for_cycle_retry(&20));

    assert_eq!(cs.next(), 30);
}

#[test]
fn cycle_state_remove_for_cycle_retry_tries_next_reverse_item() {
    SHIFT_PRESSED.store(true, Ordering::SeqCst);
    let mut cs = CycleState::new(vec![10u8, 20, 30]);

    assert_eq!(cs.next(), 30);
    assert!(cs.remove_for_cycle_retry(&30));
    assert_eq!(cs.next(), 20);

    SHIFT_PRESSED.store(false, Ordering::SeqCst);
}
