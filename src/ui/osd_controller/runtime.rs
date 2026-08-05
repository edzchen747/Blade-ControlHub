fn osd_thread() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    OSD_THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn join_osd_thread() {
    let current_thread_id = thread::current().id();
    let Some(handle) = osd_thread().take() else {
        return;
    };

    if handle.thread().id() == current_thread_id {
        warn!("Skipping join of current OSD window thread during shutdown");
        *osd_thread() = Some(handle);
        return;
    }

    if handle.join().is_err() {
        warn!("OSD window thread panicked during shutdown");
    }
}

fn post_osd_update(hwnd: HWND, params: OsdParams) {
    unsafe {
        let boxed_params = Box::new(params);
        let lparam = LPARAM(Box::into_raw(boxed_params) as isize);

        if PostMessageW(hwnd, WM_TRIGGER_OSD, WPARAM(0), lparam).is_err() {
            // Prevent memory leaks if the target window receiver pipeline goes offline.
            let _ = Box::from_raw(lparam.0 as *mut OsdParams);
        }
    }
}

fn post_osd_stop(hwnd: HWND) {
    let _ = unsafe { PostMessageW(hwnd, WM_STOP_OSD, WPARAM(0), LPARAM(0)) };
}

fn run_osd_window_thread(tx: Sender<Option<SendableHwnd>>) {
    let _ = get_svg_options();

    let hwnd = match create_controller_window() {
        Some(hwnd) => hwnd,
        None => {
            let _ = tx.send(None);
            return;
        }
    };

    let state = Box::new(OsdStackState::default());
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    }

    OSD_RUNNING.store(true, Ordering::SeqCst);
    let _ = tx.send(Some(SendableHwnd(hwnd)));
    run_osd_message_loop();
    OSD_RUNNING.store(false, Ordering::SeqCst);
}

fn create_controller_window() -> Option<HWND> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let class_name = to_wstring("RustOSD_SVG");
        let title = to_wstring("Rust OSD");
        let instance: HINSTANCE = match GetModuleHandleW(None) {
            Ok(handle) => handle.into(),
            Err(error) => {
                warn!(?error, "Failed to acquire module handle for OSD window");
                return None;
            }
        };

        let wc = WNDCLASSW {
            lpfnWndProc: Some(OsdController::window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            BASE_SIZE as i32,
            BASE_SIZE as i32,
            None,
            None,
            instance,
            None,
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(error) => {
                warn!(?error, "Failed to create OSD controller window");
                None
            }
        }
    }
}

fn create_card_window() -> Option<HWND> {
    unsafe {
        let class_name = to_wstring("RustOSD_SVG");
        let title = to_wstring("Rust OSD");
        let instance: HINSTANCE = match GetModuleHandleW(None) {
            Ok(handle) => handle.into(),
            Err(error) => {
                warn!(?error, "Failed to acquire module handle for OSD card");
                return None;
            }
        };

        match CreateWindowExW(
            OSD_EX_STYLE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            BASE_SIZE as i32,
            BASE_SIZE as i32,
            None,
            None,
            instance,
            None,
        ) {
            Ok(hwnd) => Some(hwnd),
            Err(error) => {
                warn!(?error, "Failed to create OSD card window");
                None
            }
        }
    }
}

fn run_osd_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn to_wstring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn with_osd_stack<R>(action: impl FnOnce(&mut OsdStackState) -> R) -> Option<R> {
    let controller = OSD_INSTANCE.get().and_then(|instance| instance.as_ref())?;
    let ptr = unsafe { GetWindowLongPtrW(controller.hwnd, GWLP_USERDATA) as *mut OsdStackState };
    if ptr.is_null() {
        return None;
    }

    // SAFETY: The pointer is installed from a Box in `run_osd_window_thread`
    // and cleared in `drop_osd_state`. OSD stack state is only accessed from
    // this window procedure on the dedicated OSD thread.
    Some(action(unsafe { &mut *ptr }))
}

fn drop_osd_state(hwnd: HWND) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OsdStackState };
    if !ptr.is_null() {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
        }
    }
}

fn compose_alpha(lifecycle_alpha: u8, stack_depth: f32) -> u8 {
    ((lifecycle_alpha as f32) * STACK_DEPTH_ALPHA.powf(stack_depth))
        .round()
        .clamp(0.0, 255.0) as u8
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (t - 1.0).powf(2.0)).sqrt()
}

fn tween_progress(started_at: Instant, now: Instant, duration: Duration) -> f32 {
    (now.duration_since(started_at).as_secs_f32() / duration.as_secs_f32()).min(1.0)
}

fn retarget_stack_depth(card: &mut OsdCard, target_stack_depth: f32, now: Instant) {
    if (card.stack_depth - target_stack_depth).abs() < 0.0001 {
        card.stack_depth = target_stack_depth;
        card.target_stack_depth = target_stack_depth;
        card.stack_depth_started_at = None;
        return;
    }
    card.stack_depth_start = card.stack_depth;
    card.target_stack_depth = target_stack_depth;
    card.stack_depth_started_at = Some(now);
}

fn retarget_promotion_y_offset(card: &mut OsdCard, target: f32, now: Instant) {
    if (card.promotion_y_offset - target).abs() < 0.01 {
        card.promotion_y_offset = target;
        card.promotion_y_offset_target = target;
        card.promotion_y_offset_started_at = None;
        return;
    }
    card.promotion_y_offset_start = card.promotion_y_offset;
    card.promotion_y_offset_target = target;
    card.promotion_y_offset_started_at = Some(now);
}

fn handle_osd_params(hwnd: HWND, stack: &mut OsdStackState, params: OsdParams) {
    let requested_kind = params.identity();

    if let Some(front) = stack.cards.first_mut()
        && front.identity == requested_kind
    {
        let now = Instant::now();
        front.params = params;
        front.dirty = true;
        front.lifecycle = CardLifecycle::Holding;
        front.lifecycle_started_at = Some(now);
        front.lifecycle_alpha = TARGET_ALPHA;
        front.stack_depth = 0.0;
        front.target_stack_depth = 0.0;
        front.stack_depth_start = 0.0;
        front.stack_depth_started_at = None;
        front.stack_depth_transition_duration = CARD_TRANSITION_DURATION;
        front.swap_destination = None;
        front.promotion_y_offset = 0.0;
        front.promotion_y_offset_target = 0.0;
        front.promotion_y_offset_started_at = None;
        update_card_window(front.hwnd, front);
        ensure_animation_timer(hwnd, stack);
        return;
    }

    if let Some(matching_card_index) = stack
        .cards
        .iter()
        .position(|card| card.identity == requested_kind)
    {
        let now = Instant::now();

        let front_final_stack_depth = stack.cards[0].final_stack_depth();
        let rear_final_stack_depth = stack.cards[1].final_stack_depth();
        let midpoint_stack_depth = (front_final_stack_depth + rear_final_stack_depth) / 2.0;

        if matching_card_index == 1 {
            let receding_front_card = &mut stack.cards[0];
            receding_front_card.lifecycle = CardLifecycle::Swapping;
            receding_front_card.lifecycle_started_at = Some(now);
            receding_front_card.is_promoted_during_swap = false;
            receding_front_card.lifecycle_alpha = TARGET_ALPHA;
            receding_front_card.stack_depth_transition_duration = FRONT_CARD_MIDPOINT_DURATION;
            retarget_stack_depth(receding_front_card, midpoint_stack_depth, now);
            receding_front_card.swap_destination = Some(SwapDestination {
                hidden_stack_depth: midpoint_stack_depth,
                final_stack_depth: rear_final_stack_depth,
            });
        } else {
            for card in stack.cards.iter_mut().take(matching_card_index) {
                if let Some(destination) = card.swap_destination.as_mut() {
                    destination.final_stack_depth += 1.0;
                } else {
                    retarget_stack_depth(card, card.target_stack_depth + 1.0, now);
                }
            }
        }

        let mut promoted_card = stack.cards.remove(matching_card_index);
        promoted_card.params = params;
        promoted_card.dirty = true;
        promoted_card.lifecycle = CardLifecycle::Swapping;
        promoted_card.lifecycle_started_at = Some(now);
        promoted_card.is_promoted_during_swap = true;
        promoted_card.fade_out_start_alpha = promoted_card.lifecycle_alpha;
        promoted_card.swap_destination = Some(SwapDestination {
            hidden_stack_depth: 0.0,
            final_stack_depth: front_final_stack_depth,
        });
        promoted_card.promotion_y_offset = PROMOTED_CARD_Y_OFFSET;
        promoted_card.promotion_y_offset_target = PROMOTED_CARD_Y_OFFSET;
        promoted_card.promotion_y_offset_start = PROMOTED_CARD_Y_OFFSET;
        promoted_card.promotion_y_offset_started_at = None;
        stack.cards.insert(0, promoted_card);

        let front = &mut stack.cards[0];
        update_card_window(front.hwnd, front);
        show_card_window(front.hwnd);
        ensure_animation_timer(hwnd, stack);
        return;
    }

    let now = Instant::now();
    for card in stack.cards.iter_mut() {
        if let Some(destination) = card.swap_destination.as_mut() {
            destination.final_stack_depth += 1.0;
        } else {
            retarget_stack_depth(card, card.target_stack_depth + 1.0, now);
        }
    }

    let Some(card) = create_front_card(params) else {
        warn!("Failed to create OSD card window; dropping OSD update");
        return;
    };
    stack.cards.insert(0, card);
    let front = &mut stack.cards[0];
    update_card_window(front.hwnd, front);
    show_card_window(front.hwnd);
    ensure_animation_timer(hwnd, stack);
}

fn create_front_card(params: OsdParams) -> Option<OsdCard> {
    let hwnd = create_card_window()?;
    Some(OsdCard {
        hwnd,
        identity: params.identity(),
        params,
        lifecycle: CardLifecycle::FadingIn,
        lifecycle_started_at: Some(Instant::now()),
        lifecycle_alpha: 0,
        composited_alpha: 0,
        stack_depth: 0.0,
        target_stack_depth: 0.0,
        stack_depth_start: 0.0,
        stack_depth_started_at: None,
        stack_depth_transition_duration: CARD_TRANSITION_DURATION,
        fade_out_start_alpha: 0,
        swap_destination: None,
        is_promoted_during_swap: false,
        promotion_y_offset: 0.0,
        promotion_y_offset_target: 0.0,
        promotion_y_offset_start: 0.0,
        promotion_y_offset_started_at: None,
        dirty: true,
        last_composited_alpha: 0,
        last_stack_depth: 0.0,
        render: None,
        layers: None,
    })
}

fn show_card_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if let Err(error) = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) {
            warn!(?error, "Failed to bring OSD card to front");
        }
    }
}

fn ensure_animation_timer(hwnd: HWND, stack: &OsdStackState) {
    if stack.cards.is_empty() {
        let _ = unsafe { KillTimer(hwnd, ANIMATION_TIMER) };
    } else {
        let _ = unsafe { SetTimer(hwnd, ANIMATION_TIMER, 10, None) };
    }
}

fn tick_osd_stack(hwnd: HWND, stack: &mut OsdStackState) {
    if stack.cards.is_empty() {
        let _ = unsafe { KillTimer(hwnd, ANIMATION_TIMER) };
        return;
    }

    let now = Instant::now();
    let mut finished: Vec<usize> = Vec::new();

    for (index, card) in stack.cards.iter_mut().enumerate() {
        if let Some(started) = card.stack_depth_started_at {
            let progress = tween_progress(started, now, card.stack_depth_transition_duration);
            card.stack_depth = card.stack_depth_start
                + (card.target_stack_depth - card.stack_depth_start) * ease_out(progress);
            if progress >= 1.0 {
                card.stack_depth = card.target_stack_depth;
                card.stack_depth_started_at = None;
            }
        }

        if let Some(started) = card.promotion_y_offset_started_at {
            let progress = tween_progress(started, now, CARD_TRANSITION_DURATION);
            card.promotion_y_offset = card.promotion_y_offset_start
                + (card.promotion_y_offset_target - card.promotion_y_offset_start)
                    * ease_out(progress);
            if progress >= 1.0 {
                card.promotion_y_offset = card.promotion_y_offset_target;
                card.promotion_y_offset_started_at = None;
            }
        }

        let Some(started_at) = card.lifecycle_started_at else {
            continue;
        };
        let elapsed = now.duration_since(started_at);

        match card.lifecycle {
            CardLifecycle::FadingIn => {
                let fade_in_duration = card.fade_in_duration();

                if elapsed >= fade_in_duration {
                    card.lifecycle = CardLifecycle::Holding;
                    card.lifecycle_started_at = Some(now);
                    card.lifecycle_alpha = TARGET_ALPHA;
                } else {
                    card.lifecycle_alpha =
                        (card.fade_in_progress(elapsed) * TARGET_ALPHA as f32) as u8;
                }
            }
            CardLifecycle::Holding => {
                if elapsed >= HOLD_DURATION {
                    card.lifecycle = CardLifecycle::FadingOut;
                    card.lifecycle_started_at = Some(now);
                    card.fade_out_start_alpha = card.lifecycle_alpha;
                }
            }
            CardLifecycle::FadingOut => {
                if elapsed >= FADE_OUT_DURATION {
                    card.lifecycle = CardLifecycle::Expired;
                    card.lifecycle_started_at = None;
                    card.lifecycle_alpha = 0;
                } else {
                    let progress = elapsed.as_secs_f32() / FADE_OUT_DURATION.as_secs_f32();
                    card.lifecycle_alpha =
                        ((1.0 - progress) * card.fade_out_start_alpha as f32) as u8;
                }
            }
            CardLifecycle::Swapping => {
                if elapsed >= SWAP_FADE_OUT_DURATION + SWAP_SHOW_NEW_DELAY {
                    if let Some(destination) = card.swap_destination.take() {
                        card.stack_depth = destination.hidden_stack_depth;
                        card.stack_depth_transition_duration = CARD_TRANSITION_DURATION;
                        retarget_stack_depth(card, destination.final_stack_depth, now);
                    }
                    if card.is_promoted_during_swap {
                        card.lifecycle = CardLifecycle::FadingIn;
                        card.lifecycle_started_at = Some(now);
                        card.lifecycle_alpha = 0;
                        retarget_promotion_y_offset(card, 0.0, now);
                    } else {
                        card.lifecycle = CardLifecycle::Holding;
                        card.lifecycle_started_at = Some(now);
                        card.lifecycle_alpha = RECEDING_CARD_ALPHA;
                    }
                } else if elapsed >= SWAP_FADE_OUT_DURATION {
                    if card.is_promoted_during_swap {
                        card.lifecycle_alpha = 0;
                    } else {
                        card.lifecycle_alpha = RECEDING_CARD_ALPHA;
                    }
                } else if card.is_promoted_during_swap {
                    let progress = elapsed.as_secs_f32() / SWAP_FADE_OUT_DURATION.as_secs_f32();
                    card.lifecycle_alpha =
                        ((1.0 - progress) * card.fade_out_start_alpha as f32) as u8;
                } else {
                    let progress = elapsed.as_secs_f32() / SWAP_FADE_OUT_DURATION.as_secs_f32();
                    card.lifecycle_alpha = (TARGET_ALPHA as f32
                        - (TARGET_ALPHA as f32 - RECEDING_CARD_ALPHA as f32) * progress)
                        as u8;
                }
            }
            CardLifecycle::Expired => {}
        }

        card.composited_alpha = compose_alpha(card.lifecycle_alpha, card.stack_depth);

        if card.lifecycle == CardLifecycle::Expired {
            finished.push(index);
        } else {
            update_card_window(card.hwnd, card);
        }
    }

    for index in finished.into_iter().rev() {
        let card = stack.cards.remove(index);
        let _ = unsafe { DestroyWindow(card.hwnd) };
    }

    if stack.cards.is_empty() {
        let _ = unsafe { KillTimer(hwnd, ANIMATION_TIMER) };
    }
}
