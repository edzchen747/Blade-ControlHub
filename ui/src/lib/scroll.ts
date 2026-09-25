/**
 * Scrolls the page's own scroll area so the element with `id` sits at the top.
 *
 * `scrollIntoView` is not used because it scrolls every ancestor that can be
 * scrolled, including the `overflow: hidden` document, which still moves
 * programmatically. That shoved the whole window up and left the bottom blank.
 */
export function jumpTo(id: string): void {
  const target = document.getElementById(id);
  const container = target?.closest<HTMLElement>(".content");
  if (!target || !container) return;

  const margin = parseFloat(getComputedStyle(target).scrollMarginTop) || 0;
  const top =
    target.getBoundingClientRect().top -
    container.getBoundingClientRect().top +
    container.scrollTop -
    margin;

  container.scrollTo({
    top,
    behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
  });
}
