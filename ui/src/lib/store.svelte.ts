// The window's single source of state.
//
// The runtime is authoritative: it pushes a full snapshot whenever anything
// changes, including changes this window did not cause (hotkeys, the charger
// being plugged in, an external display arriving). Controls update locally
// first so they never lag behind a HID round-trip, then the next snapshot
// reconciles them.
//
// The one subtlety is a control the user is still holding. A snapshot that
// lands mid-drag would yank the slider back to the last committed value, so a
// control with work in flight re-applies its own value over the snapshot until
// that work settles.

import * as ipc from "./ipc";
import type {
  DeviceProfileState,
  PerfMode,
  PowerProfile,
  UiState,
} from "./types";
import { FAN_SPEED_FIELDS } from "./types";

type Override = (state: UiState) => void;

/**
 * Which profile the window edits once a snapshot arrives.
 *
 * It follows the live profile until the user picks one. A change of power
 * source — the charger plugged in or pulled out — brings it back to the live
 * profile even if they had, and forgets their pick: the machine has just
 * changed under them, and leaving them on the profile that stopped running
 * would put the page's controls on the wrong one without a word.
 */
export function followLiveProfile(
  previousLive: PowerProfile | null,
  nextLive: PowerProfile,
  editing: PowerProfile,
  pinned: boolean,
): { editing: PowerProfile; pinned: boolean } {
  const powerChanged = previousLive !== null && previousLive !== nextLive;
  if (powerChanged || !pinned) return { editing: nextLive, pinned: false };
  return { editing, pinned };
}

class Store {
  state = $state<UiState | null>(null);
  /** Which profile the user is editing; not necessarily the live one. */
  editing = $state<PowerProfile>("Ac");
  /** Per-control failure messages, keyed by the control's id. */
  errors = $state<Record<string, string>>({});
  /** True once the first snapshot has arrived. */
  loaded = $state(false);

  /** Set when the user picks a profile, so live-profile changes stop following. */
  #profilePinned = false;
  /** Local values to keep on top of incoming snapshots while work is pending. */
  #overrides = new Map<string, { token: number; apply: Override }>();
  /** Per-control sequence, so a finished command cannot clear a newer one. */
  #tokens = new Map<string, number>();
  #unlisten: Array<() => void> = [];

  async start() {
    this.#unlisten.push(await ipc.onState((state) => this.#reconcile(state)));

    try {
      this.#reconcile(await ipc.getState());
    } catch (error) {
      // The device may still be initialising; the push worker will deliver a
      // snapshot as soon as it can, so this is not fatal.
      console.warn("Initial state load failed", ipc.errorMessage(error));
    }
  }

  stop() {
    for (const unlisten of this.#unlisten) unlisten();
    this.#unlisten = [];
  }

  // ── Profile ────────────────────────────────────────────────────────────────

  get live(): PowerProfile {
    return this.state?.current_profile ?? "Ac";
  }

  get editingLive(): boolean {
    return this.editing === this.live;
  }

  get profile(): DeviceProfileState | null {
    if (!this.state) return null;
    return this.editing === "Ac" ? this.state.ac_profile : this.state.battery_profile;
  }

  selectProfile(profile: PowerProfile) {
    this.#profilePinned = true;
    this.editing = profile;
  }

  /** Modes offered for the edited profile, with per-device support flagged. */
  get perfModes(): Array<{ mode: PerfMode; supported: boolean }> {
    const state = this.state;
    const profile = this.profile;
    if (!state || !profile) return [];

    const allowed =
      this.editing === "Ac"
        ? state.meta.allowed_perf_modes.ac
        : state.meta.allowed_perf_modes.battery;

    return allowed.map((mode) => ({
      mode,
      supported: profile.perf_modes.includes(mode),
    }));
  }

  /** Fan speed stored for the edited profile's current performance mode. */
  get fanSpeed(): number {
    const profile = this.profile;
    if (!profile) return 0;
    const field = FAN_SPEED_FIELDS[profile.perf_mode];
    return field ? profile.fan_speeds[field] : 0;
  }

  // ── Commands ───────────────────────────────────────────────────────────────

  /**
   * Applies `optimistic` locally, runs `command`, and holds the local value
   * against incoming snapshots until the command settles. On failure the
   * override is dropped and the message is attached to `control`.
   */
  async run(control: string, optimistic: Override, command: () => Promise<unknown>) {
    const token = (this.#tokens.get(control) ?? 0) + 1;
    this.#tokens.set(control, token);

    if (this.state) optimistic(this.state);
    this.#overrides.set(control, { token, apply: optimistic });
    this.#clearError(control);

    try {
      await command();
    } catch (error) {
      this.errors = { ...this.errors, [control]: ipc.errorMessage(error) };
    } finally {
      // A newer command for the same control may have replaced this override
      // while this one was in flight; dropping it then would let the next
      // snapshot pull the control back to a value the user has moved past.
      if (this.#overrides.get(control)?.token === token) {
        this.#overrides.delete(control);
      }
    }
  }

  /**
   * Moves a control locally without writing yet, for a drag whose write is
   * debounced. The value is held against incoming snapshots the same way an
   * in-flight command's is, so an unrelated push landing mid-drag cannot pull
   * the control back to where it started. The following `run` replaces it.
   */
  preview(control: string, optimistic: Override) {
    if (this.state) optimistic(this.state);
    const token = this.#tokens.get(control) ?? 0;
    this.#overrides.set(control, { token, apply: optimistic });
  }

  dismissError(control: string) {
    this.#clearError(control);
  }

  #clearError(control: string) {
    if (!(control in this.errors)) return;
    const { [control]: _dropped, ...rest } = this.errors;
    this.errors = rest;
  }

  #reconcile(next: UiState) {
    const followed = followLiveProfile(
      this.state?.current_profile ?? null,
      next.current_profile,
      this.editing,
      this.#profilePinned,
    );
    this.editing = followed.editing;
    this.#profilePinned = followed.pinned;

    for (const override of this.#overrides.values()) override.apply(next);

    this.state = next;
    this.loaded = true;
  }
}

export const store = new Store();
