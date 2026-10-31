<script lang="ts">
  /**
   * What Bennu has learned — a window to read it in, and to take a row back.
   *
   * Not a settings page: it is something to open, look at and close, and to find from the command palette. The settings
   * only hold a button to it. One tab per table (usual arguments, what follows what, …), a search over the places, and on
   * every row what it holds and a button to stop offering it. A hidden row stays in the list, struck through, so it can be
   * brought back; what the user did with suggestions has no "hide", only *Forget*, because those counters are theirs.
   */
  import { onMount } from 'svelte';
  import { Brain, EyeOff, Eye, RefreshCw, Trash2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import {
    learnedForgetCorrection, learnedHide, learnedReport, learnedRestore,
    type LearnedGroup, type LearnedItem, type LearnedReport, type LearnedSection,
  } from '$lib/ipc/bennu/learned';

  let { onClose }: { onClose: () => void } = $props();

  let report = $state<LearnedReport | null>(null);
  let failed = $state(false);
  let tab = $state('habits');
  let query = $state('');

  async function load() {
    try {
      report = await learnedReport();
      failed = false;
    } catch {
      failed = true;
    }
  }
  onMount(load);

  const LAYOUT_TAB = 'layout';
  const tabs = $derived<TabItem[]>([
    ...(report?.sections ?? []).map((s) => ({ id: s.id, label: s.title, count: s.groups.length })),
    { id: LAYOUT_TAB, label: 'Layout', count: report?.layout.length ?? 0 },
  ]);
  const section = $derived<LearnedSection | null>(report?.sections.find((s) => s.id === tab) ?? null);

  /** The groups whose place, or one of whose rows, contains what was typed. */
  const shown = $derived.by<LearnedGroup[]>(() => {
    const groups = section?.groups ?? [];
    const q = query.trim().toLowerCase();
    if (!q) return groups;
    return groups.filter((g) => g.key.toLowerCase().includes(q) || g.items.some((i) => i.text.toLowerCase().includes(q)));
  });

  async function toggle(group: LearnedGroup, item: LearnedItem) {
    if (!section?.kind) return;
    const rule = { kind: section.kind, key: group.key, text: item.text };
    await (item.hidden ? learnedRestore(rule) : learnedHide(rule)).catch(() => {});
    item.hidden = !item.hidden;
  }

  /** Hides or brings back a whole place — a rule with no text, so what is counted there later is covered too. */
  async function togglePlace(group: LearnedGroup) {
    if (!section?.kind) return;
    const rule = { kind: section.kind, key: group.key, text: '' };
    await (group.hidden ? learnedRestore(rule) : learnedHide(rule)).catch(() => {});
    group.hidden = !group.hidden;
  }

  async function forgetPlace(group: LearnedGroup) {
    for (const item of group.items) await learnedForgetCorrection(group.key, item.text).catch(() => {});
    await load();
  }

  async function forget(group: LearnedGroup, item: LearnedItem) {
    await learnedForgetCorrection(group.key, item.text).catch(() => {});
    await load();
  }

  const percent = (share: number | null) => (share === null ? '' : `${Math.round(share * 100)}%`);
</script>

<Modal {onClose} width="880px" height="620px" ariaLabel="What Bennu has learned">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Brain size={14} />
      <span class="title">What Bennu has learned</span>
    </ModalHeader>
  {/snippet}

  <div class="learned">
    {#if failed}
      <Alert variant="warning" title="Could not read it">The backend did not answer. Try again in a moment.</Alert>
    {:else if !report}
      <EmptyState message="Reading…" compact />
    {:else}
      <Tabs items={tabs} value={tab} onSelect={(id) => { tab = id; query = ''; }} variant="underline" size="sm" />
      {#if tab !== LAYOUT_TAB}
        <p class="about">{section?.about}</p>
        <div class="search">
          <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Search the places and what was seen there" ariaLabel="Search what was learned" />
        </div>
        <div class="groups">
          {#each shown as group (group.key)}
            <section class="group" class:hidden={group.hidden}>
              <header class="group-head">
                <span class="key">{group.key}</span>
                <span class="total">{group.total}</span>
                {#if section?.kind}
                  <Button variant="ghost" size="sm" onclick={() => togglePlace(group)}
                          ariaLabel={group.hidden ? 'Offer this place again' : 'Stop offering anything here'}>
                    {#snippet iconStart()}{#if group.hidden}<Eye size={13} />{:else}<EyeOff size={13} />{/if}{/snippet}
                    {group.hidden ? 'Offer again' : 'Stop offering all'}
                  </Button>
                {:else}
                  <Button variant="ghost" size="sm" onclick={() => forgetPlace(group)} ariaLabel="Forget all of this kind">
                    {#snippet iconStart()}<Trash2 size={13} />{/snippet}
                    Forget all
                  </Button>
                {/if}
              </header>
              {#each group.items as item (item.text)}
                <div class="row" class:hidden={item.hidden}>
                  <span class="what">
                    <code class="text">{item.text}</code>
                    {#if item.detail}<span class="detail">{item.detail}</span>{/if}
                  </span>
                  <span class="count">{item.count}{section?.kind === '' ? ' settled' : '×'}</span>
                  <span class="share">{percent(item.share)}</span>
                  {#if section?.kind}
                    <Button variant="ghost" size="sm" onclick={() => toggle(group, item)}
                            ariaLabel={item.hidden ? 'Offer it again' : 'Stop offering it'}>
                      {#snippet iconStart()}{#if item.hidden}<Eye size={13} />{:else}<EyeOff size={13} />{/if}{/snippet}
                      {item.hidden ? 'Offer again' : 'Stop offering'}
                    </Button>
                  {:else}
                    <Button variant="ghost" size="sm" onclick={() => forget(group, item)} ariaLabel="Forget it">
                      {#snippet iconStart()}<Trash2 size={13} />{/snippet}
                      Forget
                    </Button>
                  {/if}
                </div>
              {/each}
            </section>
          {:else}
            <EmptyState
              message={query ? 'Nothing matches.' : 'Nothing learned here yet.'}
              description={tab === 'corrections' && !report.learning_on ? 'Turn on “Learn from corrections” in Settings › Completion › Learning to start counting.' : 'It fills as the project is scanned and as you write.'}
            />
          {/each}
        </div>
      {:else}
        <p class="about">How this project lays out its code, where it does it nearly always. Suggestions the model and Bennu write follow it.</p>
        {#each report.layout as line (line)}
          <div class="row"><span class="text plain">{line}</span></div>
        {:else}
          <EmptyState message="No layout is consistent enough yet." description="It needs a few dozen examples of each choice." />
        {/each}
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={load}>
        {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        Refresh
      </Button>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .learned { display: flex; flex-direction: column; gap: 8px; min-height: 0; height: 100%; padding: 0 2px; }
  .about { margin: 6px 2px 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .search { margin: 2px 0; }
  .groups { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding-bottom: 6px; }
  .group { flex-shrink: 0; display: flex; flex-direction: column; border: 1px solid var(--border-subtle); border-radius: var(--radius-md, 8px); overflow: hidden; }
  .group-head {
    display: flex; align-items: baseline; gap: 8px; padding: 5px 10px;
    background: var(--bg-elevated); border-bottom: 1px solid var(--border-subtle);
  }
  .key { font-family: var(--font-code); font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .total { margin-left: auto; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .row {
    display: grid; grid-template-columns: 1fr 80px 44px auto; gap: 10px; align-items: center;
    padding: 3px 10px; min-height: 30px;
  }
  .row + .row { border-top: 1px solid color-mix(in srgb, var(--border-subtle) 50%, transparent); }
  .what { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .detail { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .group.hidden { opacity: 0.55; }
  .text { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .text.plain { font-family: var(--font-ui-sans); color: var(--text-primary); }
  .count, .share { font-size: var(--font-size-2xs); color: var(--text-muted); text-align: right; }
  .row.hidden .text { text-decoration: line-through; opacity: 0.55; }
</style>
