<script lang="ts">
  import { GitBranch, Check, X, SkipForward, RotateCcw, Bug, Undo2, LogIn, Crosshair, Bookmark } from 'lucide-svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { getGraph } from '$lib/ipc/graph';
  import { checkoutCommit } from '$lib/ipc/branch';
  import { tooltip } from '$lib/actions/tooltip';

  function scrollToCurrent(state: typeof bisectStore.state) {
    const hash = state?.result_hash ?? state?.current_hash;
    if (hash) graphStore.scrollToCommit(hash);
  }

  let { tabId }: { tabId: string } = $props();

  const s = $derived(bisectStore.state);

  async function reloadGraph() {
    graphStore.setGraph(await getGraph(tabId, 0, 500));
  }

  async function handleMark(mark: 'good' | 'bad' | 'skip') {
    const hash = s?.current_hash;
    if (!hash) return;
    try {
      await bisectStore.mark(tabId, hash, mark);
      await reloadGraph();
      scrollToCurrent(bisectStore.state);
    } catch (err) {
      uiStore.showToast(`Bisect: ${err}`, 'error');
    }
  }

  async function handleCheckout() {
    const hash = s?.current_hash;
    if (!hash) return;
    try {
      await checkoutCommit(tabId, hash);
      await reloadGraph();
      uiStore.showToast(`Checked out ${hash.slice(0, 7)} for testing`, 'info');
    } catch (err) {
      uiStore.showToast(`Checkout failed: ${err}`, 'error');
    }
  }

  async function handleUndo() {
    try {
      await bisectStore.undoLastMark(tabId);
      await reloadGraph();
      scrollToCurrent(bisectStore.state);
    } catch (err) {
      uiStore.showToast(`Bisect undo: ${err}`, 'error');
    }
  }

  async function handleReset() {
    try {
      await bisectStore.reset(tabId);
      await reloadGraph();
      uiStore.showToast('Bisect session ended', 'info');
    } catch (err) {
      uiStore.showToast(`Bisect reset failed: ${err}`, 'error');
    }
  }

  async function handleSaveAndPause() {
    try {
      await bisectStore.saveAndPause(tabId);
      uiStore.showToast('Session saved — bisect paused. Resume it from the sidebar.', 'success', 5000);
    } catch (err) {
      uiStore.showToast(`Save failed: ${err}`, 'error');
    }
  }

  const shortHash = $derived(s?.current_hash ? s.current_hash.slice(0, 7) : null);
  const resultShort = $derived(s?.result_hash ? s.result_hash.slice(0, 7) : null);

  // A real bisect range requires at least one good commit.
  // Before that, current_hash may exist (git sets BISECT_HEAD immediately in
  // --no-checkout mode) but it is NOT a computed midpoint — it's just the bad
  // commit itself, so we must not present it as "next to test".
  const hasGood = $derived((s?.good_hashes?.length ?? 0) > 0);

  // Waiting for good = session active, no result, no good marked yet.
  const waitingForGood = $derived(
    s?.active && !s.result_hash && !hasGood
  );
  // Midpoint available = range established (bad + good) AND git has a midpoint.
  const hasMidpoint = $derived(
    s?.active && !s.result_hash && hasGood && !!s.current_hash
  );
</script>

{#if s?.active}
  <div class="bisect-banner" class:result={!!s.result_hash}>
    <div class="banner-left">
      {#if s.result_hash}
        <!-- ── Result ── -->
        <Bug size={13} class="banner-icon result-icon" />
        <span class="banner-label">First bad commit found:</span>
        <button
          class="hash-chip result-chip hash-chip-btn"
          onclick={() => s?.result_hash && graphStore.scrollToCommit(s.result_hash)}
          use:tooltip={'Go to this commit in the graph'}
        >
          <Crosshair size={10} />
          {resultShort}
        </button>
        {#if s.result_message}
          <span class="result-msg">{s.result_message}</span>
        {/if}

      {:else if hasMidpoint}
        <!-- ── Midpoint ready: next commit to test ── -->
        <GitBranch size={13} class="banner-icon" />
        <span class="banner-label">Bisect</span>
        <span class="separator">—</span>
        <span class="testing-label">next to test:</span>
        <code class="hash-chip">{shortHash}</code>
        {#if s.steps_remaining != null}
          <span class="steps">~{s.steps_remaining} steps left</span>
        {/if}

      {:else if waitingForGood}
        <!-- ── Waiting for a good commit ── -->
        <GitBranch size={13} class="banner-icon" />
        <span class="banner-label">Bisect</span>
        <span class="separator">—</span>
        <span class="guide-text">right-click a known good commit in the graph</span>
      {/if}
    </div>

    <div class="banner-actions">
      {#if hasMidpoint}
        <!--
          No-checkout mode: the working tree is untouched.
          "Checkout" lets the user switch to this commit if they need to run
          tests on it. Good/Bad/Skip mark it based on the user's knowledge.
        -->
        <button
          class="btn btn-checkout"
          onclick={handleCheckout}
          use:tooltip={'Checkout this commit to run tests on it (optional)'}
        >
          <LogIn size={12} />
          Checkout
        </button>
        <div class="btn-separator"></div>
        <button class="btn btn-good"  onclick={() => handleMark('good')} use:tooltip={'Mark as Good'}>
          <Check size={12} /> Good
        </button>
        <button class="btn btn-bad"   onclick={() => handleMark('bad')}  use:tooltip={'Mark as Bad'}>
          <X size={12} /> Bad
        </button>
        <button class="btn btn-skip"  onclick={() => handleMark('skip')} use:tooltip={'Skip this commit'}>
          <SkipForward size={12} /> Skip
        </button>
      {/if}

      {#if s.can_undo && !s.result_hash}
        <button class="btn btn-undo" onclick={handleUndo} use:tooltip={'Undo last mark'}>
          <Undo2 size={12} /> Undo
        </button>
      {/if}

      {#if hasMidpoint}
        <button class="btn btn-save" onclick={handleSaveAndPause} use:tooltip={'Save session and pause for later'}>
          <Bookmark size={12} /> Save &amp; Pause
        </button>
      {/if}

      <button class="btn btn-reset" onclick={handleReset} use:tooltip={'End bisect session'}>
        <RotateCcw size={12} /> Reset
      </button>
    </div>
  </div>
{/if}

<style>
  .bisect-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 12px;
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-elevated));
    border-top: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
    font-size: 12px;
    flex-shrink: 0;
    min-height: 34px;
  }

  .bisect-banner.result {
    background: color-mix(in srgb, var(--color-bisect, #e05252) 8%, var(--bg-elevated));
    border-top-color: color-mix(in srgb, var(--color-bisect, #e05252) 35%, transparent);
    border-bottom-color: color-mix(in srgb, var(--color-bisect, #e05252) 20%, transparent);
  }

  .banner-left {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  :global(.banner-icon)  { color: var(--accent); flex-shrink: 0; }
  :global(.result-icon)  { color: var(--color-bisect, #e05252); }

  .banner-label   { color: var(--text-secondary); font-weight: 500; flex-shrink: 0; }
  .separator      { color: var(--text-muted); }
  .testing-label  { color: var(--text-muted); flex-shrink: 0; }

  .hash-chip {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    flex-shrink: 0;
  }

  .result-chip {
    color: var(--color-bisect, #e05252);
    background: color-mix(in srgb, var(--color-bisect, #e05252) 12%, transparent);
    border-color: color-mix(in srgb, var(--color-bisect, #e05252) 25%, transparent);
  }

  .hash-chip-btn {
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    transition: background 0.12s, box-shadow 0.12s;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--color-bisect, #e05252) 45%, transparent);
    text-underline-offset: 2px;
  }
  .hash-chip-btn:hover {
    background: color-mix(in srgb, var(--color-bisect, #e05252) 25%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-bisect, #e05252) 50%, transparent);
  }

  .result-msg {
    color: var(--text-secondary);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .steps {
    color: var(--text-muted);
    font-size: 11px;
    flex-shrink: 0;
  }

  .guide-text {
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }

  /* ── Action buttons ── */
  .banner-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .btn-separator {
    width: 1px;
    height: 16px;
    background: var(--border-subtle);
    margin: 0 2px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid transparent;
    transition: background 0.1s, opacity 0.1s;
    line-height: 1;
  }
  .btn:active { opacity: 0.8; }

  .btn-checkout {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--accent);
  }
  .btn-checkout:hover {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }

  .btn-good {
    background: color-mix(in srgb, var(--color-bisect-good, #3fb950) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-bisect-good, #3fb950) 35%, transparent);
    color: var(--color-bisect-good, #3fb950);
  }
  .btn-good:hover { background: color-mix(in srgb, var(--color-bisect-good, #3fb950) 20%, transparent); }

  .btn-bad {
    background: color-mix(in srgb, var(--color-bisect, #e05252) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-bisect, #e05252) 35%, transparent);
    color: var(--color-bisect, #e05252);
  }
  .btn-bad:hover { background: color-mix(in srgb, var(--color-bisect, #e05252) 20%, transparent); }

  .btn-skip {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
    color: var(--text-secondary);
  }
  .btn-skip:hover { background: var(--bg-overlay); color: var(--text-primary); }

  .btn-undo, .btn-reset {
    background: transparent;
    border-color: var(--border-subtle);
    color: var(--text-muted);
  }
  .btn-undo:hover, .btn-reset:hover {
    border-color: var(--border);
    color: var(--text-secondary);
  }

  .btn-save {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--accent);
  }
  .btn-save:hover {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
</style>
