<script lang="ts">
  /**
   * "Add step": a two-phase palette. First what the step does, then on which repositories —
   * several at once, from any workspace. `Space` ticks the highlighted repo (or, on a workspace
   * row, all of its repos), `Enter` adds the ticked ones plus the highlighted one.
   */
  import { onMount } from 'svelte';
  import { Check, FolderGit2, Layers } from 'lucide-svelte';
  import CommandPaletteShell, { type PaletteSection } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import type { IconComponent } from '$lib/types/icon';
  import type { PlanRepo, StepKind } from '$lib/types/corvus/plan';
  import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
  import { listRegistryWithRoots } from '$lib/ipc/corvus/workspace';
  import { fuzzyMatchPair } from '$lib/utils/fuzzy';
  import { STEP_KINDS, STEP_META } from './plan-meta';

  /** A repo the user picked, with the branch it has checked out (the default for the step). */
  export type PickedRepo = PlanRepo & { branch: string | null };

  interface Props {
    /** Pre-select the action (e.g. the "Merge MR" shortcut) and go straight to the repos. */
    kind?: StepKind | null;
    onPick: (kind: StepKind, repos: PickedRepo[]) => void;
    onClose: () => void;
  }

  let { kind: initialKind = null, onPick, onClose }: Props = $props();

  // svelte-ignore state_referenced_locally
  let kind = $state<StepKind | null>(initialKind);
  let query = $state('');
  let selectedIdx = $state(0);
  let ticked = $state(new Set<string>());
  let branches = $state(new Map<string, string | null>());

  onMount(async () => {
    try {
      const entries = await listRegistryWithRoots();
      branches = new Map(entries.map(e => [e.path, e.current_branch]));
    } catch { /* branches are a convenience: the step falls back to the repo's own */ }
  });

  const repos = $derived.by<PickedRepo[]>(() => {
    const seen = new Set<string>();
    const out: PickedRepo[] = [];
    for (const ws of workspacesStore.workspaces) {
      for (const id of ws.repo_ids) {
        const r = workspacesStore.registryById.get(id);
        const key = `${ws.id}\0${id}`;
        if (!r || seen.has(key)) continue;
        seen.add(key);
        out.push({ path: r.path, name: r.display_name, workspace: ws.name, branch: branches.get(r.path) ?? null });
      }
    }
    return out;
  });

  const visibleRepos = $derived.by(() => {
    const q = query.trim();
    return q ? repos.filter(r => fuzzyMatchPair(r.name, r.workspace ?? '', q)) : repos;
  });

  const keyOf = (r: PlanRepo) => `${r.workspace ?? ''}\0${r.path}`;

  function commit(extra: PickedRepo[] = []) {
    if (!kind) return;
    const chosen = new Map<string, PickedRepo>();
    for (const r of repos) if (ticked.has(keyOf(r))) chosen.set(r.path, r);
    for (const r of extra) chosen.set(r.path, r);
    if (chosen.size === 0) return;
    onPick(kind, [...chosen.values()]);
  }

  type Row = { id: string; title: string; subtitle: string; icon: string; repos: PickedRepo[] };

  /** Rows of phase 2, workspace by workspace: an "all repos" row, then the repos. */
  const rows = $derived.by<{ ws: string; rows: Row[] }[]>(() => {
    const byWs = new Map<string, PickedRepo[]>();
    for (const r of visibleRepos) byWs.set(r.workspace ?? '—', [...(byWs.get(r.workspace ?? '—') ?? []), r]);
    return [...byWs].map(([ws, list]) => ({
      ws,
      rows: [
        { id: `ws:${ws}`, title: `All repos in ${ws}`, subtitle: `${list.length} repos`, icon: 'ws', repos: list },
        ...list.map(r => ({ id: keyOf(r), title: r.name, subtitle: r.branch ?? '', icon: ticked.has(keyOf(r)) ? 'check' : 'repo', repos: [r] })),
      ],
    }));
  });

  const flat = $derived(rows.flatMap(g => g.rows));

  function toggle(row: Row | undefined) {
    if (!row) return;
    const next = new Set(ticked);
    const all = row.repos.every(r => next.has(keyOf(r)));
    for (const r of row.repos) (all ? next.delete(keyOf(r)) : next.add(keyOf(r)));
    ticked = next;
  }

  const sections = $derived.by<PaletteSection[]>(() => {
    if (!kind) {
      const q = query.trim().toLowerCase();
      return [{
        id: 'steps',
        label: 'Step',
        items: STEP_KINDS
          .filter(k => !q || STEP_META[k].label.toLowerCase().includes(q))
          .map(k => ({
            id: k,
            title: STEP_META[k].label,
            subtitle: STEP_META[k].hint,
            icon: k,
            isVerb: true,
            action: () => { kind = k; query = ''; },
          })),
      }];
    }
    return rows.map(g => ({
      id: g.ws,
      label: g.ws,
      items: g.rows.map(row => ({
        id: row.id,
        title: row.title,
        subtitle: row.subtitle,
        icon: row.icon,
        action: () => commit(row.repos),
      })),
    }));
  });

  function onKeydownCapture(e: KeyboardEvent): boolean {
    if (!kind) return false;
    if (e.key === ' ') {
      e.preventDefault();
      toggle(flat[selectedIdx]);
      return true;
    }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      commit();
      return true;
    }
    return false;
  }

  const icons: Record<string, unknown> = { repo: FolderGit2, check: Check, ws: Layers };
  const resolveIcon = (name: string): IconComponent =>
    (icons[name] ?? STEP_META[name as StepKind]?.icon ?? FolderGit2) as IconComponent;
</script>

<CommandPaletteShell
  {onClose}
  iconResolver={resolveIcon}
  {sections}
  bind:query
  bind:selectedIdx
  verbChip={kind ? { title: STEP_META[kind].label, icon: kind } : null}
  onClearVerb={() => { kind = null; query = ''; ticked = new Set(); }}
  {onKeydownCapture}
  placeholder={kind ? 'Which repositories?  Space ticks · Enter adds' : 'What should the plan do?'}
>
  {#snippet emptyMessage()}{kind ? 'No repository matches' : 'No such step'}{/snippet}
</CommandPaletteShell>
