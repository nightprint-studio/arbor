<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  /**
   * The reviewable half of a bulk naming fix: what it would rename, narrowed however you like.
   *
   * A project-wide fix on a legacy tree reaches thousands of names, and "apply all or nothing" is
   * not a review — the whole reason to look is to disagree with part of it. So every name has a
   * tick, every group has a tick, each kind of declaration can be switched off or picked alone, and
   * the name proposed can be typed over.
   *
   * The list is **windowed** (`VirtualList`): four thousand rows of real DOM is a modal that takes
   * seconds to open and stutters when scrolled. Headers and rows are laid out to the same height
   * so one flat array covers both — which is also why a row's detail (the files it reaches, the
   * declaration line, the name editor) lives in a pane under the list instead of unfolding in it.
   *
   * Everything is reachable from the keyboard: arrows move, Space ticks, F2 edits the name, F filters,
   * A / N / I select all / none / invert.
   */
  import { ExternalLink, FileCode2, Pencil, RotateCcw } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import ValueChange from '$lib/components/shared/ui/ValueChange.svelte';
  import VirtualList from '$lib/components/shared/ui/VirtualList.svelte';
  import { readFile } from '$lib/ipc/bennu';
  import type { RenamedName } from '$lib/ipc/bennu/naming';
  import {
    buildLines,
    effectiveName,
    filesOf,
    indicesInGroup,
    invertSelection,
    isValidRename,
    referenceCount,
    selectAll,
    selectNone,
    selectOnly,
    setRename,
    setRow,
    targetCounts,
    type FixFilter,
    type FixLine,
    type GroupBy,
  } from './naming-fix-selection';
  import { targetIcon } from './naming-target-icons';

  interface Props {
    renamed: RenamedName[];
    /** The project root, to read a declaration's line for the detail pane. */
    root: string | null;
    /** The live filter. */
    filter: FixFilter;
    onFilter: (next: FixFilter) => void;
    /** Open a declaration in the editor (the review steps aside). */
    onGoTo: (file: string, line: number) => void;
  }

  let { renamed, root, filter, onFilter, onGoTo }: Props = $props();

  /** Every row is exactly this tall — the window arithmetic depends on it. */
  const ROW_H = 30;

  const kinds = $derived(targetCounts(renamed));
  const lines = $derived(buildLines(renamed, filter));

  const GROUPINGS: { value: GroupBy; label: string }[] = [
    { value: 'file', label: 'Group by file' },
    { value: 'target', label: 'Group by kind' },
    { value: 'none', label: 'No grouping' },
  ];

  const update = (patch: Partial<FixFilter>) => onFilter({ ...filter, ...patch });

  // ── focus and the active row ────────────────────────────────────────────────
  let filterEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let editBox = $state<HTMLDivElement | null>(null);
  let activePos = $state(0);

  const activeLine = $derived<FixLine | null>(lines[Math.min(activePos, lines.length - 1)] ?? null);
  const activeItem = $derived(activeLine?.kind === 'item' ? activeLine : null);

  // ── tick state ──────────────────────────────────────────────────────────────
  const shownItems = $derived(lines.filter((l) => l.kind === 'item').length);
  const shownSelected = $derived(
    lines.filter((l): l is Extract<FixLine, { kind: 'item' }> => l.kind === 'item' && l.selected).length,
  );
  const allShown = $derived(shownItems > 0 && shownSelected === shownItems);
  const someShown = $derived(shownSelected > 0 && shownSelected < shownItems);

  function toggleKindVisible(target: string, on: boolean) {
    const next = new Set(filter.hiddenTargets);
    if (on) next.delete(target);
    else next.add(target);
    update({ hiddenTargets: next });
  }

  function toggleGroup(key: string, on: boolean) {
    const next = new Set(filter.hiddenGroups);
    if (on) next.delete(key);
    else next.add(key);
    // A group ticked back on comes back whole, rather than staying half-empty because of rows that
    // were unticked one by one before it was switched off.
    const excluded = new Set(filter.excluded);
    if (on) for (const i of indicesInGroup(renamed, filter, key)) excluded.delete(i);
    update({ hiddenGroups: next, excluded });
  }

  const onlyGroup = (key: string) =>
    onFilter(selectOnly(renamed, filter, (n) => (filter.by === 'file' ? n.file : n.target) === key));
  const onlyKind = (target: string) => onFilter(selectOnly(renamed, filter, (n) => n.target === target));

  function lineKey(line: FixLine): string {
    return line.kind === 'group' ? `g:${line.key}` : `i:${line.index}`;
  }

  // ── the detail pane ─────────────────────────────────────────────────────────
  const sources = new Map<string, string[]>();
  let declaration = $state<string | null>(null);

  $effect(() => {
    const item = activeItem;
    const project = root;
    declaration = null;
    if (!item || !project) return;
    const { file, line } = item.name;
    let cancelled = false;
    const show = (rows: string[]) => {
      if (!cancelled) declaration = rows[line - 1]?.trimEnd() ?? null;
    };
    const cached = sources.get(file);
    if (cached) {
      show(cached);
    } else {
      readFile(project, file)
        .then((r) => {
          const rows = r.text.split('\n');
          sources.set(file, rows);
          show(rows);
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  });

  const detailFiles = $derived(activeItem ? filesOf(activeItem.name) : []);
  const activeName = $derived(activeItem ? effectiveName(activeItem.name, activeItem.index, filter) : '');

  /** The name being typed in the detail pane; null while the field just shows the effective one. */
  let draft = $state<string | null>(null);
  const draftInvalid = $derived(
    activeItem !== null && draft !== null && draft !== '' && draft !== activeItem.name.to && !isValidRename(activeItem.name, draft),
  );
  $effect(() => {
    void activeItem?.index;
    draft = null;
  });

  function commitDraft() {
    if (activeItem && draft !== null) onFilter(setRename(renamed, filter, activeItem.index, draft));
    draft = null;
  }
  function resetName() {
    if (!activeItem) return;
    onFilter(setRename(renamed, filter, activeItem.index, activeItem.name.to));
    draft = null;
  }

  // ── keyboard ────────────────────────────────────────────────────────────────
  const TEXT_FIELD = 'input, textarea, select, [contenteditable="true"]';

  function move(to: number) {
    if (lines.length === 0) return;
    activePos = Math.max(0, Math.min(lines.length - 1, to));
  }

  function onKeydown(e: KeyboardEvent) {
    const inField = (e.target as HTMLElement | null)?.closest(TEXT_FIELD);
    if (inField) {
      // From the filter box, the arrow hands over to the list; Esc inside it is FilterInput's (clears).
      if (e.key === 'ArrowDown' && inField === filterEl) {
        e.preventDefault();
        listEl?.focus();
      }
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const handled = (() => {
      switch (e.key) {
        case 'ArrowDown': move(activePos + 1); return true;
        case 'ArrowUp': move(activePos - 1); return true;
        case 'PageDown': move(activePos + 8); return true;
        case 'PageUp': move(activePos - 8); return true;
        case 'Home': move(0); return true;
        case 'End': move(lines.length - 1); return true;
        case ' ':
          if (activeLine?.kind === 'item') onFilter(setRow(filter, activeLine.index, !activeLine.selected));
          else if (activeLine?.kind === 'group') toggleGroup(activeLine.key, activeLine.selected === 0);
          return true;
        case 'F2':
        case 'e':
          editBox?.querySelector('input')?.focus();
          return true;
        case 'f':
        case '/':
          filterEl?.focus();
          return true;
        case 'a': onFilter(selectAll(renamed, filter)); return true;
        case 'n': onFilter(selectNone(renamed, filter)); return true;
        case 'i': onFilter(invertSelection(renamed, filter)); return true;
        default: return false;
      }
    })();
    if (handled) e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="review" onkeydown={onKeydown}>
  <div class="toolbar">
    <Checkbox
      checked={allShown}
      indeterminate={someShown}
      disabled={shownItems === 0}
      ariaLabel="Select every name shown"
      onchange={(on) => onFilter(on ? selectAll(renamed, filter) : selectNone(renamed, filter))}
    />
    <div class="search">
      <FilterInput bind:value={() => filter.search, (v) => update({ search: v })} bind:element={filterEl} placeholder="Filter by name or file…  (F)" ariaLabel="Filter names" />
    </div>
    <Select value={filter.by} options={GROUPINGS} onchange={(v) => update({ by: v as GroupBy })} ariaLabel="Grouping" />
    <div class="quick">
      <Button variant="ghost" size="xs" onclick={() => onFilter(selectAll(renamed, filter))}>All</Button>
      <Button variant="ghost" size="xs" onclick={() => onFilter(selectNone(renamed, filter))}>None</Button>
      <Button variant="ghost" size="xs" onclick={() => onFilter(invertSelection(renamed, filter))}>Invert</Button>
    </div>
  </div>

  <!-- Each kind can be hidden (so left out) or picked alone: a run that should only touch constants
       does not need every method unticked one at a time. -->
  {#if kinds.length > 1}
    <div class="kinds">
      {#each kinds as k (k.target)}
        {@const Icon = targetIcon(k.target)}
        <span class="kind">
          <Checkbox
            checked={!filter.hiddenTargets.has(k.target)}
            ariaLabel={`Show ${k.target}`}
            onchange={(on) => toggleKindVisible(k.target, on)}
          />
          <Icon size={12} />
          <span>{k.target}</span>
          <span class="count">{k.count}</span>
          <Button variant="ghost" size="xs" tooltip={`Select only ${k.target}`} onclick={() => onlyKind(k.target)}>only</Button>
        </span>
      {/each}
    </div>
  {/if}

  {#if lines.length === 0}
    <EmptyState message="Nothing matches this filter." />
  {:else}
    <!-- A grid rather than a flex child: a grid item stretches to the track by default, which is
         how the windowed list gets a definite height to measure. -->
    <div class="list-area" bind:this={listEl} tabindex="0" role="application" aria-label="Names this fix would change">
      <VirtualList
        items={lines}
        rowHeight={ROW_H}
        getKey={lineKey}
        scrollTo={activePos}
        role="list"
        ariaLabel="Names this fix would change"
      >
        {#snippet row({ item, index }: { item: FixLine; index: number })}
          {#if item.kind === 'group'}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="grp" class:active={index === activePos} onclick={() => (activePos = index)}>
              <Checkbox
                checked={item.selected > 0}
                indeterminate={item.selected > 0 && item.selected < item.total}
                ariaLabel={`Select ${item.label}`}
                onchange={(on) => toggleGroup(item.key, on)}
              />
              {#if filter.by === 'file'}<FileCode2 size={12} />{/if}
              <span class="grp-label" use:tooltip={item.key}>{item.label}</span>
              <span class="grp-n">{item.selected}/{item.total}</span>
              <Button variant="ghost" size="xs" tooltip="Select only this group" onclick={() => onlyGroup(item.key)}>only</Button>
            </div>
          {:else}
            {@const Icon = targetIcon(item.name.target)}
            {@const refs = referenceCount(item.name)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="itm" class:off={!item.selected} class:active={index === activePos} onclick={() => (activePos = index)}>
              <Checkbox
                checked={item.selected}
                ariaLabel={`Rename ${item.name.from} to ${effectiveName(item.name, item.index, filter)}`}
                onchange={(on) => onFilter(setRow(filter, item.index, on))}
              />
              <span class="kind-icon" use:tooltip={item.name.target}><Icon size={13} /></span>
              <ValueChange from={item.name.from} to={effectiveName(item.name, item.index, filter)} highlight />
              {#if filter.renames.has(item.index)}<span class="typed" use:tooltip={'You typed this name'}><Pencil size={11} /></span>{/if}
              {#if refs > 0}<Badge size="sm" tone="neutral" variant="tone">{refs} use{refs === 1 ? '' : 's'}</Badge>{/if}
              {#if item.name.file_rename}<Badge size="sm" tone="info" variant="tone" tooltip="The file is renamed with it">file</Badge>{/if}
              <span class="where" use:tooltip={item.name.file}>
                {filter.by === 'file'
                  ? `:${item.name.line}`
                  : `${item.name.file.split(/[\\/]/).pop() ?? ''}:${item.name.line}`}
              </span>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    </div>

    <!-- What the highlighted row is: how far it reaches, the line it is declared on, and the name it
         will get — editable. Under the list, not inside it, so every list row stays one height. -->
    <div class="detail" aria-live="polite">
      {#if activeItem}
        {@const Icon = targetIcon(activeItem.name.target)}
        <div class="d-head">
          <Icon size={14} />
          <ValueChange from={activeItem.name.from} to={activeName} highlight />
          <Badge size="sm" tone="accent" variant="tone">{activeItem.name.target}</Badge>
          <Badge size="sm" tone="neutral" variant="tone">
            {referenceCount(activeItem.name)} use{referenceCount(activeItem.name) === 1 ? '' : 's'} · {detailFiles.length} file{detailFiles.length === 1 ? '' : 's'}
          </Badge>
          <span class="d-go">
            <Button variant="secondary" size="xs" onclick={() => onGoTo(activeItem.name.file, activeItem.name.line)}>
              {#snippet iconStart()}<ExternalLink size={12} />{/snippet}
              Go to declaration
            </Button>
          </span>
        </div>
        {#if declaration}<pre class="d-code"><span class="ln">{activeItem.name.line}</span>{declaration}</pre>{/if}
        <div class="d-body">
          <div class="d-edit" bind:this={editBox}>
            <label class="d-label" for="naming-fix-name">New name <span class="hint">(F2)</span></label>
            <div class="d-edit-row">
              <Input
                id="naming-fix-name"
                value={draft ?? activeName}
                oninput={(v) => (draft = v)}
                onchange={commitDraft}
                ariaLabel="New name"
                error={draftInvalid ? 'Not a valid name' : null}
              />
              <Button variant="ghost" size="xs" tooltip="Back to the proposed name" onclick={resetName}
                      disabled={!filter.renames.has(activeItem.index)}>
                {#snippet iconStart()}<RotateCcw size={12} />{/snippet}
                Reset
              </Button>
            </div>
          </div>
          <ul class="d-files" aria-label="Files this rename edits">
            {#each detailFiles.slice(0, 5) as f (f.file)}
              <li use:tooltip={f.file}>
                <span class="f-name">{f.file.split(/[\\/]/).pop()}</span>
                <span class="f-n">{f.edits}</span>
              </li>
            {/each}
            {#if detailFiles.length > 5}<li class="more">+{detailFiles.length - 5} more</li>{/if}
          </ul>
        </div>
      {:else}
        <span class="d-empty">Select a name to see where it is declared and what it reaches.</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .review { display: flex; flex-direction: column; gap: 8px; min-height: 0; flex: 1; }
  .list-area { flex: 1; min-height: 0; display: grid; outline: none; border-radius: var(--radius-sm); }
  .list-area:focus-visible { box-shadow: 0 0 0 1px var(--border-focus, var(--accent)); }

  .toolbar { display: flex; align-items: center; gap: 8px; }
  .search { flex: 1; min-width: 0; }
  .quick { display: flex; gap: 2px; }

  .kinds {
    display: flex; flex-wrap: wrap; gap: 2px 12px;
    padding: 4px 8px;
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
  }
  .kind { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text-secondary); }
  .kind .count { color: var(--text-muted); font-variant-numeric: tabular-nums; }

  /* Both line kinds are exactly ROW_H tall — the window maths assumes it. */
  .grp, .itm {
    display: flex; align-items: center; gap: 8px;
    height: 30px;
    padding: 0 8px;
    font-size: 12px;
    box-sizing: border-box;
    border-radius: var(--radius-sm);
    cursor: default;
  }
  .grp { background: var(--bg-elevated); color: var(--text-secondary); font-weight: 600; }
  .grp-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grp-n { margin-left: auto; font-size: 10px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .itm:hover, .grp:hover { background: var(--bg-hover); }
  .active { box-shadow: inset 2px 0 0 var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .itm.off { opacity: 0.45; }
  .kind-icon { display: inline-flex; color: var(--text-muted); }
  .typed { display: inline-flex; color: var(--warning); }
  .where { margin-left: auto; font-size: 10px; color: var(--text-muted); }

  .detail {
    flex: none; display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px; min-height: 108px; box-sizing: border-box;
    background: var(--bg-elevated); border-radius: var(--radius-md);
  }
  .d-head { display: flex; align-items: center; gap: 8px; font-size: 12px; min-width: 0; }
  .d-go { margin-left: auto; }
  .d-code {
    margin: 0; padding: 3px 8px; font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary);
    background: var(--bg-base); border-radius: var(--radius-sm); overflow: hidden; text-overflow: ellipsis; white-space: pre;
  }
  .d-code .ln { display: inline-block; min-width: 4ch; margin-right: 8px; color: var(--text-muted); text-align: right; }
  .d-body { display: flex; gap: 14px; align-items: flex-start; }
  .d-edit { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .d-label { font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .d-label .hint { text-transform: none; letter-spacing: 0; }
  .d-edit-row { display: flex; align-items: center; gap: 6px; }
  .d-files { list-style: none; margin: 0; padding: 0; width: 220px; font-size: 11px; color: var(--text-secondary); }
  .d-files li { display: flex; gap: 8px; }
  .f-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .f-n { color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .d-files .more { color: var(--text-muted); }
  .d-empty { font-size: 11px; color: var(--text-muted); }
</style>
