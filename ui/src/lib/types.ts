// Mirrors the Rust types the runtime pushes. Enum members are the serde
// variant names, which are also what commands accept back, so a value read
// from state can always be sent straight to a command.

export type PowerProfile = "Ac" | "Battery";

export type PerfMode =
  | "BatterySaver"
  | "Silent"
  | "Quiet"
  | "Balanced"
  | "Performance"
  | "Turbo"
  | "Custom"
  | "Unsupported"
  | "Unknown";

export type RGBEffect =
  | "Cycle"
  | "Wave"
  | "Breathe"
  | "Ambient"
  | "AudioBloom"
  | "Static"
  | "Starlight"
  | "Reactive"
  | "Unknown";

export type BatteryLimit =
  | "Off"
  | "Limit50"
  | "Limit55"
  | "Limit60"
  | "Limit65"
  | "Limit70"
  | "Limit75"
  | "Limit80"
  | "Unknown";

export interface ThemeColor {
  r: number;
  g: number;
  b: number;
}

export interface FanSpeeds {
  battery_saver: number;
  silent: number;
  quiet: number;
  balanced: number;
  performance: number;
  turbo: number;
  custom: number;
}

export interface FanSpeedLimits {
  min: number;
  max: number;
}

export interface CustomModeConfig {
  cpu_level: number;
  gpu_level: number;
}

export interface DeviceProfileState {
  keyboard_brightness: number;
  rgb_effect: RGBEffect;
  rgb_effects: RGBEffect[];
  underglow_enabled: boolean;
  perf_mode: PerfMode;
  /** Modes this device reports as supported, independent of the profile. */
  perf_modes: PerfMode[];
  fan_speeds: FanSpeeds;
  refresh_rate: number;
  supported_refresh_rates: number[];
}

export interface CapturedCommand {
  command: number;
  args: number[];
}

export interface UiMeta {
  perf_mode_labels: Record<string, string>;
  perf_mode_colors: Record<string, string>;
  rgb_effect_labels: Record<string, string>;
  battery_limit_labels: Record<string, string>;
  battery_limit_percents: Record<string, number | null>;
  /** Modes each profile offers, before the per-device filter is applied. */
  allowed_perf_modes: { ac: PerfMode[]; battery: PerfMode[] };
  custom_mode_levels: string[];
  keyboard_brightness_step: number;
}

export interface UiState {
  model_name: string;
  current_profile: PowerProfile;
  ac_profile: DeviceProfileState;
  battery_profile: DeviceProfileState;
  custom_mode_config: CustomModeConfig;
  battery_limit: BatteryLimit;
  battery_limits: BatteryLimit[];
  fan_speed_limits: FanSpeedLimits;
  primary_multimedia_keys: boolean;
  advanced_experimental_features: boolean;
  theme_color: ThemeColor;
  start_with_admin: boolean;
  start_with_windows: boolean;
  command_lab_commands: Record<string, CapturedCommand[]>;
  meta: UiMeta;
}

export type CommandLabStatus =
  | "Idle"
  | "Recording"
  | "Done"
  | "Cancelled"
  | "Failed"
  | "TooManyCommands"
  | "NoCommandsRecorded";

export interface CommandLabRecordingState {
  status: CommandLabStatus;
  step: number;
  captured_commands: number;
  commands: CapturedCommand[];
}

export interface UsbpcapInfo {
  installed: boolean;
  available_interfaces: number;
  label: string;
  download_url: string;
}

export const perfModeKey = (mode: PerfMode): string => mode;

/** Fan speed `0` means the firmware decides. */
export const FAN_AUTO = 0;

export const FAN_SPEED_FIELDS: Record<PerfMode, keyof FanSpeeds | null> = {
  BatterySaver: "battery_saver",
  Silent: "silent",
  Quiet: "quiet",
  Balanced: "balanced",
  Performance: "performance",
  Turbo: "turbo",
  Custom: "custom",
  Unsupported: null,
  Unknown: null,
};
