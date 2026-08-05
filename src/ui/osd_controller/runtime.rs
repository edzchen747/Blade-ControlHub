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

// --- Shared Utility Implementations ---

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

/// Hidden owner window that receives triggers/timers and holds the stack state.
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

/// Visible layered window for one stacked card. Never shown directly; it is
/// positioned and painted exclusively through `UpdateLayeredWindow`.
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

// --- Card Stack Logic ---

/// Blends the lifecycle envelope with the depth fade. Recomputing from the
/// envelope (instead of multiplying the previous tick's alpha) keeps the
/// background fade stable and lets fade-out start exactly where the card is.
fn depth_blend(envelope: u8, depth: f32) -> u8 {
    ((envelope as f32) * DEPTH_ALPHA.powf(depth))
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Ease-out curve for sliding animations: start at full speed and decelerate
/// into a slow, soft landing so cards look like they arrive and click into
/// place (easeOutQuart).
fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (t - 1.0).powf(2.0)).sqrt()
}

fn tween_progress(started_at: Instant, now: Instant, duration: Duration) -> f32 {
    (now.duration_since(started_at).as_secs_f32() / duration.as_secs_f32()).min(1.0)
}

/// Starts an eased depth transition toward `target`, continuing from the
/// card's current animated depth. No-op when already at the target.
fn retarget_depth(card: &mut OsdCard, target_depth: f32, now: Instant) {
    if (card.depth - target_depth).abs() < 0.0001 {
        card.depth = target_depth;
        card.target_depth = target_depth;
        card.depth_started_at = None;
        return;
    }
    card.depth_from = card.depth;
    card.target_depth = target_depth;
    card.depth_started_at = Some(now);
}

/// Starts the eased slide-up of the new front card toward `target`.
fn retarget_slide_up(card: &mut OsdCard, target: f32, now: Instant) {
    if (card.slide_up - target).abs() < 0.01 {
        card.slide_up = target;
        card.slide_up_target = target;
        card.slide_up_started_at = None;
        return;
    }
    card.slide_up_from = card.slide_up;
    card.slide_up_target = target;
    card.slide_up_started_at = Some(now);
}

fn handle_osd_params(hwnd: HWND, stack: &mut OsdStackState, params: OsdParams) {
    let kind = params.kind();

    // Front card already shows this control: replace it in place and restart
    // the hold (rapid same-control updates never animate).
    if let Some(front) = stack.cards.first_mut()
        && front.kind == kind
    {
        let now = Instant::now();
        front.params = params;
        front.dirty = true;
        front.animation = AnimState::Hold;
        front.animation_started_at = Some(now);
        front.envelope = TARGET_ALPHA;
        front.depth = 0.0;
        front.target_depth = 0.0;
        front.depth_from = 0.0;
        front.depth_started_at = None;
        front.depth_tween_duration = DEPTH_EASE_DURATION;
        front.swap_to = None;
        front.slide_up = 0.0;
        front.slide_up_target = 0.0;
        front.slide_up_started_at = None;
        update_card_window(front.hwnd, front);
        ensure_animation_timer(hwnd, stack);
        return;
    }

    // A buried card of the same control is being triggered again.
    if let Some(index) = stack.cards.iter().position(|card| card.kind == kind) {
        let now = Instant::now();

        // Resolve the pair's settle depths before any swap, so rapid triggers
        // never compute swap targets from mid-flight animated depths.
        let front_settle = stack.cards[0].settle_depth();
        let rear_settle = stack.cards[1].settle_depth();
        let midpoint = (front_settle + rear_settle) / 2.0;

        if index == 1 {
            // Front pair, stage 1: the rear card fades out while the front
            // card stays visible and slides to the halfway line. Stage 2:
            // depths swap while the rear is invisible, then the rear card
            // fades back in at the front while the front card continues
            // sliding to the rear. One moving card per phase, so only one
            // card re-renders at a time. Cards behind the pair stay
            // untouched (no flash, same timeout).
            let swapped = &mut stack.cards[0];
            swapped.animation = AnimState::SwapOut;
            swapped.animation_started_at = Some(now);
            swapped.swap_fade = false;
            swapped.envelope = TARGET_ALPHA;
            // The stage-1 slide must land on the midpoint exactly when the
            // depths swap, so it runs on the swap's own timeline.
            swapped.depth_tween_duration = DEPTH_SWAP_SLIDE_DURATION;
            retarget_depth(swapped, midpoint, now);
            swapped.swap_to = Some((midpoint, rear_settle));
        } else {
            // Several cards stand in front: they slide one level back (eased)
            // while the triggered card appears instantly at the front.
            for card in stack.cards.iter_mut().take(index) {
                if let Some((_, settle)) = card.swap_to.as_mut() {
                    *settle += 1.0;
                } else {
                    retarget_depth(card, card.target_depth + 1.0, now);
                }
            }
        }

        let mut triggered = stack.cards.remove(index);
        triggered.params = params;
        triggered.dirty = true;
        triggered.animation = AnimState::SwapOut;
        triggered.animation_started_at = Some(now);
        triggered.swap_fade = true;
        triggered.fade_out_from = triggered.envelope;
        // Land directly on the front position while invisible; it fades back
        // in there (rising into place) while the old front card slides to the
        // rear.
        triggered.swap_to = Some((0.0, front_settle));
        triggered.slide_up = SWAP_IN_SLIDE_PX;
        triggered.slide_up_target = SWAP_IN_SLIDE_PX;
        triggered.slide_up_from = SWAP_IN_SLIDE_PX;
        triggered.slide_up_started_at = None;
        stack.cards.insert(0, triggered);

        let front = &mut stack.cards[0];
        update_card_window(front.hwnd, front);
        show_card_window(front.hwnd);
        ensure_animation_timer(hwnd, stack);
        return;
    }

    // A new control: push every existing card one level back and create a
    // fresh front card. Depth is unbounded, so the stack can grow freely.
    let now = Instant::now();
    for card in stack.cards.iter_mut() {
        if let Some((_, settle)) = card.swap_to.as_mut() {
            *settle += 1.0;
        } else {
            retarget_depth(card, card.target_depth + 1.0, now);
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
        kind: params.kind(),
        params,
        animation: AnimState::FadeIn,
        animation_started_at: Some(Instant::now()),
        envelope: 0,
        alpha: 0,
        depth: 0.0,
        target_depth: 0.0,
        depth_from: 0.0,
        depth_started_at: None,
        depth_tween_duration: DEPTH_EASE_DURATION,
        fade_out_from: 0,
        swap_to: None,
        swap_fade: false,
        slide_up: 0.0,
        slide_up_target: 0.0,
        slide_up_from: 0.0,
        slide_up_started_at: None,
        dirty: true,
        last_alpha: 0,
        last_depth: 0.0,
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
        // Depth transition: duration-based ease-out (fast start, slow
        // landing). Re-targets continue from the current value.
        if let Some(started) = card.depth_started_at {
            let progress = tween_progress(started, now, card.depth_tween_duration);
            card.depth =
                card.depth_from + (card.target_depth - card.depth_from) * ease_out(progress);
            if progress >= 1.0 {
                card.depth = card.target_depth;
                card.depth_started_at = None;
            }
        }

        // Vertical slide of the new front card rising into place. Runs on the
        // ease-out curve: fast start, smooth landing.
        if let Some(started) = card.slide_up_started_at {
            let progress = tween_progress(started, now, DEPTH_EASE_DURATION);
            card.slide_up = card.slide_up_from
                + (card.slide_up_target - card.slide_up_from) * ease_out(progress);
            if progress >= 1.0 {
                card.slide_up = card.slide_up_target;
                card.slide_up_started_at = None;
            }
        }

        let Some(started_at) = card.animation_started_at else {
            continue;
        };
        let elapsed = now.duration_since(started_at);

        match card.animation {
            AnimState::FadeIn => {
                if elapsed >= FADE_IN_DURATION {
                    card.animation = AnimState::Hold;
                    card.animation_started_at = Some(now);
                    card.envelope = TARGET_ALPHA;
                } else {
                    let progress = elapsed.as_secs_f32() / FADE_IN_DURATION.as_secs_f32();
                    // The swap's foreground card fades in with the soft slide
                    // curve (fast start, gentle landing); brand-new cards
                    // stay linear.
                    let curve = if card.swap_fade {
                        ease_out(progress)
                    } else {
                        progress
                    };
                    card.envelope = (curve * TARGET_ALPHA as f32) as u8;
                }
            }
            AnimState::Hold => {
                if elapsed >= HOLD_DURATION {
                    card.animation = AnimState::FadeOut;
                    card.animation_started_at = Some(now);
                    card.fade_out_from = card.envelope;
                }
            }
            AnimState::FadeOut => {
                if elapsed >= FADE_OUT_DURATION {
                    card.animation = AnimState::Idle;
                    card.animation_started_at = None;
                    card.envelope = 0;
                } else {
                    let progress = elapsed.as_secs_f32() / FADE_OUT_DURATION.as_secs_f32();
                    card.envelope = ((1.0 - progress) * card.fade_out_from as f32) as u8;
                }
            }
            AnimState::SwapOut => {
                if elapsed >= SWAP_FADE_OUT_DURATION + SWAP_IN_DELAY {
                    // Land on the swapped depth while invisible, then ease to
                    // it during the next stage.
                    if let Some((landing, settle)) = card.swap_to.take() {
                        card.depth = landing;
                        // Stage-2 slides run at the normal duration again.
                        card.depth_tween_duration = DEPTH_EASE_DURATION;
                        retarget_depth(card, settle, now);
                    }
                    if card.swap_fade {
                        // Rear card: invisible after the fade-out, so it fades
                        // back in at its swapped (front) position, rising into
                        // place.
                        card.animation = AnimState::FadeIn;
                        card.animation_started_at = Some(now);
                        card.envelope = 0;
                        retarget_slide_up(card, 0.0, now);
                    } else {
                        // Front card: never faded — it just continues its hold
                        // while sliding to the rear, staying dimmed.
                        card.animation = AnimState::Hold;
                        card.animation_started_at = Some(now);
                        card.envelope = SWAP_BACK_ALPHA;
                    }
                } else if elapsed >= SWAP_FADE_OUT_DURATION {
                    // Fade-out finished; hold the invisible beat before the
                    // new front card fades in.
                    if card.swap_fade {
                        card.envelope = 0;
                    } else {
                        card.envelope = SWAP_BACK_ALPHA;
                    }
                } else if card.swap_fade {
                    let progress = elapsed.as_secs_f32() / SWAP_FADE_OUT_DURATION.as_secs_f32();
                    card.envelope = ((1.0 - progress) * card.fade_out_from as f32) as u8;
                } else {
                    // The sliding front card dims as soon as it starts moving
                    // back. Swap-only: normal push-backs dim via depth alone.
                    let progress = elapsed.as_secs_f32() / SWAP_FADE_OUT_DURATION.as_secs_f32();
                    card.envelope = (TARGET_ALPHA as f32
                        - (TARGET_ALPHA as f32 - SWAP_BACK_ALPHA as f32) * progress)
                        as u8;
                }
            }
            AnimState::Idle => {}
        }

        // Background cards are dimmer, but the fade is stable: it always stems
        // from the lifecycle envelope, never from the previous tick's alpha.
        card.alpha = depth_blend(card.envelope, card.depth);

        if card.animation == AnimState::Idle {
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
