<script lang="ts">
  /**
   * Settings › Imber › Tools and skills: the programs on this machine Imber may run, one row each — its name, the
   * program, how it asks, a switch, Edit and Remove — and "Add a tool…". A row that a run would not offer says
   * why, from the backend's own check, so a tool never looks ready while being silently dropped.
   */
  import { Pencil, Plus, TerminalSquare, X } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ExternalToolModal from './ExternalToolModal.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { externalToolProblems } from '$lib/ipc/bennu/assistant';
  import type { ExternalTool } from '$lib/ipc/bennu/config';
  import { APPROVAL_OPTIONS, programName } from './external-tool-form';

  const s = bennuAssistantStore;

  /** The tool being edited: its index, or `null` for a new one; `undefined` when the modal is closed. */
  let editing = $state<number | null | undefined>(undefined);

  let problems = $state<string[][]>([]);
  $effect(() => {
    const tools = s.externalTools;
    if (!tools.length) { problems = []; return; }
    externalToolProblems(tools).then((p) => (problems = p)).catch(() => (problems = []));
  });

  const approvalLabel = (t: ExternalTool) => APPROVAL_OPTIONS.find((o) => o.value === t.approval)?.label ?? t.approval;

  function save(tool: ExternalTool) {
    s.saveExternalTool(editing ?? null, tool);
    editing = undefined;
  }
</script>

<FormRow label="External tools" description="Programs on this machine Imber may run by name, with no shell: you choose the arguments and variables it may pass, and when it asks. Their output comes back to it; a call past its time limit is stopped. A scheduled run never uses them.">
  <Button variant="secondary" size="sm" onclick={() => (editing = null)}><Plus size={12} /> Add a tool…</Button>
</FormRow>

{#each s.externalTools as t, i (i)}
  {@const wrong = problems[i] ?? []}
  <div class="tool" class:off={!t.enabled}>
    <TerminalSquare size={13} />
    <div class="what">
      <div class="line1">
        <b>{t.name || 'unnamed'}</b>
        <span class="prog" title={t.program}>{programName(t.program)}{t.args.length ? ` ${t.args.join(' ')}` : ''}</span>
      </div>
      <div class="line2">
        <Badge variant="tone" tone="neutral" size="sm">Asks: {approvalLabel(t).toLowerCase()}</Badge>
        {#if t.writes}<Badge variant="tone" tone="warning" size="sm">Changes things</Badge>{/if}
        {#if wrong.length}<span class="wrong" title={wrong.join('\n')}>Not offered: {wrong[0]}</span>{/if}
      </div>
    </div>
    <Toggle checked={t.enabled} size="sm" onchange={(v) => s.setExternalToolEnabled(i, v)} ariaLabel={`Offer ${t.name} to Imber`} />
    <IconButton tooltip="Edit" size={22} onclick={() => (editing = i)}><Pencil size={12} /></IconButton>
    <IconButton tooltip="Remove" size={22} onclick={() => s.removeExternalTool(i)}><X size={12} /></IconButton>
  </div>
{/each}

{#if editing !== undefined}
  <ExternalToolModal tool={editing === null ? null : s.externalTools[editing]} onSave={save} onClose={() => (editing = undefined)} />
{/if}

<style>
  .tool {
    display: flex; align-items: center; gap: 10px;
    margin: 0 14px 6px; padding: 8px 10px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }
  .tool.off { opacity: 0.6; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .line1 { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .line1 b { color: var(--text-primary); font-weight: 600; font-family: var(--font-code); }
  .prog { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .line2 { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .wrong { font-size: var(--font-size-2xs); color: var(--warning); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
