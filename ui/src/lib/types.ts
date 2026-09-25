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

/**
 * A two-state control built out of two Command Lab captures: `on_capture` is
 * replayed to switch it on, `off_capture` to switch it off. Both name a saved
 * capture, and are empty until the user has chosen one.
 */
export interface CustomToggle {
  name: string;
  on_capture: string;
  off_capture: string;
  enabled: boolean;
}

/**
 * What the Dashboard's Custom Controls section leaves out, by name. Everything
 * is shown by default, so this lists the exceptions — a capture recorded later
 * appears on its own rather than waiting to be added.
 */
export interface HiddenDashboardControls {
  captures: string[];
  controls: string[];
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
  /** Device actions a key can be bound to, keyed by the value a command takes. */
  device_action_labels: Record<DeviceAction, string>;
  /** What each key does before it is rebound, keyed by key code. */
  built_in_bindings: {
    razer: Record<number, string>;
    hypershift: Record<number, string>;
  };
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
  custom_toggles: CustomToggle[];
  hidden_dashboard_controls: HiddenDashboardControls;
  key_bindings: KeyBindings;
  meta: UiMeta;
}

// ── Key bindings ─────────────────────────────────────────────────────────────

export type DeviceAction =
  | "cycle_perf_mode"
  | "cycle_rgb_effect"
  | "cycle_refresh_rate"
  | "cycle_battery_limit"
  | "toggle_underglow"
  | "keyboard_light_up"
  | "keyboard_light_down"
  | "screen_brightness_up"
  | "screen_brightness_down"
  | "toggle_mic_mute"
  | "toggle_speaker_mute"
  | "toggle_trackpad"
  | "toggle_primary_multimedia_keys"
  | "close_gpu_apps";

/** Bit positions in `Chord.modifiers`, mirroring `win::input::binding`. */
export const MOD_CTRL = 1;
export const MOD_ALT = 2;
export const MOD_SHIFT = 4;
export const MOD_WIN = 8;

export interface Chord {
  modifiers: number;
  /** A Windows virtual-key code; `0` means the chord has no key yet. */
  key: number;
}

export type KeyAction =
  | { kind: "none" }
  | { kind: "key"; chord: Chord }
  | { kind: "macro"; steps: Chord[] }
  | { kind: "device"; action: DeviceAction }
  | { kind: "toggle_ui" }
  | { kind: "launch_app"; path: string; args: string; name: string }
  | { kind: "run_command"; command: string }
  | { kind: "replay_capture"; name: string }
  /** Flips a custom control; the runtime decides which of its captures runs. */
  | { kind: "toggle_custom_control"; name: string };

export type ActionKind = KeyAction["kind"];

export interface KeyBinding {
  key_code: number;
  label: string;
  action: KeyAction;
}

export interface KeyBindings {
  razer: KeyBinding[];
  hypershift: KeyBinding[];
}

/**
 * The keys a "key" action can send, grouped for the dropdown.
 *
 * This is the reason that action exists: a binding is not limited to keys the
 * laptop physically has, so the full virtual-key range is offered rather than
 * whatever can be pressed to capture it. Codes are Windows virtual-key codes,
 * which is what the runtime sends.
 *
 * Bare Ctrl, Alt and Shift are deliberately absent: a synthesized keystroke is
 * pressed and released at once, so binding a key to "Shift" could not hold it
 * down and the entry would promise something that does not work. They are
 * available as modifiers instead.
 */
export const KEY_CATALOG: Array<{ group: string; keys: Array<[number, string]> }> = [
  {
    group: "Letters",
    keys: Array.from({ length: 26 }, (_, index) => [
      0x41 + index,
      String.fromCharCode(0x41 + index),
    ]),
  },
  {
    group: "Digits",
    keys: Array.from({ length: 10 }, (_, index) => [0x30 + index, String(index)]),
  },
  {
    group: "Function keys",
    keys: Array.from({ length: 24 }, (_, index) => [0x70 + index, `F${index + 1}`]),
  },
  {
    group: "Editing",
    keys: [
      [0x08, "Backspace"],
      [0x09, "Tab"],
      [0x0d, "Enter"],
      [0x1b, "Esc"],
      [0x20, "Space"],
      [0x2d, "Insert"],
      [0x2e, "Delete"],
    ],
  },
  {
    group: "Navigation",
    keys: [
      [0x25, "Left"],
      [0x26, "Up"],
      [0x27, "Right"],
      [0x28, "Down"],
      [0x24, "Home"],
      [0x23, "End"],
      [0x21, "Page Up"],
      [0x22, "Page Down"],
    ],
  },
  {
    group: "Numeric keypad",
    keys: [
      ...(Array.from({ length: 10 }, (_, index) => [0x60 + index, `Numpad ${index}`]) as Array<
        [number, string]
      >),
      [0x6a, "Numpad *"],
      [0x6b, "Numpad +"],
      [0x6d, "Numpad -"],
      [0x6e, "Numpad ."],
      [0x6f, "Numpad /"],
    ],
  },
  {
    group: "Punctuation",
    keys: [
      [0xba, "; :"],
      [0xbb, "= +"],
      [0xbc, ", <"],
      [0xbd, "- _"],
      [0xbe, ". >"],
      [0xbf, "/ ?"],
      [0xc0, "` ~"],
      [0xdb, "[ {"],
      [0xdc, "\ |"],
      [0xdd, "] }"],
      [0xde, "' \""],
    ],
  },
  {
    group: "Media",
    keys: [
      [0xad, "Mute"],
      [0xae, "Volume down"],
      [0xaf, "Volume up"],
      [0xb0, "Next track"],
      [0xb1, "Previous track"],
      [0xb2, "Stop"],
      [0xb3, "Play/Pause"],
    ],
  },
  {
    group: "Browser",
    keys: [
      [0xa6, "Browser back"],
      [0xa7, "Browser forward"],
      [0xa8, "Browser refresh"],
      [0xa9, "Browser stop"],
      [0xaa, "Browser search"],
      [0xab, "Browser favourites"],
      [0xac, "Browser home"],
    ],
  },
  {
    group: "Launch",
    keys: [
      [0xb4, "Mail"],
      [0xb5, "Media player"],
      [0xb6, "Application 1"],
      [0xb7, "Application 2"],
    ],
  },
  {
    group: "System",
    keys: [
      [0x2c, "Print Screen"],
      [0x13, "Pause"],
      [0x14, "Caps Lock"],
      [0x90, "Num Lock"],
      [0x91, "Scroll Lock"],
      [0x5b, "Left Windows"],
      [0x5c, "Right Windows"],
      [0x5d, "Menu"],
      [0x5f, "Sleep"],
    ],
  },
];

/** One application the Start menu index found. */
export interface IndexedApp {
  name: string;
  /** The shortcut, or `shell:AppsFolder\<id>` for a packaged application. */
  path: string;
  /** The executable it resolves to, or the model ID of a packaged application. */
  target: string;
  /** A base64 PNG data URL, or empty when no icon could be read. */
  icon: string;
  /** Packaged applications have no executable path worth showing. */
  packaged: boolean;
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
