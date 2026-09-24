// Trailing debounce for controls that send on every movement.
//
// Each command is a blocking HID write, so a slider must not send once per
// pixel. The local value still updates immediately; only the write waits.

export const SLIDER_COMMIT_MS = 120;
/** The accent colour picker fires continuously while dragging. */
export const COLOR_COMMIT_MS = 180;

export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  waitMs: number,
): ((...args: A) => void) & { flush: () => void; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: A | undefined;

  const fire = () => {
    timer = undefined;
    if (!pending) return;
    const args = pending;
    pending = undefined;
    fn(...args);
  };

  const debounced = (...args: A) => {
    pending = args;
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(fire, waitMs);
  };

  debounced.flush = () => {
    if (timer === undefined) return;
    clearTimeout(timer);
    fire();
  };

  debounced.cancel = () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    pending = undefined;
  };

  return debounced;
}
