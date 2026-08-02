fn server_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    IPC_SERVER_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn razer_key_events() -> MutexGuard<'static, RazerKeyEventLog> {
    RAZER_KEY_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct RazerKeyEventLog {
    next_sequence: u64,
    events: VecDeque<RazerKeyEvent>,
}

impl RazerKeyEventLog {
    const fn new() -> Self {
        Self {
            next_sequence: 0,
            events: VecDeque::new(),
        }
    }

    fn push(&mut self, key_code: u8) -> RazerKeyEvent {
        self.push_at(key_code, current_unix_ms())
    }

    fn push_at(&mut self, key_code: u8, unix_ms: u64) -> RazerKeyEvent {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let event = RazerKeyEvent {
            sequence: self.next_sequence,
            unix_ms,
            key_code,
        };
        self.events.push_back(event);
        while self.events.len() > MAX_RAZER_KEY_EVENTS {
            self.events.pop_front();
        }
        event
    }

    fn latest_sequence_before(&self, unix_ms: u64) -> u64 {
        self.events
            .iter()
            .rev()
            .find(|event| event.unix_ms < unix_ms)
            .map(|event| event.sequence)
            .unwrap_or(0)
    }

    fn first_after(&self, sequence: u64) -> Option<RazerKeyEvent> {
        self.events
            .iter()
            .copied()
            .find(|event| event.sequence > sequence)
    }

    fn clear(&mut self) {
        self.next_sequence = 0;
        self.events.clear();
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

