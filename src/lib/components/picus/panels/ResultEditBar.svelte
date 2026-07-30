<script lang="ts">
  /**
   * The two buttons that end an editing session, and the notice that says what
   * happened.
   *
   * Deliberately a bar and not a pair of icons in the toolbar. While there are
   * pending edits the grid holds values that are **not in the database**, and that
   * state has to be impossible to miss — a user who scrolls away and comes back must
   * see immediately that three cells are waiting rather than saved.
   *
   * *Store* and *Restore* are the two ways out, and there is no third: no autosave,
   * no write on blur, no "save on close". Every one of those turns a mistyped value
   * into an `UPDATE` nobody chose to run.
   */
  import { Save, Undo2, X } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { resultEditStore } from '$lib/stores/picus/result-edit.svelte';

  interface Props {
    /** Runs the write, and re-reads the query afterwards. */
    onStore: () => void;
  }

  let { onStore }: Props = $props();

  const store = resultEditStore;

  /** "3 cells in 2 rows" — both numbers, because they answer different questions. */
  const summary = $derived.by(() => {
    const cells = store.pending.length;
    const rows = store.rowCount;
    const cellText = `${cells} cell${cells === 1 ? '' : 's'}`;
    return rows === cells ? cellText : `${cellText} in ${rows} row${rows === 1 ? '' : 's'}`;
  });
</script>

{#if store.dirty}
  <div class="reb" role="status">
    <span class="reb-count">{summary} changed and not saved</span>
    <span class="reb-spacer"></span>
    <Button
      variant="primary"
      size="xs"
      disabled={store.saving}
      tooltip={{ content: 'Write these changes to the database', shortcut: 'Ctrl+S' }}
      onclick={onStore}
    >
      {#snippet iconStart()}<Save size={12} />{/snippet}
      Store
    </Button>
    <Button
      variant="secondary"
      size="xs"
      disabled={store.saving}
      tooltip={'Put every cell back to the value it was read with'}
      onclick={() => store.revert()}
    >
      {#snippet iconStart()}<Undo2 size={12} />{/snippet}
      Restore
    </Button>
  </div>
{/if}

{#if store.error}
  <div class="reb-notice">
    <Alert variant="error" compact dismissible onclose={() => store.revert()}>
      {store.error}
    </Alert>
  </div>
{:else if store.outcome}
  <div class="reb-notice">
    <!-- The SQL is shown, not summarised. It is the one write in this product the
         user did not read beforehand, so they get to read it afterwards — and it is
         paste-ready for the script this change probably also belongs in. -->
    <Alert
      variant={store.outcome.warning ? 'warning' : 'success'}
      compact
      dismissible
      onclose={() => store.clearOutcome()}
    >
      <div class="reb-done">
        <span>
          {store.outcome.affected.toLocaleString()} row(s) updated.
          {store.outcome.warning ?? ''}
        </span>
        <pre class="reb-sql">{store.outcome.sql}</pre>
      </div>
    </Alert>
  </div>
{/if}

<style>
  .reb {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    padding: 4px 8px;
    background: var(--warning-bg, var(--bg-hover));
    border-bottom: 1px solid var(--border-subtle);
  }
  .reb-spacer { flex: 1; }
  .reb-count { font-size: 11px; font-weight: 600; color: var(--text-secondary); }

  .reb-notice { flex-shrink: 0; padding: 6px 8px 0; }

  .reb-done { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  /* Scrolls in its own box: a batch of twenty updates must not stretch the panel. */
  .reb-sql {
    margin: 0;
    max-height: 108px;
    overflow: auto;
    font-family: var(--font-code);
    font-size: 10.5px;
    line-height: 1.55;
    color: var(--text-muted);
    white-space: pre;
  }
</style>
