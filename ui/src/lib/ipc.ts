// Typed wrappers over the runtime's Tauri commands and events.
//
// Nothing else in the UI calls `invoke` directly: keeping the surface here
// means the argument shapes are checked once, against the Rust signatures.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BatteryLimit,
  CapturedCommand,
  CommandLabRecordingState,
  IndexedApp,
  KeyBindings,
  PerfMode,
  PowerProfile,
  RGBEffect,
  ThemeColor,
  UiState,
  UsbpcapInfo,
} from "./types";

export const getState = () => invoke<UiState>("get_state");

export const setPerfMode = (profile: PowerProfile, mode: PerfMode) =>
  invoke<void>("set_perf_mode", { profile, mode });

export const setCustomModeConfig = (cpuLevel: number, gpuLevel: number) =>
  invoke<void>("set_custom_mode_config", { cpuLevel, gpuLevel });

export const setFanSpeed = (profile: PowerProfile, speed: number) =>
  invoke<void>("set_fan_speed", { profile, speed });

export const setRefreshRate = (profile: PowerProfile, hz: number) =>
  invoke<void>("set_refresh_rate", { profile, hz });

export const setKeyboardBrightness = (profile: PowerProfile, level: number) =>
  invoke<void>("set_keyboard_brightness", { profile, level });

export const setRgbEffect = (profile: PowerProfile, effect: RGBEffect) =>
  invoke<void>("set_rgb_effect", { profile, effect });

export const setUnderGlow = (profile: PowerProfile, enabled: boolean) =>
  invoke<void>("set_under_glow", { profile, enabled });

export const setBatteryLimit = (limit: BatteryLimit) =>
  invoke<void>("set_battery_limit", { limit });

export const setPrimaryMultimediaKeys = (enabled: boolean) =>
  invoke<void>("set_primary_multimedia_keys", { enabled });

export const setAdvancedExperimentalFeatures = (enabled: boolean) =>
  invoke<void>("set_advanced_experimental_features", { enabled });

export const setStartWithAdmin = (enabled: boolean) =>
  invoke<void>("set_start_with_admin", { enabled });

export const setStartWithWindows = (enabled: boolean) =>
  invoke<void>("set_start_with_windows", { enabled });

export const setThemeColor = (color: ThemeColor) =>
  invoke<void>("set_theme_color", { color });

export const beginRazerKeyCapture = () => invoke<void>("begin_razer_key_capture");
export const cancelRazerKeyCapture = () => invoke<void>("cancel_razer_key_capture");

export const setKeyBindings = (bindings: KeyBindings) =>
  invoke<void>("set_key_bindings", { bindings });

export const listApps = () => invoke<IndexedApp[]>("list_apps");
export const refreshApps = () => invoke<IndexedApp[]>("refresh_apps");
export const pickExecutable = () => invoke<string | null>("pick_executable");

export const beginCommandLabRecord = () =>
  invoke<CommandLabRecordingState>("begin_command_lab_record");
export const cancelCommandLabRecord = () => invoke<void>("cancel_command_lab_record");
export const getCommandLabState = () =>
  invoke<CommandLabRecordingState>("get_command_lab_state");

export const playCommandLabCommands = (commands: CapturedCommand[]) =>
  invoke<void>("play_command_lab_commands", { commands });

export const saveCommandLabCommands = (name: string, commands: CapturedCommand[]) =>
  invoke<void>("save_command_lab_commands", { name, commands });

export const removeCommandLabCommand = (name: string) =>
  invoke<void>("remove_command_lab_command", { name });

export const getUsbpcapStatus = () => invoke<UsbpcapInfo>("get_usbpcap_status");

export const closeGpuApps = () => invoke<void>("close_gpu_apps");
export const hideWindow = () => invoke<void>("hide_window");
export const restartApp = () => invoke<void>("restart_app");
export const quitApp = () => invoke<void>("quit_app");

export const onState = (handler: (state: UiState) => void): Promise<UnlistenFn> =>
  listen<UiState>("state", (event) => handler(event.payload));

export const onRazerKey = (
  handler: (keyCode: number) => void,
): Promise<UnlistenFn> =>
  listen<{ key_code: number }>("razer-key", (event) => handler(event.payload.key_code));

export const onCommandLab = (
  handler: (state: CommandLabRecordingState) => void,
): Promise<UnlistenFn> =>
  listen<CommandLabRecordingState>("command-lab", (event) => handler(event.payload));

/** Normalises the string a rejected command comes back as. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
