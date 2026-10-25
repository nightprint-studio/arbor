<!--
  Grants — a temporary override of the gates, handed out for a stretch when nobody is at the
  screen.

  The consent prompt needs a person; an assistant left to work does not have one. A grant is
  the same answer given in advance, and it is what makes the closed default usable rather
  than merely safe: the products gate can stay off — no Corvus on the AI surface at all —
  and a grant switches it on for its own stretch, on the projects you name, for the tools you
  name, for as long as its budget lasts.

  It is narrowed on every axis: named projects, named tools, a budget of uses and/or time. It
  cannot widen its own reach — a call whose paths stray outside those projects is refused with
  no prompt, exactly as before — and it lives in memory only, so quitting Arbor ends it. That
  last part is a property, not a shortcut: a grant written to disk is a grant that can be
  written, and what a grant authorises is an assistant holding file tools.
-->
<script lang="ts">
  import { FolderOpen, KeyRound, Trash2 } from 'lucide-svelte';

  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import type { ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import { getMcpTools, listMcpGrants, mintMcpGrant, revokeAllMcpGrants, revokeMcpGrant } from '$lib/ipc/mcp';
  import { listRecentProjects } from '$lib/ipc/recents';
  import type { McpGrant, McpProgramTools } from '$lib/types/mcp';

  import McpGrantCard from './McpGrantCard.svelte';
  import McpGrantToolPicker from './McpGrantToolPicker.svelte';

  let { guard }: { guard: (action: () => Promise<void>) => Promise<void> } = $props();

  let grants = $state<McpGrant[]>([]);
  /** Every tool the surface can carry — reads included, since a grant now opens the gate
      that refuses them, not just the prompt that interrupts them. */
  let programs = $state<McpProgramTools[]>([]);
  let projects = $state<{ path: string; name: string }[]>([]);

  let label = $state('');
  let roots = $state<string[]>([]);
  let chosen = $state<Record<string, boolean>>({});
  let uses = $state(20);
  let minutes = $state(60);
  let browsing = $state(false);

  const picked = $derived(Object.keys(chosen).filter((t) => chosen[t]));
  /** The products this grant would put on the surface that are not on it already. */
  const wouldExpose = $derived(
    programs
      .filter((p) => !p.exposed && p.tools.some((t) => chosen[t.name]))
      .map((p) => p.program),
  );

  const problem = $derived.by(() => {
    if (!label.trim()) return 'Give the grant a name.';
    if (!roots.length) return 'Choose at least one project.';
    if (!picked.length) return 'Choose at least one tool.';
    if (!uses && !minutes) return 'Set a number of uses, a duration, or both.';
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
      .then((all) => { programs = all; })
      .catch(() => { programs = []; });
    void listRecentProjects()
      .then((entries) => {
        const seen = new Map<string, string>();
        for (const e of entries) if (!seen.has(e.path)) seen.set(e.path, e.name);
        projects = [...seen].map(([path, name]) => ({ path, name }));
      })
      .catch(() => { projects = []; });
  });

  /** Recents, plus anything browsed to that is not among them, so a chosen project is always
      a chip that can be clicked off again. */
  const projectChips = $derived<ChipItem[]>([
    ...projects.map((p) => ({ id: p.path, label: p.name || p.path, tooltip: p.path })),
    ...roots
      .filter((r) => !projects.some((p) => p.path === r))
      .map((r) => ({ id: r, label: r.split(/[\\/]/).filter(Boolean).pop() || r, tooltip: r })),
  ]);

  function mint() {
    if (problem) return;
    void guard(async () => {
      await mintMcpGrant({
        label: label.trim(),
        roots,
        tools: picked,
        max_uses: uses > 0 ? uses : null,
        ttl_secs: minutes > 0 ? minutes * 60 : null,
      });
      label = '';
      chosen = {};
      roots = [];
      await refresh();
    });
  }

  function revokeOne(id: string) {
    void guard(async () => { await revokeMcpGrant(id); await refresh(); });
  }

  function revokeEvery() {
    void guard(async () => { await revokeAllMcpGrants(); await refresh(); });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); mint(); }
  }
</script>

<!-- Ctrl/Cmd+Enter creates the grant. On the window rather than on the section: this page is
     only mounted while it is showing, and a handler on a non-interactive element is an a11y
     error. -->
<svelte:window onkeydown={onKeydown} />

<section>
  <SectionHeader
    title="Active grants"
    description="Permissions that are running out as you read them. They disappear when the budget is spent, when they expire, or when Arbor quits.">
    {#snippet actions()}
      {#if grants.length > 1}
        <Button variant="ghost" size="sm" onclick={revokeEvery}>
          {#snippet iconStart()}<Trash2 size={14} />{/snippet}
          Revoke all
        </Button>
      {/if}
    {/snippet}
  </SectionHeader>

  <div class="cards">
    {#each grants as grant (grant.id)}
      <McpGrantCard {grant} onrevoke={() => revokeOne(grant.id)} />
    {:else}
      <EmptyState
        compact
        message="No active grants"
        description="Nothing is pre-approved, so a product that is off stays off and a call that needs confirmation waits for you at the screen." />
    {/each}
  </div>
</section>

<section aria-label="New grant">
  <SectionHeader
    title="Hand out a grant"
    description="Let an assistant work unattended on projects you name — including with a product you keep switched off the rest of the time." />

  <FormRow label="Name" description="How you will recognise it in the list.">
    <Input bind:value={label} placeholder="Overnight refactor" ariaLabel="Grant name" />
  </FormRow>

  <div class="field">
    <div class="field-head">
      <span class="field-title">Projects</span>
      <span class="field-note">
        {roots.length
          ? `${roots.length} chosen — a call is covered only when every path in it is inside one of them`
          : 'A call is covered only when every path in it is inside one of these'}
      </span>
      <span class="spacer"></span>
      <Button variant="outline" size="sm" onclick={() => (browsing = true)}>
        {#snippet iconStart()}<FolderOpen size={14} />{/snippet}
        Browse…
      </Button>
    </div>
    {#if projectChips.length}
      <ChipBar
        items={projectChips}
        selected={roots}
        multi
        size="sm"
        ariaLabel="Projects this grant covers"
        onSelect={(sel: string | string[]) => (roots = Array.isArray(sel) ? sel : [sel])} />
    {:else}
      <EmptyState
        compact
        message="No recent projects"
        description="Browse to the folder this grant should cover." />
    {/if}
  </div>

  <div class="field">
    <span class="field-title">Tools</span>
    <McpGrantToolPicker
      {programs}
      {chosen}
      onchange={(next: Record<string, boolean>) => (chosen = next)} />
  </div>

  <FormRow label="Uses" description="Calls it covers. 0 for no count limit.">
    <NumberStepper bind:value={uses} min={0} max={1000} step={5} ariaLabel="Number of uses" />
    <span class="preset">
      {#each [10, 50, 200] as n (n)}
        <Button variant="ghost" size="sm" onclick={() => (uses = n)}>{n}</Button>
      {/each}
    </span>
  </FormRow>
  <FormRow label="Duration" description="Minutes it stays valid, at most 24 h. 0 for no time limit.">
    <NumberStepper bind:value={minutes} min={0} max={1440} step={15} ariaLabel="Duration in minutes" />
    <span class="preset">
      {#each [{ m: 30, l: '30 min' }, { m: 120, l: '2 h' }, { m: 480, l: '8 h' }] as p (p.m)}
        <Button variant="ghost" size="sm" onclick={() => (minutes = p.m)}>{p.l}</Button>
      {/each}
    </span>
  </FormRow>

  {#if wouldExpose.length}
    <Alert variant="warning">
      This grant switches on {wouldExpose.join(' and ')} for as long as it lasts, even though
      it is off above — and only the tools you ticked, which are the only ones a client will
      be shown.
    </Alert>
  {/if}
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
    title="Add a project to the grant"
    onConfirm={(p: string) => {
      if (!roots.includes(p)) roots = [...roots, p];
      browsing = false;
    }}
    onCancel={() => (browsing = false)}
    onClose={() => (browsing = false)} />
{/if}

<style>
  section     { display: flex; flex-direction: column; gap: 10px; }
  .cards      { display: flex; flex-direction: column; gap: 6px; }

  .field      { display: flex; flex-direction: column; gap: 6px; }
  .field-head { display: flex; align-items: baseline; gap: 8px; }
  .field-title { font-size: 12.5px; color: var(--text-primary); font-weight: 500; }
  .field-note { font-size: 11px; color: var(--text-tertiary); }
  .spacer     { flex: 1; }

  .preset     { display: flex; gap: 2px; }
  .actions    { display: flex; justify-content: flex-end; }
</style>
