# Blade ControlHub

A lightweight, native Windows application for Razer Blade laptops that provides granular hardware control via direct HID communication — no proprietary drivers or Razer Synapse required.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/language-Rust-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows-green.svg)

## Features

### New features
* **Razer Key Mappings:** Uses keyhooks to set the primary $F1 - F12$ actions to multimedia controls, with support for secondary "Razer Hyperboost" functions via the $Fn$ key.
* **Ambient Keyboard Lighting Effect:** Dynamically matches the keyboard backlight to the dominant color on your screen.
* **Adaptive OSD:** Modern, aesthetic design with smooth fade animations. Built on a reusable architecture that supports multiple SVG icons, text labels, and precise indicator levels.
* **Power Profiles:** Automatically switches settings based on your power state (**On Battery** vs. **AC Power**).


### Razer Device Controls
| Category | Controls (SHIFT to cycle backwards) |
|---|---|
| **Performance Mode** | Silent, Quiet, Balanced, Performance, Turbo, Custom |
| **RGB Lighting** | Cycle, Wave, Breathe, Ambient, Starlight, Reactive |
| **Battery Limit** | Off, 50%, 55%, 60%, 65%, 70%, 75%, 80% |
| **Display** | Screen brightness, refresh rate |
| **Keyboard** | Backlight intensity, Function/Multimedia key toggle |
| **Key Mapping** | Custom remapping of Razer special keys (macro-style) |
| **Power Profiles** | Separate settings for plugged-in and battery modes |
| **GPU Mode** | GPU CLI utilities |

## Supported Devices

| Model | VID:PID | Support |
|---|---|---|
| Razer Blade 18 (2025) | `1570:02C7` | Fully supported |
| Razer Blade 14 (2021/2022) | `1570:1016` | WIP |
| Razer Blade 15/17 (2021) | `1570:1043` | WIP |
| Razer Blade 14 (2024) | `1570:1044` | WIP |
| Razer Blade 15/17 (2023) | `1570:1045` | WIP |
| Razer Blade 16 (2024) | `1570:1046` | WIP |
| Razer Blade 14 (2025) | `1570:1047` | WIP |
| Razer Blade 16 (2025) | `1570:1048` | WIP |
| Razer Blade 14 (2024 V2) | `1570:1049` | WIP |

## Architecture

![Blade ControlHub architecture](./assets/architecture.svg)

One process, one owner of the hardware. Tauri's event loop runs on the main
thread and hosts the tray icon and the settings window (a WebView2 view over an
embedded Svelte UI). The OSD keeps its own Win32 thread and message pump, so
overlay latency never depends on the webview.

- **UI:** `ui::webui` builds the Tauri app, its tray and its command surface;
  the settings window calls those commands and receives pushed state, and never
  opens a HID device or persists configuration itself. See
  [`docs/UI-SPEC.md`](./docs/UI-SPEC.md) for what the window is meant to do.
- **OSD:** `ui::osd_controller` owns a stack of click-through layered windows
  rendered with `resvg`. It is fully independent of the UI toolkit. An overlay is
  suppressed per command by whoever issued it — the window's own controls silence
  theirs, the keys keep theirs — so opening the settings window takes nothing
  away from the Razer keys, Fn detection or the actions they run.
- **Hardware:** `razer::DeviceHandle` serializes normal and urgent commands to
  the single `razer::Executer`, which owns `librazer::Device`, config updates,
  and persistence. Every window command is moved off the event loop before it
  touches the device.
- **Windows services:** input hooks, power/standby, display/GPU, external
  monitor, brightness, and ambient workers publish commands or events without
  becoming additional HID owners.
- **State:** `runtime::SettingsState` is the runtime snapshot; a successful HID
  command marks it stale and `ui::webui::push` coalesces those into at most one
  snapshot per window. The window never polls. Persisted `AppConfig` keeps the
  AC and battery profiles while device-backed state is queried by the executor.

## Hardware Control

Blade ControlHub communicates directly with the Razer Blade's embedded controller via HID protocol.

Core device control via locally vendored `librazer` (derived from [razer-ctl](https://github.com/tdakhran/razer-ctl))

## Configuration

Settings are persisted to disk as JSON. The configuration supports **dual power profiles**:

- **Power State** — Settings applied when the laptop is plugged in
- **Battery State** — Settings applied when running on battery

Each profile independently controls: keyboard backlight level, RGB effect, backlight intensity, screen brightness, refresh rate, and performance mode.

## Building

Requires a Rust toolchain and Node.js (the build script compiles the Svelte UI
and Tauri embeds the result in the executable).

```bash
cargo build --release
```

The single self-contained binary is produced at
`target/release/blade-controlhub.exe`. There is no separate UI bundle to ship.

The UI is embedded because the `embedded-ui` feature is on by default, which
turns on Tauri's `custom-protocol`. The Tauri CLI normally supplies that flag;
this project builds with plain cargo, so it is a default instead.

To iterate on the UI with hot reload, start the Vite dev server and build
without that feature, which makes Tauri load `build.devUrl` instead:

```bash
npm --prefix ui run dev          # in one terminal
cargo run --no-default-features  # in another
```

### Runtime requirement

The settings window uses the Microsoft Edge WebView2 runtime, which ships with
Windows 11 and recent Windows 10. Nothing else is required.

## Usage

```bash
# Normal start (shows initialization notification)
blade-controlhub.exe

# Silent start (no startup notification)
blade-controlhub.exe --silent
```

The application enforces single-instance operation — launching a second instance will close the new one.

## License

MIT
