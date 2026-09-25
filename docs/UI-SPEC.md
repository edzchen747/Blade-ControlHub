# Blade ControlHub — UI Specification

Status: authoritative spec for the Tauri rebuild. Written before any UI code, and
deliberately **not** a description of the egui window it replaces. The egui UI is
referenced only where it defines a hardware contract that must be preserved.

---

## 1. Why a redesign and not a port

The egui UI was shaped by immediate-mode constraints, and those constraints leaked
into the product:

| egui constraint | How it shaped the old UI | What it cost |
|---|---|---|
| Fixed 500×750 non-resizable window | Every control had to fit a hard-coded box | Long lists (refresh rates, perf modes) wrapped into ragged 3-column grids |
| No layout engine | Slider widths computed by measuring label text each frame | Labels and values drift out of alignment at different DPI |
| Re-render everything per frame | 16 ms frame pacing loop, `request_repaint_after` sprinkled through logic | Timing concerns tangled with presentation |
| Widgets are functions, not components | Sections duplicated `Frame::group` + `separator` + title by hand | Six near-identical section shells, each subtly different |
| Tabs as `String` compared by name | `"Device"`, `"Command Lab"` magic strings | Tab state was stringly-typed and unvalidated |
| Notices as timestamps polled per frame | `unsupported_perf_mode_notice`, `duplicate_key_notice` | Two bespoke expiry mechanisms for what is one concept |

The redesign therefore does not keep the tab set, the section order, or the control
choices. It keeps the **capabilities**.

---

## 2. Product framing

Blade ControlHub is a *hardware control panel for a laptop you are using right now*.
That implies three things the design must honour:

1. **Most visits are short and single-purpose.** "Set Turbo." "Turn the keyboard
   down." The primary surface must make the five most common actions reachable
   without scrolling or navigating.
2. **The hardware is the source of truth, not the UI.** Hotkeys, power events and
   Razer's own key handling change device state while the window is open. Every
   control must be able to move on its own.
3. **Two profiles, one machine.** Every device setting exists twice (AC / Battery),
   and exactly one of them is live at any moment. Editing the inactive profile is a
   legitimate, common act, and the UI must never let the user believe they just
   changed the running state when they did not.

Point 3 is the single biggest usability failure of the old UI: the AC/Battery
switcher was a pair of unlabelled text tabs with a small grey "Active" word beside
them, and the controls below looked identical either way.

---

## 3. Information architecture

Three destinations plus a gated fourth, not four peer tabs. Command Lab stops being
a peer of Device and Settings — it is a power-user workshop behind a flag.

```
┌─ Sidebar ────────┬─ Content ─────────────────────────────┐
│ ● Dashboard      │                                       │
│   Lighting       │   (single scroll region)              │
│   Keys           │                                       │
│   System         │                                       │
│ ─────────────    │                                       │
│   Command Lab *  │                                       │
│                  │                                       │
│ ┌──────────────┐ │                                       │
│ │ Blade 18     │ │                                       │
│ │ AC · Turbo   │ │                                       │
│ └──────────────┘ │                                       │
└──────────────────┴───────────────────────────────────────┘
        200px                    flexible
* only when Advanced Experimental Features is on
```

**Why a sidebar rather than the old top tab strip:** the window is now resizable and
wider than tall; a vertical rail costs nothing horizontally, keeps the destination
list visible while content scrolls, and gives a permanent home to the device status
card — the answer to "what is my machine doing right now", which is the most
frequent implicit question.

### 3.1 The profile model

The AC/Battery choice is **global to the profile-scoped pages**, not per-section,
and it lives in a persistent header bar above the content rather than inside one
page. Only Dashboard and Lighting are profile-scoped; System, Keys and Command
Lab hold settings that exist once for the machine, so the header bar is absent
there — showing it would imply a per-profile behaviour that does not exist:

```
┌────────────────────────────────────────────────────────┐
│  Editing:  [ AC Power  •LIVE ]  [ Battery ]            │
└────────────────────────────────────────────────────────┘
```

Rules:

- The segmented control shows which profile is being **edited**.
- A `LIVE` badge marks the profile Windows is currently running on. It is on exactly
  one segment, always.
- When the edited profile is **not** the live one, the content region takes a visible
  de-emphasis treatment (a 2 px accent-muted left border plus a one-line banner:
  *"Changes apply when the laptop switches to Battery."*). This is the fix for the
  old UI's core ambiguity.
- The window opens on the live profile. If the live profile changes while the window
  is open (charger plugged/unplugged), the `LIVE` badge moves, and the editing
  selection follows **only if** the user has not manually switched.

---

## 4. Pages

### 4.1 Dashboard

The five most common actions, above the fold, no scrolling at the default size.

```
┌─ Performance ─────────────────────────────── ● Turbo ──┐
│  ┌──────────┐┌──────────┐┌──────────┐┌──────────┐      │
│  │  Silent  ││  Quiet   ││ Balanced ││ Perform. │      │
│  └──────────┘└──────────┘└──────────┘└──────────┘      │
│  ┌──────────┐┌──────────┐                              │
│  │  Turbo   ││  Custom  │                              │
│  └──────────┘└──────────┘                              │
│                                                        │
│    CPU   Low ──●──────── Max       (Custom only)       │
│    GPU   Low ────────●── Max                           │
└────────────────────────────────────────────────────────┘

┌─ Cooling ──────────────────────────────────────────────┐
│  Fan   ( Auto )( Manual )                              │
│        ──────────●──────                               │
└────────────────────────────────────────────────────────┘

┌─ Display ──────────────────────────────────────────────┐
│  Refresh rate   ( 60 Hz )( 120 Hz )( 240 Hz )          │
└────────────────────────────────────────────────────────┘

┌─ Keyboard ─────────────────────────────────────────────┐
│  Backlight    [Off] ● ● ● ○ ○                60%       │
│  Effect       [ Ambient                           ▾ ]  │
└────────────────────────────────────────────────────────┘
```

Design decisions and their reasons:

- **Perf modes are a wrapping chip grid, not a fixed 3-column `ui.columns`.** The
  mode list is device- and profile-dependent; a flex grid with `minmax` sizing
  removes the ragged final row the old grid produced.
- **Unsupported modes stay visible but disabled**, with a tooltip giving the reason.
  The old UI rendered them as dead buttons that flashed a transient message on click
  — a discoverability trap. Preserved behaviour: clicking one still surfaces
  *"<Mode> is not supported on this device"*, now as a persistent tooltip rather
  than a 2-second timer.
- **Custom CPU/GPU sliders are disclosed inline under the mode grid**, only when
  Custom is selected and only on the AC profile (firmware limitation, preserved).
  Levels render as named stops (Low / Medium / High / Max), not a bare 0–3 track.
- **Fan is a two-state segmented control plus a conditional slider, and the
  slider carries no number at all.** The manual range comes from firmware
  (`fan_speed_limits`, e.g. 10–46). That value has no unit and no meaning the
  user can act on — it is neither a percentage nor an RPM — so showing it, or
  its range, or a derived percentage, only invites a false reading. The slider
  shows position and nothing else; the segmented control already says whether
  the firmware or the user is in charge.
- **Keyboard backlight is an explicit Off button plus 5 dots, not a 0–255 slider
  stepped by 51.** The firmware has six discrete states — off, then five levels
  up to 100% — and a continuous-looking slider misrepresented that. Off is its
  own target rather than a hidden state at the bottom of a track, so the
  backlight can be killed in one click from any level, and the readout names the
  value ("Off", "20%" … "100%").
- **Refresh rate is a chip row** sourced from `supported_refresh_rates`.

### 4.2 Lighting

```
┌─ Keyboard effect ──────────────────────────────────────┐
│  ┌────────┐┌────────┐┌────────┐┌─────────┐             │
│  │ ▓▓▓▓▓▓ ││ ▒▓██▓▒ ││  ░▒▓█  ││ · : · : │             │
│  │ Static ││  Wave  ││Breathe ││Starlight│             │
│  └────────┘└────────┘└────────┘└─────────┘             │
│  ┌────────┐┌────────┐┌────────┐┌─────────┐             │
│  │Reactive││ Cycle  ││Ambient ││  Audio  │             │
│  └────────┘└────────┘└────────┘└─────────┘             │
└────────────────────────────────────────────────────────┘

┌─ Brightness ───────────────────────────────────────────┐
│  Keyboard          [Off] ● ● ● ○ ○           60%       │
│  Vapour chamber light                        [ ●━━ ]   │  ← Blade 18 only
└────────────────────────────────────────────────────────┘
```

- Effects become a **visual picker**, not a `ComboBox`. A dropdown for eight
  visual choices was the wrong control: the user is picking an *appearance*, and
  appearance is showable.
- **Each preview has to show what the effect actually does**, not merely that it
  moves — a preview that animates the wrong thing is worse than none, because it
  teaches the wrong mapping:
  - *Static* — one colour, no motion.
  - *Cycle* — the **whole keyboard one colour at a time**, walking the rainbow.
  - *Wave* — a rainbow **spread across** the keyboard, travelling along it, in the
    opposite direction to Cycle's colour order.
  - *Breathe* — fades in and out, **a different rainbow colour each breath**, the
    colour swapping while the preview is at its dimmest.
  - *Starlight* — individual keys twinkling at **scattered points**, each with its
    own delay and duration so no pulse is in step with another.
  - *Reactive* — a key lights on press, then decays.
  - *Ambient* — a spread of unrelated colours, since it mirrors the screen.
  - *Audio Bloom* — a **waveform ribbon**: two sine bands at different speeds and
    phases, stroke width pulsing, reading as a Siri-style wave.
- Vapour Chamber Light keeps its toggle, moved here from the old "Other" section.
  "Other" is not a category.
- **The vapour chamber toggle only exists where the hardware does.** Only the Blade
  18 has the illuminated vent, so the descriptor in `librazer` carries a
  `vapour-chamber` feature and the snapshot's `meta.vapour_chamber` says whether
  this model claims it. Without it the toggle is not rendered, "Toggle vapour
  chamber light" is not offered as a device action, and Fn+V falls through to
  Windows rather than doing nothing. A model the app does not recognise falls back
  to a descriptor with no features, so it is treated as not having one.

### 4.3 Keys

Two sections on one page, replacing the old nested tab-within-a-tab.

```
┌─ Razer special keys ───────────────────────────────────┐
│  Every key does what you map it to, nothing before…    │
│  ┌──────────────┬────────────┬──────────────┬───┐      │
│  │ Label        │ Key        │ Action       │   │      │
│  ├──────────────┼────────────┼──────────────┼───┤      │
│  │ [Task mgr  ] │ [ 0x24   ] │ [Run a macro▾]│ ✕│      │
│  │ [Ctrl][Alt][Shift][Win]                      │      │
│  │ [ 1. Ctrl+Shift+Esc          ]  + Add step   │      │
│  ├──────────────────────────────────────────────┤      │
│  │ [          ] │ [ Listen…] │ [Do nothing ▾]│ ✕│      │
│  └──────────────┴────────────┴──────────────┴───┘      │
│  + Add mapping                                         │
└────────────────────────────────────────────────────────┘

┌─ Hypershift ───────────────────────────────────────────┐
│  Keys held with Fn for secondary actions               │
│  … same table shape; Key column captures A–Z / 0–9     │
└────────────────────────────────────────────────────────┘
```

- **Capture is an inline state, not a modal.** Pressing the Key cell puts that cell
  in a listening state with a pulsing ring; Esc cancels. Preserved from the old UI.
  The one modal on the page is the application picker, because choosing from a few
  hundred installed programs is not a cell-sized job.
- **Duplicate keys are rejected inline** on the offending row (red ring + message
  under it), not as a page-level banner that expires after 2 s. An error about a
  specific row belongs on that row and should persist until resolved.
- Razer special-key capture is **hardware-side**: it flips `KEYMAP_LISTENING` and
  waits for the HID reader to report a code. That contract is preserved exactly;
  only the presentation changes.
- Hypershift capture is **webview-side**: a `keydown` listener restricted to A–Z and
  0–9, matching the old `normal_key_code` whitelist byte-for-byte. The *chord* a
  key/macro action sends is captured separately and accepts any key.
- **The Action column is real.** Each row picks one of: do nothing, send a key, run a
  macro, device control, open ControlHub, launch an application, run a command,
  replay a Command Lab capture. The device vocabulary comes from the state snapshot
  (`meta.device_action_labels`), never hard-coded in the page.
- **Sending a key and running a macro are separate actions,** because they are chosen
  in opposite ways. A key is *picked* from a grouped dropdown of the whole virtual-key
  range, which is the only way to reach a key the laptop does not physically have —
  F13–F24, the numpad, media and browser keys. A macro is *captured*, one step at a
  time, as the user performs it. Both carry Ctrl/Alt/Shift/Win.
- **Replaying a capture is only offered while advanced experimental features are
  on,** since it is a Command Lab feature and the page must not advertise one the
  user has not turned on. A row already set to it keeps the option: the binding still
  works, because the saved captures live in the config rather than behind the flag,
  and a dropdown whose current value is missing renders blank.
- **The application picker covers both kinds of application.** Desktop programs come
  from the Start menu's shortcut trees; packaged applications — Notepad, Terminal,
  Calculator, Settings, anything from the Store — have no shortcut at all and come
  from the shell's `AppsFolder`, launched by model ID rather than by path.
- **An action's own control spans the whole list,** on a second line under the row,
  not squeezed into the Action column: a macro's steps and an application plus its
  arguments do not fit in one column. Rows are separated by a rule, since a row is
  no longer one line tall.
- **A row is not finished until its action can run.** `+ Add mapping` stays disabled
  and the row carries a persistent error while a launch action has no application,
  a command is blank, or a macro step has no key. This mirrors
  `KeyAction::is_complete` in the runtime, which drops incomplete rows rather than
  dispatching them.
- **No Razer special key has a built-in action.** Which code a key sends differs
  between Blades, so there is no stock table to be right about: a special key does
  what the user mapped it to and nothing until then. The Key cell shows the
  captured code (`0x24`), never a guessed name — the row's own label is what says
  which key it is.
- **A Hypershift key that already did something says so.** Rows whose key appears
  in `meta.built_in_hypershift` show a warning badge whose tooltip names the
  built-in action being replaced. Custom bindings win; the badge is the only
  warning. Fn+V is absent from that table on a model with no vapour chamber, since
  there is nothing to override.
- **Only one cell listens at a time,** across both tables and every macro step, and
  while anything is listening Esc belongs to it rather than to the window.
- **Hypershift is reachable.** Both sections share one scroll page, so while the
  Hypershift section is out of view a sticky "Jump to Hypershift" control sits at
  the bottom of the page.

### 4.4 System

```
┌─ Battery ──────────────────────────────────────────────┐
│  Charge limit                                          │
│  Off  50  55  60  65  70  75  80                        │
│   ├───┼───┼───┼───●───┼───┼───┤              65 %      │
│  Stops charging at 65% to reduce battery wear.         │
└────────────────────────────────────────────────────────┘

┌─ Function keys ────────────────────────────────────────┐
│  Top row sends       ( F1–F12 )( Media keys )          │
│  Hold Fn for the other behaviour.                      │
└────────────────────────────────────────────────────────┘

┌─ Appearance ───────────────────────────────────────────┐
│  Accent colour   [██] #FFD700                Reset     │
│  Used for the tray icon, the OSD and this window.      │
└────────────────────────────────────────────────────────┘

┌─ Startup ──────────────────────────────────────────────┐
│  Start with Windows                          [ ●━━ ]   │
│  Start as administrator                      [ ━━○ ]   │
│    Avoids repeated UAC prompts. Restarts the app.      │
└────────────────────────────────────────────────────────┘

┌─ Advanced ─────────────────────────────────────────────┐
│  Experimental features                       [ ━━○ ]   │
│  ⚠ May not work on every device. Adds Command Lab.     │
└────────────────────────────────────────────────────────┘
```

- The battery-limit slider is a **ticked, labelled track**, not an index slider with
  a bare value label. The stops are named values, so name them.
- **Every toggle gets a one-line consequence.** The old UI had two paragraphs of
  warning text above unlabelled switches and nothing at all on the others.
- "Start as administrator" states that it restarts the app, because it does.
- **Accent colour is an OS-native colour picker**, keeping the 180 ms commit
  debounce so dragging does not spam the HID device.

### 4.5 Command Lab *(experimental only)*

```
┌─ ⚠ Experimental ───────────────────────────────────────┐
│  Record USB traffic while Synapse changes a setting,   │
│  then replay it from ControlHub.                       │
│  USBPcap driver: Installed                             │
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│  Name              Captured                            │
│  [ Snap Tap on  ]  03 03 01 05 FF…      ▶   ✕          │
│  [              ]  ● Recording  5…4…3   Cancel         │
│  + New capture                              Help ▾     │
└────────────────────────────────────────────────────────┘
```

- Recording state is shown **on the row being recorded** with a live countdown that
  mirrors the OSD (5 steps, 1 s each). The OSD remains the primary feedback surface,
  since the user is in Synapse while recording.
- The captured byte preview keeps its monospace pill + full-list tooltip.
- Terminal outcomes (`Done` / `TooManyCommands` / `NoCommandsRecorded` / `Cancelled`
  / `Failed`) each get a distinct row state, replacing the old two-variant
  `CommandLabRowNotice`.
- Help is a collapsible disclosure, not a toggled block that shifts the page.

---

## 5. Cross-cutting behaviour

### 5.1 Live state

The window is a **view over runtime state that changes without it**. The backend
pushes a full `SettingsState` snapshot on a Tauri event (`state`) whenever a hardware
command succeeds or an external change lands (hotkey, AC transition, external
monitor). The frontend never polls.

**Optimistic echo with reconciliation:** clicking a control updates local state
immediately (so the UI never lags behind a HID round-trip), sends the command, and
then accepts the next authoritative snapshot as truth. If the command fails, the
snapshot restores the real value and an inline error appears on the control that
failed — the old UI silently logged the failure and left the UI showing a value the
hardware never took.

### 5.2 Rate-limited controls

Sliders (fan speed, custom CPU/GPU, keyboard backlight, accent colour) send on a
trailing debounce, never per-pixel, because each send is a blocking HID write. Accent
colour keeps the existing 180 ms window; other sliders use 120 ms.

### 5.3 OSD suppression

Suppression follows **which surface issued the command**, not which surface has
focus. A control in the window already shows its own new value, so the overlay it
would raise is noise and is suppressed. A Razer special key, an Fn combination or
a power transition has nothing else to show for itself, so it keeps its overlay —
including while the window is open and focused.

`Executer::dispatch` is where this is decided, because the device queue is the
last point at which a command's origin is still known. The window is the only
caller that names a profile and a value (`SetPerfMode`, `SetKeyboardBrightness`,
`SetRGBMode`, …); a key cycles, toggles or adjusts. The overlay is
`WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`, so it draws over the
window without taking focus or swallowing a click.

This replaces the earlier focus-based rule
(`settings_window_should_suppress_osd`), which silenced *every* overlay while the
window was focused and so made the hardware keys look dead whenever the user was
in the window — they had always kept working, with nothing to say so. Focus is
still reported from the webview, but only to refresh the snapshot the window may
have missed while it was in the background.

### 5.3.1 Keyboard input while the window is focused

Windows does **not** invoke a `WH_KEYBOARD_LL` hook for keyboard input aimed at
this window. Measured, not assumed: with the window unfocused every key press
reaches the hook, and from the moment it takes focus the hook callback is not
entered at all, while the HID reader keeps delivering Razer keys and Fn
throughout. So while the window has focus, every hook-sourced feature — the
multimedia top row, the brightness keys, the Shift modifier, Hypershift — is dead
unless the window reports the keys itself.

It does. `ui/src/lib/hotkeys.svelte.ts` forwards its own key events to the
`forward_key` command, which runs `key_hook::dispatch_forwarded_key` — the *same*
dispatch the hook runs, so no mapping logic is duplicated. What the window
forwards:

- **F1–F12**, whatever has focus. The hook sees the top row in every other app,
  text fields included, so a focused toggle, slider or input must not be the one
  place it stops working. An F-key the runtime does not consume keeps its default.
- **anything pressed with Fn**, including from a text field, since Fn+D is a
  Hypershift press wherever the caret is.
- **Shift and Alt, always, and never consumed.** These are global state the
  runtime reads for keys that never reach this window at all: `SHIFT_PRESSED` is
  what makes the cycling controls run backwards for a *Razer* key, and
  `ALT_PRESSED` is what keeps Alt+F4 out of the top row's hands. Losing focus
  releases whatever was reported as held, or a Shift the runtime still believed
  was down would reverse every control from then on.

Two things have to be decided in the window rather than by the runtime, because a
Tauri command resolves a tick too late to swallow a keystroke:

- **Fn state**, which arrives as a pushed `fn-state` event from the HID reader.
  Fn is never a virtual key, so the window cannot observe it. The event fires only
  on a real transition, because the `0x00` report marks *every* special-key
  release, not just Fn's.
- **Whether the runtime will consume the key**, computed from the same rules the
  runtime applies — a bound Hypershift key, or the `IsXOR` media/function rule for
  the top row with its `IsFalse(ALT_PRESSED)` guard. That mirror is what stops
  Fn+D from also typing "d", and it is pinned by `hotkeys.test.ts`, since a rule
  duplicated across two languages is a rule that drifts.

A cell waiting for a key press still owns the keyboard: nothing is forwarded while
the Keys page is capturing, or a key would be bound and run its old action at once.

### 5.4 Window behaviour

| Property | Value | Reason |
|---|---|---|
| Default size | 880 × 620 | Dashboard fits without scrolling |
| Min size | 720 × 520 | Sidebar + widest chip row still fit |
| Resizable | Yes | Was fixed at 500 × 750 purely because egui layout could not cope |
| Spawn position | Bottom-right, above the tray | Preserved; it is a tray app |
| Close | Hides, does not exit | Single process now — closing must not kill the hardware runtime |
| Tray left-click | Toggles show/hide | Preserved |
| Startup | Window never shown; tray only | Preserved |

### 5.5 Theming

- Follows the Windows app mode, light or dark, and switches live when the user
  changes it. The runtime listens for the `WM_SETTINGCHANGE` "ImmersiveColorSet"
  broadcast on a hidden top-level window (`win::system::app_theme`), reads
  `AppsUseLightTheme` only when it arrives, and pushes an `app-theme` event; the
  page keys its palette on `data-theme` rather than `prefers-color-scheme`, which
  WebView2 did not re-evaluate for a change made while the app was running. The
  window frame is set to the same mode so the title bar matches.
- Exactly one user-controlled colour: the accent, shared with the tray icon and OSD.
  All other colours derive from a neutral ramp so the accent stays legible on any
  hue. Foreground on accent flips between black and white at luminance 145, matching
  the existing `theme_text_color` rule.
- Performance-mode colours are fixed and semantic (green → red), unchanged from
  `perf_mode_color_components`, and used for the mode dot and tray icon only.

### 5.6 Accessibility

- All controls are real focusable elements; the whole UI is keyboard-navigable in DOM
  order. egui had no tab order at all.
- Every icon-only button (delete, play) carries an `aria-label`.
- `prefers-reduced-motion` disables the effect-tile animations and the capture pulse.
- Minimum contrast 4.5:1 for text, in both themes.

---

## 6. Explicitly preserved hardware contracts

These must not change, because they encode device or firmware facts:

1. Keyboard brightness is `0..=255` stepped by 51: six states, `0` off through
   `255` at 100%.
2. Custom perf mode is AC-only; CPU/GPU levels are `0..=3`.
3. Fan speed `0` means Auto; the manual range comes from firmware limits and is a
   raw, unitless level — never presented as a percentage or a number.
4. Battery limit is chosen from the device's `BATTERY_LIMITS` list, not free-form.
5. Perf-mode availability is per-profile (`allowed_perf_modes`) **and** per-device
   (`perf_modes`); both filters apply.
6. Razer special-key codes must be unique across mappings; same for Hypershift.
   Battery limit, refresh rate, performance mode and RGB effect all cycle
   backwards while Shift is held.
7. Command Lab: 5-second capture, >20 commands is a failure, capture start may raise
   a UAC prompt and blocks until resolved. A capture contains only *other*
   applications' traffic: the device's replies are excluded by transfer
   direction, and ControlHub's own writes are recorded as they are sent and
   subtracted from the result.
8. Turning on "Start as administrator" relaunches elevated; turning it off does not.
9. The OSD is the sole owner of overlay feedback; the window never draws overlays.
    Opening the window suppresses no hardware path: the keyboard hook, the HID
    reader that detects Fn, and every device action a key runs stay live, and
    their overlays still appear over the window.
10. A custom key binding is consulted before the built-in key map, so any key can be
    reclaimed. A bound Hypershift key is swallowed, and keystrokes the runtime
    synthesizes are ignored by the hook so a binding cannot retrigger itself.
    The keyboard hook is not the only source of keys: Windows never calls it for
    input aimed at the settings window, so that window forwards its own key events
    into the same dispatch (see §5.3.1). Both paths must stay one implementation.
11. Overlay feedback follows the action, not the row's label: device actions raise
    the overlay their own handler already owns, launching an application or running
    a command raises one naming the target, and key remaps and macros are silent.
12. An application launched from a binding must not inherit administrator rights when
    ControlHub is running elevated; it is started through Explorer instead.
13. A packaged application is started as `shell:AppsFolder\<model id>`, never by path:
    it has no executable the shell will accept.
14. Icons the shell renders for a packaged application are already premultiplied;
    an icon's own colour bitmap is not. Premultiplying the wrong one darkens it.
