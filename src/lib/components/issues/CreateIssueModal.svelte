<script lang="ts">
  import { X, ChevronDown, Check, Tag, User, AlertCircle } from 'lucide-svelte';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { IssueStatus } from '$lib/types/issues';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let title         = $state('');
  let titleEl: HTMLInputElement | undefined = $state();
  $effect(() => { titleEl?.focus(); });
  let description   = $state('');
  let teamId        = $state(issuesStore.filterOptions?.teams[0]?.id ?? '');
  let statusId      = $state('');
  let priority      = $state<number | undefined>(undefined);
  let labelIds      = $state<string[]>([]);
  let projectId     = $state('');
  let milestoneId   = $state('');
  let assigneeMe    = $state(false);
  let dueDate       = $state('');
  // Estimate is the Linear story-point value. Stored as `number | null`
  // (rather than string) so it round-trips cleanly through the
  // NumberStepper widget — empty / cleared cell ⇒ null ⇒ undefined in
  // the submission payload.
  let estimate      = $state<number | null>(null);
  let saving        = $state(false);
  let error         = $state('');

  // Dropdown open states
  let teamDropOpen      = $state(false);
  let statusDropOpen    = $state(false);
  let priorityDropOpen  = $state(false);
  let labelsDropOpen    = $state(false);
  let projectDropOpen   = $state(false);
  let milestoneDropOpen = $state(false);

  const teams      = $derived(issuesStore.filterOptions?.teams      ?? []);
  const statuses   = $derived(issuesStore.filterOptions?.statuses   ?? []);
  const labels     = $derived(issuesStore.filterOptions?.labels     ?? []);
  const projects   = $derived(issuesStore.filterOptions?.projects   ?? []);
  const milestones = $derived(issuesStore.filterOptions?.milestones ?? []);
  const me         = $derived(issuesStore.filterOptions?.me ?? null);

  const selectedTeam      = $derived(teams.find(t => t.id === teamId));
  const selectedStatus    = $derived(statuses.find(s => s.id === statusId));
  const selectedProject   = $derived(projects.find(p => p.id === projectId));
  const selectedMilestone = $derived(milestones.find(m => m.id === milestoneId));
  const selectedLabels    = $derived(labels.filter(l => labelIds.includes(l.id)));

  const statusGroups = $derived((() => {
    const order = ['backlog', 'unstarted', 'started', 'completed', 'cancelled'];
    const groups: Record<string, IssueStatus[]> = {};
    for (const s of statuses) {
      if (!groups[s.statusType]) groups[s.statusType] = [];
      groups[s.statusType].push(s);
    }
    return order.filter(o => groups[o]).map(o => ({ type: o, items: groups[o] }));
  })());

  function closeAllDrops() {
    teamDropOpen = false; statusDropOpen = false; priorityDropOpen = false;
    labelsDropOpen = false; projectDropOpen = false; milestoneDropOpen = false;
  }

  function toggleLabel(id: string) {
    labelIds = labelIds.includes(id) ? labelIds.filter(i => i !== id) : [...labelIds, id];
  }

  function labelChipStyle(color: string): string {
    const hex = color.startsWith('#') ? color : `#${color}`;
    if (hex.length < 7) return '';
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if (lum < 0.1) return `background:rgba(160,160,160,0.12);color:var(--text-secondary);border:1px solid rgba(160,160,160,0.25)`;
    return `background:${hex}22;color:${hex};border:1px solid ${hex}55`;
  }

  function statusIcon(statusType: string, color: string): string {
    const c = color || '#6b7280';
    const sw = '1.8';
    if (statusType === 'completed')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6.5" fill="${c}"/><polyline points="4.5,7.5 6.5,9.5 10.5,5" fill="none" stroke="white" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    if (statusType === 'started')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><path d="M7.5,1.5 A6,6 0 0,1 7.5,13.5 L7.5,7.5 Z" fill="${c}"/></svg>`;
    if (statusType === 'cancelled')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><line x1="5" y1="5" x2="10" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/><line x1="10" y1="5" x2="5" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/></svg>`;
    if (statusType === 'unstarted')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/></svg>`;
    return `<svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}" stroke-dasharray="3.5 2.5"/></svg>`;
  }

  const PRIORITIES = [
    { value: undefined, label: 'No priority', icon: '—' },
    { value: 1,         label: 'Urgent',      icon: '🔴' },
    { value: 2,         label: 'High',        icon: '🟠' },
    { value: 3,         label: 'Medium',      icon: '🟡' },
    { value: 4,         label: 'Low',         icon: '🔵' },
  ];

  async function submit() {
    if (!title.trim() || !teamId) return;
    saving = true; error = '';
    try {
      await issuesStore.createIssue({
        title:       title.trim(),
        description: description.trim() || undefined,
        teamId,
        statusId:    statusId    || undefined,
        projectId:   projectId   || undefined,
        milestoneId: milestoneId || undefined,
        assigneeId:  assigneeMe && me ? me.id : undefined,
        priority,
        labelIds:    labelIds.length ? labelIds : undefined,
        dueDate:     dueDate  || undefined,
        estimate:    estimate != null ? estimate : undefined,
      });
      uiStore.showToast('Issue created', 'success');
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal {onClose} width="700px" padBody={false} ariaLabel="Create Issue">
  {#snippet header()}
    <ModalHeader title="New Issue" {onClose} />
  {/snippet}

  <!-- Two-column body -->
  <div class="modal-columns">

    <!-- Left: title + description -->
    <div class="col-main">
      <input
        class="title-input"
        type="text"
        placeholder="Issue title…"
        bind:value={title}
        bind:this={titleEl}
        onkeydown={(e) => e.key === 'Enter' && !e.shiftKey && submit()}
      />
      <textarea
        class="desc-input"
        placeholder="Add description… (optional)"
        bind:value={description}
      ></textarea>
    </div>

    <!-- Right: properties sidebar -->
    <div class="col-props">

      <!-- Team (only when multiple) -->
      {#if teams.length > 1}
        <div class="prop-row">
          <span class="prop-label">Team</span>
          <div class="prop-drop-wrap">
            <button class="prop-btn" class:prop-active={!!teamId}
              onclick={() => { closeAllDrops(); teamDropOpen = true; }}>
              <span class="prop-val">{selectedTeam?.name ?? '—'}</span>
              <ChevronDown size={10} />
            </button>
            {#if teamDropOpen}
              <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (teamDropOpen = false)}></button>
              <div class="prop-drop">
                {#each teams as t}
                  <button class="drop-item" class:drop-sel={t.id === teamId}
                    onclick={() => { teamId = t.id; teamDropOpen = false; }}>
                    <span class="team-key">{t.key}</span>{t.name}
                    {#if t.id === teamId}<Check size={11} class="drop-check" />{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Status -->
      <div class="prop-row">
        <span class="prop-label">Status</span>
        <div class="prop-drop-wrap">
          <button class="prop-btn" class:prop-active={!!statusId}
            onclick={() => { closeAllDrops(); statusDropOpen = true; }}>
            {#if selectedStatus}
              <span class="status-icon-wrap">{@html statusIcon(selectedStatus.statusType, selectedStatus.color)}</span>
            {/if}
            <span class="prop-val">{selectedStatus?.name ?? 'Default'}</span>
            <ChevronDown size={10} />
          </button>
          {#if statusDropOpen}
            <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (statusDropOpen = false)}></button>
            <div class="prop-drop prop-drop-tall">
              <button class="drop-item" onclick={() => { statusId = ''; statusDropOpen = false; }}>
                — Default {!statusId ? '✓' : ''}
              </button>
              {#each statusGroups as grp}
                <div class="drop-group">{grp.type}</div>
                {#each grp.items as st}
                  <button class="drop-item" class:drop-sel={st.id === statusId}
                    onclick={() => { statusId = st.id; statusDropOpen = false; }}>
                    <span class="status-icon-wrap">{@html statusIcon(st.statusType, st.color)}</span>
                    {st.name}
                    {#if st.id === statusId}<Check size={11} class="drop-check" />{/if}
                  </button>
                {/each}
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- Priority -->
      <div class="prop-row">
        <span class="prop-label">Priority</span>
        <div class="prop-drop-wrap">
          <button class="prop-btn" class:prop-active={priority !== undefined}
            onclick={() => { closeAllDrops(); priorityDropOpen = true; }}>
            {#if priority !== undefined}
              <span class="prio-icon">{PRIORITIES.find(p => p.value === priority)?.icon}</span>
            {/if}
            <span class="prop-val">{PRIORITIES.find(p => p.value === priority)?.label ?? 'No priority'}</span>
            <ChevronDown size={10} />
          </button>
          {#if priorityDropOpen}
            <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (priorityDropOpen = false)}></button>
            <div class="prop-drop">
              {#each PRIORITIES as p}
                <button class="drop-item" class:drop-sel={p.value === priority}
                  onclick={() => { priority = p.value; priorityDropOpen = false; }}>
                  <span class="prio-icon">{p.icon}</span>{p.label}
                  {#if p.value === priority}<Check size={11} class="drop-check" />{/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- Project -->
      {#if projects.length > 0}
        <div class="prop-row">
          <span class="prop-label">Project</span>
          <div class="prop-drop-wrap">
            <button class="prop-btn" class:prop-active={!!projectId}
              onclick={() => { closeAllDrops(); projectDropOpen = true; }}>
              {#if selectedProject?.color}
                <span class="color-dot" style="background:{selectedProject.color}"></span>
              {/if}
              <span class="prop-val">{selectedProject?.name ?? '—'}</span>
              <ChevronDown size={10} />
            </button>
            {#if projectDropOpen}
              <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (projectDropOpen = false)}></button>
              <div class="prop-drop">
                <button class="drop-item" onclick={() => { projectId = ''; projectDropOpen = false; }}>
                  — No project {!projectId ? '✓' : ''}
                </button>
                {#each projects as proj}
                  <button class="drop-item" class:drop-sel={proj.id === projectId}
                    onclick={() => { projectId = proj.id; projectDropOpen = false; }}>
                    {#if proj.color}<span class="color-dot" style="background:{proj.color}"></span>{/if}
                    {proj.name}
                    {#if proj.id === projectId}<Check size={11} class="drop-check" />{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Milestone -->
      {#if milestones.length > 0}
        <div class="prop-row">
          <span class="prop-label">Milestone</span>
          <div class="prop-drop-wrap">
            <button class="prop-btn" class:prop-active={!!milestoneId}
              onclick={() => { closeAllDrops(); milestoneDropOpen = true; }}>
              <span class="prop-val">{selectedMilestone?.name ?? '—'}</span>
              <ChevronDown size={10} />
            </button>
            {#if milestoneDropOpen}
              <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (milestoneDropOpen = false)}></button>
              <div class="prop-drop">
                <button class="drop-item" onclick={() => { milestoneId = ''; milestoneDropOpen = false; }}>
                  — No milestone {!milestoneId ? '✓' : ''}
                </button>
                {#each milestones as ms}
                  <button class="drop-item" class:drop-sel={ms.id === milestoneId}
                    onclick={() => { milestoneId = ms.id; milestoneDropOpen = false; }}>
                    {ms.name}
                    {#if ms.targetDate}<span class="ms-date">{ms.targetDate}</span>{/if}
                    {#if ms.id === milestoneId}<Check size={11} class="drop-check" />{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Assignee -->
      {#if me}
        <div class="prop-row">
          <span class="prop-label">Assignee</span>
          <button class="prop-btn prop-toggle" class:prop-active={assigneeMe}
            onclick={() => { assigneeMe = !assigneeMe; }}>
            <User size={11} />
            <span class="prop-val">{assigneeMe ? me.displayName : 'Unassigned'}</span>
          </button>
        </div>
      {/if}

      <!-- Labels -->
      {#if labels.length > 0}
        <div class="prop-row">
          <span class="prop-label">Labels</span>
          <div class="prop-drop-wrap">
            <button class="prop-btn" class:prop-active={labelIds.length > 0}
              onclick={() => { closeAllDrops(); labelsDropOpen = true; }}>
              <Tag size={11} />
              <span class="prop-val">
                {labelIds.length > 0 ? `${labelIds.length} label${labelIds.length > 1 ? 's' : ''}` : '—'}
              </span>
              <ChevronDown size={10} />
            </button>
            {#if labelsDropOpen}
              <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => (labelsDropOpen = false)}></button>
              <div class="prop-drop prop-drop-tall">
                {#each labels as lbl}
                  <button class="drop-item" class:drop-sel={labelIds.includes(lbl.id)}
                    onclick={() => toggleLabel(lbl.id)}>
                    <span class="label-dot" style="background:{lbl.color.startsWith('#') ? lbl.color : '#'+lbl.color}"></span>
                    {lbl.name}
                    {#if labelIds.includes(lbl.id)}<Check size={11} class="drop-check" />{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>
        {#if selectedLabels.length > 0}
          <div class="prop-label-chips">
            {#each selectedLabels as lbl}
              <span class="label-chip" style={labelChipStyle(lbl.color)}>
                {lbl.name}
                <button class="chip-remove" onclick={() => toggleLabel(lbl.id)}><X size={9} /></button>
              </span>
            {/each}
          </div>
        {/if}
      {/if}

      <div class="prop-sep"></div>

      <!-- Due date -->
      <div class="prop-row">
        <span class="prop-label">Due date</span>
        <input class="prop-date-input" type="date" bind:value={dueDate} />
      </div>

      <!-- Estimate -->
      <div class="prop-row">
        <span class="prop-label">Estimate</span>
        <NumberStepper
          bind:value={estimate}
          min={0}
          step={0.5}
          ariaLabel="Estimate (story points)"
        />
      </div>

    </div>
  </div>

  {#if error}
    <div class="modal-error"><AlertCircle size={12} />{error}</div>
  {/if}

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    <Button variant="primary" onclick={submit} disabled={saving || !title.trim() || !teamId} loading={saving}>
      {saving ? 'Creating…' : 'Create Issue'}
    </Button>
  {/snippet}
</Modal>

<style>
  /* ── Two-column layout ── */
  .modal-columns {
    display: flex;
    height: 100%;
    min-height: 0;
  }

  /* Left column */
  .col-main {
    flex: 1; min-width: 0;
    padding: 16px;
    display: flex; flex-direction: column; gap: 10px;
    overflow-y: auto;
  }

  .title-input {
    width: 100%; padding: 10px 12px;
    font-size: 15px; font-weight: 500;
    font-family: var(--font-ui-sans);
    background: transparent; border: none; border-bottom: 1px solid var(--border-subtle);
    border-radius: 0;
    color: var(--text-primary); outline: none;
    transition: border-color var(--transition-fast);
    box-sizing: border-box;
  }
  .title-input:focus { border-bottom-color: var(--accent); }
  .title-input::placeholder { color: var(--text-muted); font-weight: 400; }

  .desc-input {
    flex: 1; width: 100%; min-height: 140px; padding: 8px 12px;
    font-size: 12px; line-height: 1.65;
    font-family: var(--font-ui-sans);
    background: transparent; border: none;
    color: var(--text-secondary); outline: none;
    resize: none;
    box-sizing: border-box;
  }
  .desc-input::placeholder { color: var(--text-muted); }

  /* Right column */
  .col-props {
    width: 210px; flex-shrink: 0;
    background: var(--bg-elevated);
    border-left: 1px solid var(--border-subtle);
    padding: 10px 6px 14px;
    display: flex; flex-direction: column; gap: 1px;
    overflow-y: auto;
    overflow-x: visible;
    position: relative;
  }

  /* Property rows */
  .prop-row {
    display: flex; align-items: center; gap: 4px;
    padding: 2px 4px; min-height: 30px;
  }
  .prop-label {
    font-size: 11px; color: var(--text-muted);
    width: 60px; flex-shrink: 0; user-select: none;
  }

  .prop-drop-wrap { flex: 1; min-width: 0; position: relative; }

  .prop-btn {
    display: flex; align-items: center; gap: 4px;
    width: 100%; padding: 4px 6px;
    font-size: 11px; font-family: var(--font-ui-sans);
    background: transparent; border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer; text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .prop-btn:hover { background: var(--bg-hover); border-color: var(--border-subtle); color: var(--text-secondary); }
  .prop-active { color: var(--text-primary) !important; }
  .prop-toggle { flex: 1; }

  .prop-val { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .status-icon-wrap { display: flex; align-items: center; flex-shrink: 0; line-height: 0; }
  .color-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .prio-icon { font-size: 11px; flex-shrink: 0; }
  .label-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

  /* Dropdown panel */
  .drop-backdrop { position: fixed; inset: 0; z-index: var(--z-backdrop); background: transparent; border: none; padding: 0; cursor: default; }
  .prop-drop {
    position: absolute; top: calc(100% + 2px); right: 0; z-index: var(--z-modal);
    min-width: 170px; max-height: 230px; overflow-y: auto;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); padding: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    animation: dropIn var(--anim-dur-fast) cubic-bezier(0.16,1,0.3,1);
  }
  .prop-drop-tall { max-height: 300px; }
  @keyframes dropIn { from { opacity:0; transform:translateY(-4px); } to { opacity:1; transform:none; } }

  .drop-group {
    padding: 5px 8px 2px;
    font-size: 9px; font-weight: 600; letter-spacing: 0.5px;
    text-transform: uppercase; color: var(--text-muted);
  }
  .drop-item {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 5px 8px; text-align: left;
    font-size: 11px; font-family: var(--font-ui-sans);
    color: var(--text-primary); background: transparent; border: none;
    border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast);
  }
  .drop-item:hover { background: var(--bg-hover); }
  .drop-sel { color: var(--accent); }
  :global(.drop-check) { margin-left: auto; color: var(--accent); flex-shrink: 0; }

  .team-key {
    font-family: var(--font-code); font-size: 9px;
    color: var(--text-muted); background: var(--bg-elevated);
    padding: 0 3px; border-radius: 2px; flex-shrink: 0;
    margin-right: 2px;
  }
  .ms-date {
    margin-left: auto; font-size: 10px;
    color: var(--text-muted); font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  /* Label chips */
  .prop-label-chips {
    display: flex; flex-wrap: wrap; gap: 4px;
    padding: 2px 4px 4px 68px;
  }
  .label-chip {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px; font-weight: 500;
    padding: 2px 3px 2px 6px; border-radius: var(--radius-sm);
  }
  .chip-remove {
    display: flex; align-items: center; justify-content: center;
    width: 13px; height: 13px; border: none; background: transparent;
    cursor: pointer; color: inherit; opacity: 0.65; border-radius: 2px; padding: 0;
  }
  .chip-remove:hover { opacity: 1; }

  /* Separator */
  .prop-sep {
    height: 1px; background: var(--border-subtle);
    margin: 6px 4px;
  }

  /* Date input — the numeric Estimate field has been promoted to the
     shared <NumberStepper> widget, leaving this rule scoped to the
     date picker alone. The flat ghost-on-hover treatment matches the
     rest of the modal's prop rows. */
  .prop-date-input {
    flex: 1; width: 100%; padding: 4px 6px;
    font-size: 11px; font-family: var(--font-ui-sans);
    background: transparent; border: 1px solid transparent;
    border-radius: var(--radius-sm); color: var(--text-primary); outline: none;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    box-sizing: border-box;
    color-scheme: dark;
  }
  .prop-date-input:hover {
    background: var(--bg-hover); border-color: var(--border-subtle);
  }
  .prop-date-input:focus {
    background: var(--bg-hover); border-color: var(--accent);
  }

  /* Error bar */
  .modal-error {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 14px; font-size: 11px;
    color: var(--status-error, #f87171);
    background: rgba(248,113,113,0.08);
    border-top: 1px solid rgba(248,113,113,0.2);
    flex-shrink: 0;
  }

</style>
