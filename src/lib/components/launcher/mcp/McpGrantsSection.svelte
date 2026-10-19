<!--
  Grants — pre-approvals handed out for a stretch when nobody is at the screen.

  The consent prompt needs a person; an assistant left to work does not have one. A grant is
  the same answer given in advance, and narrowed on every axis: one project, named tools,
  and a budget of uses and/or time. It replaces the prompt and nothing else — a call the
  rules above refuse stays refused — and it lives in memory only, so quitting Arbor ends it.
-->
<script lang="ts">
  import { FolderOpen, KeyRound, Trash2 } from 'lucide-svelte';

  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { getMcpTools, listMcpGrants, mintMcpGrant, revokeAllMcpGrants, revokeMcpGrant } from '$lib/ipc/mcp';
  import { listRecentProjects } from '$lib/ipc/recents';
  import type { McpGrant, McpToolSummary } from '$lib/types/mcp';

  let { guard }: { guard: (action: () => Promise<void>) => Promise<void> } = $props();

  let grants = $state<McpGrant[]>([]);
  /** Tools a grant can cover: the ones that would otherwise ask. Reads never prompt. */
  let tools  = $state<McpToolSummary[]>([]);
  let projects = $state<{ path: string; name: string }[]>([]);

  let label    = $state('');
  let root     = $state('');
  let chosen   = $state<Record<string, boolean>>({});
  let uses     = $state(1);
  let minutes  = $state(60);
  let browsing = $state(false);

  const picked = $derived(Object.keys(chosen).filter((t) => chosen[t]));
  const problem = $derived.by(() => {
    if (!label.trim())      return 'Give the grant a name.';
    if (!root)              return 'Choose a project.';
    if (!picked.length)     return 'Choose at least one tool.';
    if (!uses && !minutes)  return 'Set a number of uses, a duration, or both.';
    return null;
  });

  async function refresh() {
    try { grants = await listMcpGrants(); } catch { grants = []; }
  }

  // The list is a countdown, so it is re-read while the page is on screen.
  $effect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 15_000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    void getMcpTools()
      .then((all) => { tools = all.flatMap((p) => p.tools).filter((t) => t.safety !== 'read'); })
      .catch(() => { tools = []; });
    void listRecentProjects()
      .then((entries) => {
        const seen = new Map<string, string>();
        for (const e of entries) if (!seen.has(e.path)) seen.set(e.path, e.name);
        projects = [...seen].map(([path, name]) => ({ path, name }));
      })
      .catch(() => { projects = []; });
  });

  const projectOptions = $derived([
    ...(root && !projects.some((p) => p.path === root) ? [{ value: root, label: root }] : []),
    ...projects.map((p) => ({ value: p.path, label: p.name || p.path })),
  ]);

  function mint() {
    if (problem) return;
    void guard(async () => {
      await mintMcpGrant({
        label: label.trim(),
        root,
        tools: picked,
        max_uses: uses > 0 ? uses : null,
        ttl_secs: minutes > 0 ? minutes * 60 : null,
      });
      label = '';
      chosen = {};
      await refresh();
    });
  }

  function remaining(g: McpGrant): string {
    const parts: string[] = [];
    if (g.uses_left !== null) parts.push(`${g.uses_left} use${g.uses_left === 1 ? '' : 's'} left`);
    if (g.expires_in_secs !== null) {
      const m = Math.max(1, Math.round(g.expires_in_secs / 60));
      parts.push(m >= 120 ? `${Math.round(m / 60)} h left` : `${m} min left`);
    }
    return parts.join(' · ');
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); mint(); }
  }
</script>

<section>
  <SectionHeader
    title="Active grants"
    description="Pre-approved calls that are still usable. They disappear when they run out, when they expire, or when Arbor quits.">
    {#snippet actions()}
      {#if grants.length > 1}
        <Button variant="ghost" size="sm" onclick={() => guard(async () => { await revokeAllMcpGrants(); await refresh(); })}>
          Revoke all
        </Button>
      {/if}
    {/snippet}
  </SectionHeader>

  <div class="rows">
    {#each grants as g (g.id)}
      <div class="row">
        <span class="ident">
          <strong>{g.label}</strong>
          <code>{g.root}</code>
          <span class="tools">{g.tools.join(', ')}</span>
        </span>
        <Badge variant="tone" tone="info">{remaining(g)}</Badge>
        <Button variant="ghost" size="sm" ariaLabel="Revoke grant {g.label}"
                onclick={() => guard(async () => { await revokeMcpGrant(g.id); await refresh(); })}>
          {#snippet iconStart()}<Trash2 size={14} />{/snippet}
        </Button>
      </div>
    {:else}
      <EmptyState compact message="No active grants"
        description="Without one, a call that asks for confirmation waits for you at the screen." />
    {/each}
  </div>
</section>

<!-- Ctrl/Cmd+Enter creates the grant. On the window rather than on the section: this page is only
     mounted while it is showing, and a handler on a non-interactive element is an a11y error. -->
<svelte:window onkeydown={onKeydown} />

<section aria-label="New grant">
  <SectionHeader
    title="Hand out a grant"
    description="Let an assistant make specific calls on one project without asking each time — for example to commit while you are away." />

  <FormRow label="Name" description="How you will recognise it in the list.">
    <Input bind:value={label} placeholder="Overnight refactor" ariaLabel="Grant name" />
  </FormRow>

  <FormRow label="Project" description="The grant covers calls whose paths all lie inside it." wideControl>
    <Select value={root} options={projectOptions} ariaLabel="Project"
            onchange={(v: string) => (root = v)} />
    <Button variant="outline" size="sm" onclick={() => (browsing = true)}>
      {#snippet iconStart()}<FolderOpen size={14} />{/snippet}
      Browse…
    </Button>
  </FormRow>

  <div class="tool-list">
    <span class="tool-title">Tools</span>
    {#each tools as t (t.name)}
      <Toggle
        checked={chosen[t.name] ?? false}
        label={t.title}
        description={`${t.name} — ${t.safety === 'destructive' ? 'destructive' : 'modifies files or history'}`}
        onchange={(v: boolean) => (chosen = { ...chosen, [t.name]: v })} />
    {:else}
      <EmptyState compact message="No tool needs confirmation"
        description="Enable a product on the Endpoint page, or set a class to ask on Permissions." />
    {/each}
  </div>

  <FormRow label="Uses" description="Calls it covers. 0 for no count limit.">
    <NumberStepper bind:value={uses} min={0} max={1000} step={1} ariaLabel="Number of uses" />
  </FormRow>
  <FormRow label="Duration" description="Minutes it stays valid, at most 24 h. 0 for no time limit.">
    <NumberStepper bind:value={minutes} min={0} max={1440} step={15} ariaLabel="Duration in minutes" />
    <span class="unit">min</span>
  </FormRow>

  {#if problem}
    <Alert variant="info">{problem}</Alert>
  {/if}
  <div class="actions">
    <Button variant="primary" disabled={problem !== null} onclick={mint}>
      {#snippet iconStart()}<KeyRound size={14} />{/snippet}
      Create grant
    </Button>
  </div>
</section>

{#if browsing}
  <FileExplorerModal
    mode="folder"
    title="Choose the project the grant covers"
    onConfirm={(p: string) => { root = p; browsing = false; }}
    onCancel={() => (browsing = false)}
    onClose={() => (browsing = false)} />
{/if}

<style>
  section  { display: flex; flex-direction: column; gap: 10px; }
  .rows    { display: flex; flex-direction: column; gap: 6px; align-items: stretch; }

  .row     { display: flex; align-items: center; gap: 12px; padding: 7px 10px;
             background: var(--bg-base); border: 1px solid var(--border-subtle);
             border-radius: var(--radius-md); }
  .ident   { display: flex; flex-direction: column; gap: 1px; min-width: 0; flex: 1; }
  .ident strong { font-size: 12.5px; color: var(--text-primary); }
  .ident code   { font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary);
                  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tools   { font-size: 11px; color: var(--text-tertiary); }

  .tool-list  { display: flex; flex-direction: column; gap: 8px; }
  .tool-title { font-size: 12.5px; color: var(--text-primary); font-weight: 500; }
  .unit    { font-size: 12px; color: var(--text-tertiary); }
  .actions { display: flex; justify-content: flex-end; }
</style>
