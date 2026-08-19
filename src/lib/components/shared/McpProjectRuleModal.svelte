<!--
  Which projects an AI client may work on, and what it may do in each.

  Lives in `shared/` because it is opened from two different windows: the AI-access
  settings on the home surface (where you audit everything you have granted) and the
  product window a project is open in (where you are when you want to change it). Two
  copies of this form would be two places for the same permission to drift.

  ## Not one project

  It opens ON a project — the one you came from — but it is not about only that one. The
  question "may this project be written to" is never asked in isolation: you ask it while
  remembering what you granted the other three, and a window that showed one row at a time
  made you close and reopen it to compare. So the left column is the whole list, and the
  project you arrived from is simply the one selected.

  And you pick from what Arbor already knows. A folder picker asks "where is it on disk",
  which is the wrong question for a project you have been working in all week — its path is
  something Arbor recorded the moment you opened it. So the rail offers those first and
  keeps the picker as the way in for a folder Arbor has never seen.

  That list is also the scope under **Listed projects**, which is why the banner above it
  says which of the two it currently is. The full scope picker stays on the settings page —
  it is a global mode, not a fact about a project — but the one decision this list itself
  raises ("should these be the ONLY reachable projects?") is answerable here, where the
  list you would be committing to is in front of you.

  ## Inherit is a position, not an absence

  Everything is an OVERRIDE, and "Inherit" is a real, selectable choice rather than the
  lack of one. A rule that says only what it disagrees with keeps following the global
  settings for everything else, so tightening the profile still tightens every project that
  never objected. A form that silently froze today's global values into each project would
  quietly opt every one of them out of the next change. Each inherited row states what it
  resolves to — "Inherit (Ask)" — because the question is "what happens here", and a row
  that only says "inherit" answers it only if you already remember the profile's setting.

  ## Changes apply as you make them

  No Save button, matching every other page of the AI settings: a permission you can see is
  a permission that is in force. The alternative — a draft you might close unsaved — is a
  window whose contents can be a lie about what an assistant is allowed to do right now.
-->
<script lang="ts">
  import { FolderPlus, Layers, ListChecks, ListFilter, Plus, RotateCcw, Trash2 } from 'lucide-svelte';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { getMcpTools } from '$lib/ipc/mcp';
  import { listRecentProjects } from '$lib/ipc/recents';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import { emptyProjectRule, MCP_PRODUCTS, MCP_SAFETY_TIERS, ruleIsEmpty } from '$lib/types/mcp';
  import type {
    McpDecision,
    McpProgramTools,
    McpProjectRule,
    McpToolSummary,
  } from '$lib/types/mcp';

  let {
    root,
    name,
    product,
    onClose,
  }: {
    /** Project to open on. Absent → the first one on the list. */
    root?: string;
    /** Display name for a project that is not on the list yet. */
    name?: string;
    /**
     * The product window this was opened from.
     *
     * Decides which projects are offered FIRST, not which are offered at all: opened from
     * Bennu, Bennu's projects are the ones you came to write a rule about, and burying
     * them alphabetically among every repository Corvus has ever seen is the soup this
     * avoids. The rest stay reachable one fold away, because a rule is about a path and a
     * path does not belong to a product.
     */
    product?: string;
    onClose: () => void;
  } = $props();

  const cfg = $derived(mcpStore.config);

  // This modal opens from windows that are not the home surface (Bennu's own titlebar),
  // and the store is per-window: without this it would render the all-closed defaults as
  // if they were the user's settings, and every "Inherit (…)" label would be a lie.
  const ready = $derived(mcpStore.loaded);
  $effect(() => { void mcpStore.ensureLoaded(); });

  let picked = $state<string | null>(root ?? null);
  let query = $state('');

  /**
   * Projects Arbor has opened, deduped by path.
   *
   * Recents are kept per (product, path), so one project opened in two products is two
   * rows there and must be one row here — the rule is about the path, and offering it
   * twice would be offering the same grant twice.
   */
  let known = $state<{ path: string; name: string; products: string[] }[]>([]);
  let askedForRecents = false;

  $effect(() => {
    if (askedForRecents) return;
    askedForRecents = true;
    void listRecentProjects()
      .then((entries) => {
        const byPath = new Map<string, { path: string; name: string; products: string[] }>();
        for (const e of entries) {
          const hit = byPath.get(e.path);
          if (hit) {
            if (!hit.products.includes(e.product)) hit.products.push(e.product);
          } else {
            byPath.set(e.path, { path: e.path, name: e.name, products: [e.product] });
          }
        }
        known = [...byPath.values()];
      })
      .catch(() => { known = []; });
  });
  let tab = $state('access');
  let error = $state<string | null>(null);
  let picking = $state(false);

  /**
   * The project the arriving window is about, when it has no rule yet.
   *
   * Shown on the list as a pending entry rather than written to the config on open: a
   * modal that added a row merely by being opened would leave a trail of empty rules
   * behind every glance — and under Listed projects, silently widen the scope.
   */
  const pending = $derived(
    root && !cfg.projects.some((p) => p.root === root)
      ? emptyProjectRule(root, name)
      : null,
  );

  const listed = $derived<McpProjectRule[]>(
    pending ? [pending, ...cfg.projects] : cfg.projects,
  );

  const rule = $derived(listed.find((p) => p.root === picked) ?? listed[0] ?? null);

  const needleRoot = $derived(query.trim().toLowerCase());

  function hits(name: string, path: string): boolean {
    return (
      !needleRoot ||
      name.toLowerCase().includes(needleRoot) ||
      path.toLowerCase().includes(needleRoot)
    );
  }

  const withRules = $derived(listed.filter((p) => hits(p.name || p.root, p.root)));

  /** Everything Arbor has opened that is not on the list yet. */
  const offered = $derived(
    known
      .filter((k) => !listed.some((p) => p.root === k.path))
      .filter((k) => hits(k.name, k.path))
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  /** The calling product's own projects, when this was opened from one. */
  const mine = $derived(product ? offered.filter((k) => k.products.includes(product)) : []);
  const others = $derived(product ? offered.filter((k) => !k.products.includes(product)) : offered);
  const isPending = $derived(!!rule && !!pending && rule.root === pending.root);

  const DECISION_LABEL: Record<McpDecision, string> = {
    allow: 'Allow',
    ask: 'Ask',
    deny: 'Refuse',
  };

  /** Write `rule` back, creating its entry the first time it says anything. */
  async function apply(next: McpProjectRule) {
    error = null;
    try {
      await mcpStore.saveProject(next);
      picked = next.root;
    } catch (e) {
      error = String(e);
    }
  }

  function setProduct(id: string, v: string) {
    if (!rule) return;
    const products = { ...rule.products };
    if (v === 'inherit') delete products[id];
    else products[id] = v === 'true';
    void apply({ ...structuredClone($state.snapshot(rule)), products });
  }

  function setTier(key: (typeof MCP_SAFETY_TIERS)[number]['key'], v: string) {
    if (!rule) return;
    const policy = { ...rule.policy, [key]: v === 'inherit' ? null : (v as McpDecision) };
    void apply({ ...structuredClone($state.snapshot(rule)), policy });
  }

  function setTool(toolName: string, v: string) {
    if (!rule) return;
    const tools = { ...(rule.tools ?? {}) };
    if (v === 'inherit') delete tools[toolName];
    else tools[toolName] = v as McpDecision;
    void apply({ ...structuredClone($state.snapshot(rule)), tools });
  }

  function clearTools() {
    if (!rule) return;
    void apply({ ...structuredClone($state.snapshot(rule)), tools: {} });
  }

  async function addRoot(chosen: string, chosenName?: string) {
    picking = false;
    if (!chosen) return;
    await apply(emptyProjectRule(chosen, chosenName));
  }

  const PRODUCT_LABEL: Record<string, string> = Object.fromEntries(
    MCP_PRODUCTS.map((p) => [p.id, p.name]),
  );

  /**
   * A product's display name.
   *
   * `MCP_PRODUCTS` only names the ones with tools of their own, and a project reaches this
   * list because SOME product opened it — Corvus and Merula do, and neither is in there. So
   * an unknown id is capitalised rather than dropped: the row's whole job is to say where
   * you last saw this project.
   */
  function pretty(id: string): string {
    return PRODUCT_LABEL[id] ?? id.charAt(0).toUpperCase() + id.slice(1);
  }

  /** Where this project was last opened, for a row that is otherwise just a name. */
  function openedIn(products: string[]): string {
    return products.map(pretty).join(', ');
  }

  const productLabel = $derived(product ? pretty(product) : '');

  async function remove(target: string) {
    error = null;
    try {
      await mcpStore.removeProject(target);
      picked = null;
    } catch (e) {
      error = String(e);
    }
  }

  const allowlist = $derived(cfg.scope.mode === 'allowlist');

  /** What the list means right now, in the mode that is actually set. */
  const scopeLine = $derived(
    {
      allowlist: 'Only these projects are reachable.',
      open_projects: 'Anything open in Arbor is in reach. These add rules.',
      by_product: 'Each product reaches its own open projects. These add rules.',
      anywhere: 'Every readable file is in reach. These add rules.',
    }[cfg.scope.mode] ?? 'These projects add rules.',
  );

  async function setAllowlist(on: boolean) {
    error = null;
    try {
      await mcpStore.patch({
        scope: { ...cfg.scope, mode: on ? 'allowlist' : 'open_projects' },
      });
    } catch (e) {
      error = String(e);
    }
  }

  /** Options for one class, with the inherited value spelled out. */
  function decisionOptions(inherited: McpDecision) {
    return [
      { value: 'inherit', label: `Inherit (${DECISION_LABEL[inherited]})` },
      { value: 'allow', label: 'Allow' },
      { value: 'ask', label: 'Ask' },
      { value: 'deny', label: 'Refuse' },
    ];
  }

  /**
   * The same three verbs the class rows use, not "Allowed here" / "Refused here". The
   * modal names the project and every row on it is about that project, so "here" was
   * already said — and the longer wording wrapped the segment onto two lines.
   */
  function productOptions(inherited: boolean) {
    return [
      { value: 'inherit', label: `Inherit (${inherited ? 'on' : 'off'})` },
      { value: 'true', label: 'Allow' },
      { value: 'false', label: 'Refuse' },
    ];
  }

  /**
   * A product refused here makes its classes moot, so the class rows are disabled rather
   * than left editable and ineffective — an editable control that changes nothing is a lie
   * about what the form does.
   */
  const allProductsRefused = $derived(
    !!rule && MCP_PRODUCTS.every((p) => rule.products[p.id] === false),
  );

  /** One line saying what a rule does, so the list reads without opening every entry. */
  function summarise(r: McpProjectRule): string {
    if (ruleIsEmpty(r)) return 'Inherits everything';
    const parts: string[] = [];
    const refused = MCP_PRODUCTS.filter((p) => r.products[p.id] === false).map((p) => p.name);
    const added = MCP_PRODUCTS.filter((p) => r.products[p.id] === true).map((p) => p.name);
    if (refused.length) parts.push(`${refused.join(', ')} refused`);
    if (added.length) parts.push(`${added.join(', ')} allowed`);
    for (const tier of MCP_SAFETY_TIERS) {
      const d = r.policy[tier.key];
      if (d) parts.push(`${tier.title.toLowerCase()}: ${DECISION_LABEL[d].toLowerCase()}`);
    }
    const tools = Object.keys(r.tools ?? {}).length;
    if (tools) parts.push(`${tools} tool${tools === 1 ? '' : 's'} set individually`);
    return parts.join(' · ');
  }

  // ── Per tool ────────────────────────────────────────────────────────────────

  let programs = $state<McpProgramTools[] | null>(null);
  let toolQuery = $state('');
  let onlyOverridden = $state(false);
  /** Plain, not `$state`: an effect that reads what it is about to write re-runs itself. */
  let askedForTools = false;

  // Loaded when the tab is first opened, not on mount: reading an inventory starts the
  // backend that holds it, and most visits to this modal never leave the first tab.
  $effect(() => {
    if (tab !== 'tools' || askedForTools) return;
    askedForTools = true;
    void getMcpTools()
      .then((list) => { programs = list; })
      .catch((e) => { error = String(e); programs = []; });
  });

  const overrideCount = $derived(Object.keys(rule?.tools ?? {}).length);
  const needle = $derived(toolQuery.trim().toLowerCase());

  const visible = $derived(
    (programs ?? [])
      .map((p) => ({
        ...p,
        matches: p.tools.filter(
          (t) =>
            (!onlyOverridden || rule?.tools?.[t.name] !== undefined) &&
            (!needle || t.name.includes(needle) || t.title.toLowerCase().includes(needle)),
        ),
      }))
      .filter((p) => p.matches.length > 0),
  );

  /**
   * What a tool resolves to when it says nothing of its own: the project's class override
   * if it has one, else the profile's. The same chain `policy::resolve` walks — a label
   * that guessed differently would be a promise the backend does not keep.
   */
  function inherited(tool: McpToolSummary): McpDecision {
    return rule?.policy[tool.safety] ?? cfg.policy[tool.safety];
  }

  function toolOptions(tool: McpToolSummary) {
    return [
      { value: 'inherit', label: `Inherit (${DECISION_LABEL[inherited(tool)]})` },
      { value: 'allow', label: 'Allow' },
      { value: 'ask', label: 'Ask' },
      { value: 'deny', label: 'Refuse' },
    ];
  }

  function productName(program: string): string {
    return MCP_PRODUCTS.find((p) => p.id === program)?.name ?? program;
  }

  const tabs = $derived<TabItem[]>([
    { id: 'access', label: 'Access', icon: Layers },
    {
      id: 'tools',
      label: 'Per tool',
      icon: ListFilter,
      badge: overrideCount > 0 ? overrideCount : undefined,
    },
  ]);
</script>

{#snippet offer(entry: { path: string; name: string; products: string[] })}
  <SidebarItem onclick={() => addRoot(entry.path, entry.name)}>
    <span class="entry-name muted">{entry.name}</span>
    {#snippet subtitle()}
      <span class="entry-summary">{openedIn(entry.products)}</span>
    {/snippet}
    {#snippet actions()}
      <Plus size={12} />
    {/snippet}
  </SidebarItem>
{/snippet}

<Modal
  {onClose}
  width="920px"
  height="640px"
  padBody={false}
  ariaLabel="AI access by project"
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <ListChecks size={14} />
      <span class="modal-title">AI access by project</span>
      <Badge variant="pill" size="sm" label={String(cfg.projects.length)} />
    </ModalHeader>
  {/snippet}

  <div class="shell">
    {#if !ready}
      <div class="loading"><Spinner /></div>
    {:else}
      <aside class="rail">
        <!-- What the list currently MEANS, and the one scope decision it raises. The full
             four-way picker stays on the settings page: that is a global mode, and having
             both in reach here would be two owners for one setting. -->
        <div class="scope" class:strict={allowlist}>
          <span class="scope-line">{scopeLine}</span>
          <Button
            size="xs"
            variant="outline"
            onclick={() => setAllowlist(!allowlist)}
            title={allowlist
              ? 'Go back to letting an open project be the grant'
              : 'Refuse every path that is not on this list'}>
            {allowlist ? 'Allow open projects' : 'Make this the only scope'}
          </Button>
        </div>

        <div class="rail-filter">
          <SearchBar
            bind:query
            showRegex={false}
            showCounter={false}
            placeholder="Filter projects…"
            ariaLabel="Filter projects" />
        </div>

        <div class="rail-list">
          <SidebarSection label="With rules" badge={withRules.length} expanded>
            {#each withRules as entry (entry.root)}
              <SidebarItem
                selected={rule?.root === entry.root}
                onclick={() => (picked = entry.root)}>
                <span class="entry-name">{entry.name || entry.root}</span>
                {#snippet subtitle()}
                  <span class="entry-summary" class:custom={!ruleIsEmpty(entry)}>
                    {pending && entry.root === pending.root
                      ? 'Not on the list yet'
                      : summarise(entry)}
                  </span>
                {/snippet}
              </SidebarItem>
            {:else}
              <EmptyState compact message="Nothing yet — pick one below." />
            {/each}
          </SidebarSection>

          <!-- What Arbor has already opened. A project you have worked in all week has a
               path Arbor recorded the moment you opened it, so asking you to go and find it
               on disk is asking a question that was already answered.

               Split when we know which product asked: those are the projects you came about.
               The others are one fold away rather than gone — a rule is about a path, and a
               path does not belong to a product. -->
          {#if mine.length}
            <SidebarSection label={`Opened in ${productLabel}`} badge={mine.length} expanded>
              {#each mine as entry (entry.path)}
                {@render offer(entry)}
              {/each}
            </SidebarSection>
          {/if}

          {#if others.length}
            <SidebarSection
              label={product ? 'Opened elsewhere' : 'Opened in Arbor'}
              badge={others.length}
              expanded={!product}>
              {#each others as entry (entry.path)}
                {@render offer(entry)}
              {/each}
            </SidebarSection>
          {/if}

          {#if withRules.length === 0 && offered.length === 0}
            <EmptyState
              compact
              message="Nothing matches."
              description="Arbor lists the projects it has opened; use the folder picker for anything else." />
          {/if}
        </div>

        <div class="rail-foot">
          <Button size="sm" variant="ghost" block onclick={() => (picking = true)}>
            {#snippet iconStart()}<FolderPlus size={13} />{/snippet}
            Add a folder…
          </Button>
        </div>
      </aside>

      <div class="pane">
        {#if !rule}
          <EmptyState
            message="Nothing selected"
            description="Add a project to give it rules of its own." />
        {:else}
          <header class="pane-head">
            <div class="ident">
              <strong>{rule.name || rule.root}</strong>
              <code>{rule.root}</code>
            </div>
            <div class="ident-actions">
              {#if isPending}
                <Badge variant="tone" tone="info" size="sm" label="Not on the list" />
              {:else if ruleIsEmpty(rule)}
                <Badge variant="tone" tone="neutral" size="sm" label="Inherits everything" />
              {:else}
                <Badge variant="tone" tone="accent" size="sm" label="Has its own rules" />
              {/if}
              {#if !isPending}
                <Button
                  size="sm"
                  variant="icon"
                  title="Remove this project from the list"
                  onclick={() => remove(rule.root)}>
                  <Trash2 size={13} />
                </Button>
              {/if}
            </div>
          </header>

          <div class="tabbar">
            <Tabs
              items={tabs}
              value={tab}
              variant="underline"
              size="sm"
              onSelect={(id) => (tab = id)} />
          </div>

          {#if error}
            <div class="pad-x"><Alert variant="error" compact>{error}</Alert></div>
          {/if}

          {#if tab === 'access'}
            <div class="page">
              <section>
                <SectionHeader
                  title="Backends"
                  description="A product refused here stays refused even when it is on everywhere else." />
                {#each MCP_PRODUCTS as product (product.id)}
                  <FormRow label={product.name}>
                    <RadioGroup
                      size="sm"
                      nowrap
                      value={rule.products[product.id] === undefined
                        ? 'inherit'
                        : String(rule.products[product.id])}
                      options={productOptions(cfg.products[product.id] ?? false)}
                      onchange={(v: string) => setProduct(product.id, v)} />
                  </FormRow>
                {/each}
              </section>

              <section>
                <SectionHeader
                  title="What tools may do here"
                  description="The grant a single switch cannot express: that this one project may also be written to, while everything else stays read-only." />
                {#if allProductsRefused}
                  <Alert variant="info" compact>
                    Every backend is refused here, so nothing below can run.
                  </Alert>
                {/if}
                {#each MCP_SAFETY_TIERS as tier (tier.key)}
                  <FormRow label={tier.title} description={tier.blurb}>
                    <RadioGroup
                      size="sm"
                      nowrap
                      disabled={allProductsRefused}
                      value={rule.policy[tier.key] ?? 'inherit'}
                      options={decisionOptions(cfg.policy[tier.key])}
                      onchange={(v: string) => setTier(tier.key, v)} />
                  </FormRow>
                {/each}
              </section>
            </div>
          {:else}
            <div class="tools-tab">
              <div class="toolbar">
                <div class="search">
                  <SearchBar
                    bind:query={toolQuery}
                    showRegex={false}
                    showCounter={false}
                    autofocus
                    placeholder="Filter tools…"
                    ariaLabel="Filter tools" />
                </div>
                <Button
                  size="sm"
                  variant={onlyOverridden ? 'secondary' : 'ghost'}
                  onclick={() => (onlyOverridden = !onlyOverridden)}>
                  Only overridden
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={overrideCount === 0}
                  title="Drop every per-tool override on this project"
                  onclick={clearTools}>
                  {#snippet iconStart()}<RotateCcw size={12} />{/snippet}
                  Reset
                </Button>
              </div>

              {#if !programs}
                <div class="loading"><Spinner /></div>
              {:else}
                <div class="tool-list">
                  {#each visible as program (program.program)}
                    <div class="group-head">
                      <span>{productName(program.program)}</span>
                      <Badge variant="pill" size="sm" label={String(program.matches.length)} />
                    </div>
                    {#each program.matches as tool (tool.name)}
                      {@const override = rule.tools?.[tool.name]}
                      <div class="tool-row" class:overridden={override !== undefined}>
                        <span class="dot" data-safety={tool.safety}></span>
                        <span class="tool-id">
                          <code>{tool.name}</code>
                          <span class="tool-title">{tool.title}</span>
                        </span>
                        <Select
                          size="sm"
                          value={override ?? 'inherit'}
                          options={toolOptions(tool)}
                          quiet
                          highlight={override !== undefined}
                          ariaLabel={`Permission for ${tool.name}`}
                          onchange={(v: string) => setTool(tool.name, v)} />
                      </div>
                    {/each}
                  {/each}

                  {#if visible.length === 0}
                    <EmptyState
                      message={onlyOverridden && !needle
                        ? 'No tool has its own rule here.'
                        : 'Nothing matches.'}
                      description={onlyOverridden && !needle
                        ? 'Every tool follows the classes on the Access tab.'
                        : undefined} />
                  {/if}
                </div>
              {/if}
            </div>
          {/if}
        {/if}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">
        Stored in your Arbor profile against each path — never in the project, so a rule is
        never committed and never reaches anyone else who opens the same repository.
      </span>
      <Button variant="primary" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Choose a project the AI surface may reach"
    onConfirm={addRoot}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)} />
{/if}

<style>
  .shell { display: flex; height: 100%; min-height: 0; }

  /* ── the list ─────────────────────────────────────────────────────────── */

  .rail {
    flex: none;
    width: 258px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
  }

  .scope {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .scope-line { font-size: 11px; line-height: 1.45; color: var(--text-tertiary); }
  /* When the list IS the gate, it says so in the colour the rest of the app uses for
     "this is in force", not in the same grey as an explanatory note. */
  .scope.strict .scope-line { color: var(--accent); font-weight: 500; }

  .rail-filter { flex: none; padding: 8px 10px; border-bottom: 1px solid var(--border-subtle); }

  .rail-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }

  .entry-name {
    font-size: 12.5px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .entry-summary {
    font-size: 11px;
    color: var(--text-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A rule that says something is the reason to look at this list, so it is the row that
     carries colour; one that only inherits stays quiet. */
  .entry-summary.custom { color: var(--accent); }

  /* An offer, not an entry: it reads as available until it is on the list. */
  .entry-name.muted { color: var(--text-secondary); }

  .rail-foot { flex: none; padding: 8px; border-top: 1px solid var(--border-subtle); }

  /* ── the selected project ─────────────────────────────────────────────── */

  .pane { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }

  .pane-head {
    flex: none;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 18px 10px;
  }
  .ident { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .ident strong { font-size: var(--font-size-md); color: var(--text-primary); }
  .ident code {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-tertiary);
    overflow-x: auto;
    white-space: nowrap;
  }
  .ident-actions { display: flex; align-items: center; gap: 6px; flex: none; }

  .tabbar { flex: none; padding: 0 18px; border-bottom: 1px solid var(--border); }
  .pad-x { padding: 10px 18px 0; }

  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  section { display: flex; flex-direction: column; gap: 6px; }

  /* ── per tool ─────────────────────────────────────────────────────────── */

  .tools-tab { flex: 1; min-height: 0; display: flex; flex-direction: column; }

  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 18px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .search { flex: 1; min-width: 0; }

  .tool-list { flex: 1; min-height: 0; overflow-y: auto; padding-bottom: 8px; }

  .group-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px 4px;
    background: var(--bg-base);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-tertiary);
  }

  .tool-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 14px 2px 18px;
    min-height: 28px;
    border-left: 2px solid transparent;
  }
  .tool-row:hover { background: var(--bg-hover); }
  /* A row that disagrees with its class is marked, so the exceptions are findable in a
     list where every other row says "inherit". */
  .tool-row.overridden { border-left-color: var(--accent); }

  .dot { width: 6px; height: 6px; border-radius: 50%; flex: none; }
  .dot[data-safety='read'] { background: var(--info); }
  .dot[data-safety='write'] { background: var(--warning); }
  .dot[data-safety='destructive'] { background: var(--error); }

  .tool-id { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; }
  .tool-id code {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--text-primary);
    white-space: nowrap;
  }
  .tool-title {
    font-size: 11px;
    color: var(--text-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .loading { flex: 1; display: flex; align-items: center; justify-content: center; }
  .hint { font-size: 11px; line-height: 1.5; color: var(--text-tertiary); max-width: 62ch; }
</style>
