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
   * Read-only, highlighted with the selected destination's own dialect mode.
   */
  import { Copy, Code2 } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { DIALECTS, FOLDER_ROLE_SHORT } from '$lib/types/picus';

  const targets = $derived(dmlStore.enabledTargets);

  /** Fall back to the first enabled target when the selected one gets disabled. */
  const target = $derived(
    targets.find((t) => t.id === dmlStore.previewTargetId) ?? targets[0] ?? null,
  );

  const items = $derived<TabItem[]>(
    targets.map((t) => ({
      id: t.id,
      label: `${DIALECTS[t.dialect].short} · ${FOLDER_ROLE_SHORT[t.role]}`,
      title: t.file,
    })),
  );

  const sql = $derived(target ? dmlStore.sqlFor(target) : '');
  const language = $derived(sqlLanguage(target?.dialect));

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

  <div class="sp-code">
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
      {#if target.wrap === 'block'}
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
    font-size: 10.5px;
    color: var(--text-disabled);
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sp-code {
    display: flex;
    height: 300px;
    min-height: 0;
    overflow: hidden;
  }
  .sp-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .sp-note {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-muted);
  }
  .sp-note :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
</style>
