<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    /** One line under the title explaining what the dialog is for. */
    hint?: string;
    onclose: () => void;
    children: Snippet;
  }

  let { title, hint, onclose, children }: Props = $props();

  let dialog = $state<HTMLDialogElement | null>(null);

  // `showModal` has to run after the element exists, and only once: calling it
  // on an already-open dialog throws.
  $effect(() => {
    if (dialog && !dialog.open) dialog.showModal();
  });

  // Esc is spoken for twice over on this page — it cancels a key capture, and
  // it hides the window — so while a dialog is open it must mean "close the
  // dialog" and go no further.
  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    onclose();
  }

  /** A click on the backdrop lands on the dialog itself, not its contents. */
  function onClick(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }
</script>

<dialog bind:this={dialog} onkeydown={onKeyDown} onclick={onClick} oncancel={onclose}>
  <div class="panel">
    <header>
      <div class="stack-sm">
        <h2>{title}</h2>
        {#if hint}<p class="hint">{hint}</p>{/if}
      </div>
      <button type="button" class="icon" aria-label="Close" onclick={onclose}>✕</button>
    </header>
    <div class="body">
      {@render children()}
    </div>
  </div>
</dialog>

<style>
  dialog {
    border: none;
    padding: 0;
    background: transparent;
    color: var(--fg);
    max-width: none;
    max-height: none;
    width: 100%;
    height: 100%;
  }

  dialog::backdrop {
    background: color-mix(in srgb, #000 55%, transparent);
  }

  .panel {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    width: min(520px, calc(100% - var(--gap-lg) * 2));
    max-height: calc(100% - var(--gap-lg) * 2);
    background: var(--bg-raised);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--gap);
    padding: var(--gap) var(--gap) var(--gap-sm);
    border-bottom: 1px solid var(--line);
  }

  h2 {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    padding: var(--gap);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
</style>
