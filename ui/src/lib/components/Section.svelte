<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    /** Anchor for jumping to this section from elsewhere on the page. */
    id?: string;
    /** One line under the title explaining what the section changes. */
    hint?: string;
    /** Rendered at the far end of the header row, e.g. a status dot. */
    trailing?: Snippet;
    children: Snippet;
  }

  let { id, title, hint, trailing, children }: Props = $props();
</script>

<section {id}>
  <header>
    <h2>{title}</h2>
    {#if trailing}
      <div class="trailing">{@render trailing()}</div>
    {/if}
  </header>
  {#if hint}
    <p class="hint">{hint}</p>
  {/if}
  <div class="body">
    {@render children()}
  </div>
</section>

<style>
  section {
    /* So a jump lands with the heading clear of the top of the scroll area. */
    scroll-margin-top: var(--gap);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: var(--gap) var(--gap) calc(var(--gap) + 2px);
    box-shadow: var(--shadow);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap);
    margin-bottom: 2px;
  }

  h2 {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }

  .trailing {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    margin-top: var(--gap-sm);
  }
</style>
