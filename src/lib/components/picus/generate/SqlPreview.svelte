<script lang="ts">
  /**
   * Multi-dialect preview — the SQL each enabled destination would receive.
   *
   * It regenerates as values and rules change: there is no "refresh preview"
   * button by design, because a stale preview is worse than none. The selector
   * above the code switches destinations without leaving the page, so the Oracle
   * and PostgreSQL forms of the same change are one keystroke apart
   * (Alt+←/Alt+→).
   *
   * The SQL comes from `picus-emit` in the backend, so "regenerates" means a
   * debounced round trip rather than a recomputation. Far too fast to earn a
   * spinner — what it earns is honesty: while the shown SQL predates the current
   * model the code is dimmed, so nothing here is ever read as current when it
   * isn't.
   *
   * Read-only, highlighted with the selected destination's own dialect mode.
   */
  import { Copy, Code2 } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { FOLDER_ROLE_SHORT, engineLabel, isDialect } from '$lib/types/picus';

  const targets = $derived(dmlStore.enabledTargets);

  /** Fall back to the first enabled target when the selected one gets disabled. */
  const target = $derived(
    targets.find((t) => t.id === dmlStore.previewTargetId) ?? targets[0] ?? null,
  );

  const items = $derived<TabItem[]>(
    targets.map((t) => ({
      id: t.id,
      label: `${engineLabel(t.dialect)} · ${FOLDER_ROLE_SHORT[t.role]}`,
      title: t.file,
    })),
  );

  const sql = $derived(target ? dmlStore.sqlFor(target) : '');
  // A portable destination has no single dialect to highlight as; the grammar is
  // one permissive superset either way, so the fallback costs nothing visible.
  const language = $derived(sqlLanguage(isDialect(target?.dialect) ? target.dialect : null));
  /** This destination's own rules contradict each other — stated, not swallowed. */
  const conflict = $derived(target ? dmlStore.ruleConflictFor(target.id) : null);
  /** A destination that describes the starting state rather than a change to it. */
  const seeding = $derived(target?.role === 'init' || target?.role === 'data');

  async function copy() {
    try {
      await navigator.clipboard.writeText(sql);
      toastStore.show('SQL copied.', 'success');
    } catch {
      toastStore.show('Could not reach the clipboard.', 'error');
    }
  }
</script>

<div class="sp">
  <div class="sp-head">
    {#if items.length}
      <Tabs
        {items}
        value={target?.id ?? null}
        variant="pill"
        size="sm"
        ariaLabel="Destination to preview"
        onSelect={(id) => dmlStore.setPreviewTarget(id)}
      />
    {/if}
    <span class="sp-spacer"></span>
    {#if target}
      <span class="sp-path" title={target.file}>{target.file}</span>
      <Button variant="icon" size="xs" title="Copy the generated SQL" ariaLabel="Copy the generated SQL" onclick={copy}>
        {#snippet iconStart()}<Copy size={13} />{/snippet}
      </Button>
    {/if}
  </div>

  {#if dmlStore.emitError}
    <div class="sp-banner">
      <Alert
        variant="error"
        compact
        title="The generator could not produce this SQL"
        text={dmlStore.emitError}
      />
    </div>
  {:else if conflict}
    <!-- A rule that cannot hold on this destination. Reported next to the SQL it
         is missing from, because the SQL alone looks perfectly fine. -->
    <div class="sp-banner">
      <Alert variant="warning" compact title="This destination's rules disagree" text={conflict} />
    </div>
  {/if}

  <div class="sp-code" class:sp-dim={dmlStore.previewStale}>
    {#if !target}
      <StateBlock tone="info" label="No destination is enabled — nothing to preview." />
    {:else}
      {#key target.id}
        <CodeEditor value={sql} {language} readOnly />
      {/key}
    {/if}
  </div>

  {#if target}
    <p class="sp-note">
      <Code2 size={12} />
      {#if seeding && dmlStore.operation === 'upsert'}
        <!-- Said where the SQL is, because the SQL does not look like what was
             asked for and the difference is the point. -->
        Written as a plain <b>INSERT</b>: an initialisation runs once against an empty
        database, so "insert if missing" is answered here rather than by the engine — a row
        already in the initialisation is changed where it is.
      {:else if target.wrap === 'block'}
        Wrapped in {target.dialect === 'oracle' ? 'a PL/SQL block' : 'a DO block'}
        {#if target.guards.version}
          , guarded on version {target.guards.version.from} and closing at {target.guards.version.to}
        {/if}.
      {:else}
        Bare statements, appended as they are.
      {/if}
    </p>
  {/if}
</div>

<style>
  .sp { display: flex; flex-direction: column; min-height: 0; }

  .sp-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sp-spacer { flex: 1; }
  .sp-path {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sp-banner { padding: 8px 10px 0; }

  .sp-code {
    display: flex;
    height: 300px;
    min-height: 0;
    overflow: hidden;
    transition: opacity var(--transition-fast);
  }
  .sp-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }
  /* Showing SQL for a model that has moved on. Dimmed rather than blanked or
     spun over: the previous text is still the best answer available, and the
     replacement is milliseconds away. */
  .sp-dim { opacity: 0.5; }

  .sp-note {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .sp-note :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
</style>
