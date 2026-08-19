<!--
  Which projects are in play, and what each one says for itself.

  Scope and the project list are one page because they are one thought read at two
  grains: the mode says how a project gets in, the list says what happens once it is.
  Split across two pages, "Listed folders" would name a list the reader cannot see.
-->
<script lang="ts">
  import { Boxes, ChevronRight, FolderOpen, Globe, ListChecks, Settings2 } from 'lucide-svelte';

  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import McpProjectRuleModal from '$lib/components/shared/McpProjectRuleModal.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import { MCP_PRODUCTS, ruleIsEmpty } from '$lib/types/mcp';
  import type { McpProjectRule, McpScopeMode } from '$lib/types/mcp';

  let { guard }: { guard: (action: () => Promise<void>) => Promise<void> } = $props();

  const cfg = $derived(mcpStore.config);

  /**
   * The manager, and which project it opens on. `''` opens it on its own first entry —
   * the state for "manage them all" rather than "this one".
   */
  let managing = $state<string | null>(null);

  const SCOPES: { value: McpScopeMode; label: string; description: string; icon: unknown }[] = [
    { value: 'open_projects', label: 'Open projects', icon: FolderOpen,
      description: 'Anything you have opened in Arbor, in any product. Opening a project is the grant.' },
    { value: 'by_product', label: 'What each product has open', icon: Boxes,
      description: "Each product reaches only its own projects — Bennu's tools work on what you opened in Bennu, and a repository you only opened in Corvus stays out of their reach." },
    { value: 'allowlist', label: 'Listed projects', icon: ListChecks,
      description: 'Only the projects below, whether or not they are open.' },
    { value: 'anywhere', label: 'Anywhere', icon: Globe,
      description: 'Any file this account can read.' },
  ];

  /** One line saying what a row does, so the list reads without opening every entry. */
  function summarise(rule: McpProjectRule): string {
    if (ruleIsEmpty(rule)) return 'Inherits the defaults';
    const parts: string[] = [];
    const refused = MCP_PRODUCTS.filter((p) => rule.products[p.id] === false).map((p) => p.name);
    const added   = MCP_PRODUCTS.filter((p) => rule.products[p.id] === true).map((p) => p.name);
    if (refused.length) parts.push(`${refused.join(', ')} refused`);
    if (added.length)   parts.push(`${added.join(', ')} allowed`);
    for (const [tier, label] of [['read', 'read'], ['write', 'modify'], ['destructive', 'destructive']] as const) {
      const d = rule.policy[tier];
      if (d) parts.push(`${label}: ${d === 'deny' ? 'refused' : d}`);
    }
    // Counted, not listed: a project with six tool overrides would otherwise push the
    // classes off the end of the line, and the count is what tells you to go and look.
    const tools = Object.keys(rule.tools ?? {}).length;
    if (tools) parts.push(`${tools} tool${tools === 1 ? '' : 's'} set individually`);
    return parts.join(' · ');
  }
</script>

<section>
  <SectionHeader
    title="Project scope"
    description="Which paths on disk the tools may touch. A path outside scope is refused without prompting — so a request for something private cannot become a dialog you might click through." />
  <RadioGroup
    value={cfg.scope.mode}
    appearance="card"
    direction="vertical"
    options={SCOPES}
    onchange={(v: string) => guard(() => mcpStore.patch({ scope: { ...cfg.scope, mode: v as McpScopeMode } }))} />

  {#if cfg.scope.mode === 'anywhere'}
    <Alert variant="warning">
      Any file this account can read is in reach, including keys and credentials outside your projects.
    </Alert>
  {/if}

  <!-- Said HERE, where the mode is chosen, and not only in the empty list below it. This
       combination refuses everything, and it does so while looking configured: the client
       still sees the tools, still calls them, and gets a scope refusal for a project that
       is plainly open — which reads as a broken endpoint rather than as a setting. -->
  {#if cfg.scope.mode === 'allowlist' && cfg.projects.length === 0}
    <Alert variant="warning">
      No projects are listed, so <strong>every</strong> call is refused. Add one below, or
      switch to <strong>Open projects</strong> to let opening a project be the grant.
    </Alert>
  {/if}
</section>

<section>
  <!-- Listed in every mode, not only `allowlist`. The list is two things at once: under
       Listed projects it IS the scope, and everywhere it is where a project states what
       it allows for itself. Hiding it in the other modes would hide rules still in force. -->
  <SectionHeader
    title="Projects"
    description={cfg.scope.mode === 'allowlist'
      ? 'This list is the scope, and each entry can also say what is allowed on it in particular.'
      : 'A project here overrides the defaults for itself — the grant a single switch cannot express: that this one may also be written to.'}>
    {#snippet actions()}
      <!-- Adding a project and giving it rules are the same act, and the manager is where
           both happen. A second picker here would be a second place for the list to be
           added to, with only one of them able to say what the new entry allows. -->
      <Button variant="ghost" size="sm" onclick={() => (managing = '')}>
        {#snippet iconStart()}<Settings2 size={14} />{/snippet}
        Manage projects…
      </Button>
    {/snippet}
  </SectionHeader>

  <div class="rows">
    {#each cfg.projects as rule (rule.root)}
      <button type="button" class="row" onclick={() => (managing = rule.root)}>
        <span class="ident">
          <strong>{rule.name || rule.root}</strong>
          <code>{rule.root}</code>
        </span>
        <span class="summary" class:custom={!ruleIsEmpty(rule)}>{summarise(rule)}</span>
        <ChevronRight size={14} />
      </button>
    {:else}
      <EmptyState
        compact
        message={cfg.scope.mode === 'allowlist' ? 'No projects listed' : 'No project has rules of its own'}
        description={cfg.scope.mode === 'allowlist'
          ? 'Every path is refused while this mode is selected — add one from Manage projects.'
          : 'All of them follow the defaults on the Permissions page.'} />
    {/each}
  </div>
</section>

{#if managing !== null}
  <McpProjectRuleModal root={managing || undefined} onClose={() => (managing = null)} />
{/if}

<style>
  section  { display: flex; flex-direction: column; gap: 10px; }
  .rows    { display: flex; flex-direction: column; gap: 6px; align-items: stretch; }

  .row     { display: flex; align-items: center; gap: 12px; width: 100%;
             padding: 7px 10px; text-align: left; cursor: pointer;
             background: var(--bg-base); border: 1px solid var(--border-subtle);
             border-radius: var(--radius-md); color: var(--text-tertiary); }
  .row:hover { border-color: var(--border-strong); background: var(--bg-hover); }
  .row:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

  .ident   { display: flex; flex-direction: column; gap: 1px; min-width: 0; flex: 1; }
  .ident strong { font-size: 12.5px; color: var(--text-primary); }
  .ident code   { font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary);
             overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* A rule that says something is the reason to look at this list, so it is the row
     that carries colour; one that only inherits stays quiet. */
  .summary { flex-shrink: 0; max-width: 42%; font-size: 11.5px; color: var(--text-tertiary);
             overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .summary.custom { color: var(--accent); }
</style>
