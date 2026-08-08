use crate::core::shared_state::SHIFT_PRESSED;
use crate::error::{AppError, AppResult};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CycleState<T> {
    pub index: usize,
    pub items: Vec<T>,
}

impl<T: Clone + PartialEq + Default> CycleState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { index: 0, items }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> T {
        if self.items.is_empty() {
            self.index = 0;
            return T::default();
        }
        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);
        let current_index = self.normalized_index();
        self.index = if reverse {
            current_index
                .checked_sub(1)
                .unwrap_or_else(|| self.items.len() - 1)
        } else {
            (current_index + 1) % self.items.len()
        };
        self.items[self.index].clone()
    }

    pub fn value(&mut self) -> T {
        if self.items.is_empty() {
            self.index = 0;
            return T::default();
        }
        self.index = self.normalized_index();
        self.items[self.index].clone()
    }

    pub fn set(&mut self, value: &T) -> AppResult<()> {
        match self.items.iter().position(|x| x == value) {
            Some(pos) => {
                self.index = pos;
                Ok(())
            }
            None => Err(AppError::Internal(
                "CycleState::set: value not found in items list".to_string(),
            )),
        }
    }

    pub fn remove(&mut self, value: &T) -> bool {
        let Some(pos) = self.items.iter().position(|x| x == value) else {
            return false;
        };
        self.items.remove(pos);
        if self.items.is_empty() {
            self.index = 0;
        } else if pos < self.index {
            self.index -= 1;
        } else if self.index >= self.items.len() {
            self.index = 0;
        }
        true
    }

    pub fn remove_for_cycle_retry(&mut self, value: &T) -> bool {
        let Some(pos) = self.items.iter().position(|x| x == value) else {
            return false;
        };
        let reverse = SHIFT_PRESSED.load(Ordering::SeqCst);
        self.items.remove(pos);
        if self.items.is_empty() {
            self.index = 0;
            return true;
        }
        let next_candidate_pos = if reverse {
            pos.checked_sub(1).unwrap_or_else(|| self.items.len() - 1)
        } else if pos >= self.items.len() {
            0
        } else {
            pos
        };
        self.index = if reverse {
            (next_candidate_pos + 1) % self.items.len()
        } else {
            next_candidate_pos
                .checked_sub(1)
                .unwrap_or_else(|| self.items.len() - 1)
        };
        true
    }

    fn normalized_index(&self) -> usize {
        if self.index < self.items.len() {
            self.index
        } else {
            0
        }
    }
}
