<script lang="ts">
  /**
   * The skills manager — which of the skills on disk the assistant is allowed to know about.
   *
   * A skill is a folder with a `SKILL.md` (the format Claude Code uses) found in the project's
   * `.bennu/skills` or `.claude/skills`, or in the user's own folders. Nothing reaches the model until it
   * is ticked here: the assistant gets only a one-line index of the enabled ones and loads a body when a
   * task calls for it. Folders that could not be used are listed with the reason, so a skill that "does
   * not show up" explains itself.
   *
   * Keyboard: the search field has the focus. ↑/↓ move through the list, Enter (or Space, when the search
   * is empty) switches the skill under the cursor, →/← open and close its preview, Esc closes.
   */
  import { WandSparkles, RefreshCw, ChevronRight, ChevronDown, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { SkillInfo } from '$lib/ipc/bennu/assistant';
  import { formatSkillSize, matchSkills, sourceLabel } from './assistant-skills';

  const store = bennuAssistantStore;

  let query = $state('');
  let cursor = $state(0);
  let expanded = $state<string | null>(null);
  let list = $state<HTMLElement | null>(null);

  const listing = $derived(store.skillListing);
  const shown = $derived<SkillInfo[]>(matchSkills(listing?.skills ?? [], query));
  const invalid = $derived(listing?.invalid ?? []);
  const counts = $derived(store.skillCounts);
  const current = $derived(shown[Math.min(cursor, shown.length - 1)] ?? null);

  // A narrower list starts again from the top.
  $effect(() => {
    void query;
    cursor = 0;
  });

  // The cursor row stays in view while the arrows walk the list.
  $effect(() => {
    void cursor;
    list?.querySelector<HTMLElement>('[data-cursor="true"]')?.scrollIntoView({ block: 'nearest' });
  });

  function toggle(skill: SkillInfo) {
    store.setSkill(skill.id, !store.skillOn(skill.id));
  }

  /** Enable or disable what the search is showing — all of it when the search is empty. */
  function setShown(on: boolean) {
    store.setSkills(shown.map((s) => s.id), on);
  }

  function onKeydown(e: KeyboardEvent) {
    const n = shown.length;
    if (e.key === 'ArrowDown' && n) { e.preventDefault(); cursor = Math.min(cursor + 1, n - 1); }
    else if (e.key === 'ArrowUp' && n) { e.preventDefault(); cursor = Math.max(cursor - 1, 0); }
    else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); store.closeSkills(); }
    else if (e.key === 'Enter' && current) { e.preventDefault(); toggle(current); }
    else if (e.key === ' ' && !query && current) { e.preventDefault(); toggle(current); }
    else if (e.key === 'ArrowRight' && current && !query) { e.preventDefault(); expanded = current.id; }
    else if (e.key === 'ArrowLeft' && current && !query) { e.preventDefault(); expanded = null; }
  }
</script>

{#if store.skillsOpen}
  <Modal onClose={() => store.closeSkills()} width="780px" height="620px" ariaLabel="Imber skills">
    {#snippet header()}
      <ModalHeader onClose={() => store.closeSkills()}>
        <WandSparkles size={14} />
        <span class="title">Skills</span>
        <Badge variant="count" size="sm" label={`${counts.on} of ${counts.total} enabled`} />
      </ModalHeader>
    {/snippet}

    <div class="body">
      <div class="bar">
        <div class="search">
          <SearchBar
            bind:query
            showRegex={false}
            showCounter={false}
            autofocus
            placeholder="Filter skills…"
            ariaLabel="Filter skills"
            onkeydown={onKeydown}
          />
        </div>
        <Button variant="secondary" size="sm" disabled={shown.length === 0} onclick={() => setShown(true)}>Enable all</Button>
        <Button variant="secondary" size="sm" disabled={shown.length === 0} onclick={() => setShown(false)}>Disable all</Button>
        <IconButton tooltip="Look on disk again" size={26} disabled={store.skillsLoading} onclick={() => void store.refreshSkills()}>
          {#if store.skillsLoading}<Spinner />{:else}<RefreshCw size={13} />{/if}
        </IconButton>
      </div>

      <Alert variant="info" compact text="Only the skills you enable are known to Imber: it sees their name and one line, and loads a skill when a task calls for it. Nothing in a skill is run." />

      <div class="scroll" bind:this={list}>
        {#if store.skillsLoading && !listing}
          <EmptyState message="Looking for skills…" />
        {:else if shown.length === 0 && invalid.length === 0}
          <EmptyState
            message={query ? 'No skill matches.' : 'No skills found.'}
            description={query ? undefined : 'Put a folder with a SKILL.md in .bennu/skills or .claude/skills of the project, or in Bennu’s skills folder.'}
          />
        {:else}
          <RowList items={shown} key={(s) => s.id} dimmed={(s) => s.shadowed}>
            {#snippet row(skill, index)}
              <div class="skill" class:active={index === cursor} data-cursor={index === cursor} onfocusin={() => (cursor = index)} role="group" aria-label={skill.name}>
                <div class="head">
                  <Toggle size="sm" checked={store.skillOn(skill.id)} ariaLabel={`Enable ${skill.name}`} onchange={() => toggle(skill)} />
                  <span class="name">{skill.name}</span>
                  <Badge variant="tone" tone={skill.scope === 'project' ? 'accent' : 'neutral'} size="sm" label={sourceLabel(skill)} />
                  {#if skill.project}<Badge variant="chip" size="sm" label={skill.project} />{/if}
                  {#if skill.shadowed}<Badge variant="tone" tone="warning" size="sm" label="hidden by a project skill" />{/if}
                  {#if skill.truncated}<Badge variant="tone" tone="warning" size="sm" label="cut at 32 KB" />{/if}
                  <span class="grow"></span>
                  <span class="size">{formatSkillSize(skill.bytes)}</span>
                  <IconButton
                    tooltip={expanded === skill.id ? 'Hide the text' : 'Preview the text'}
                    size={22}
                    ariaExpanded={expanded === skill.id}
                    onclick={() => (expanded = expanded === skill.id ? null : skill.id)}
                  >
                    {#if expanded === skill.id}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
                  </IconButton>
                </div>
                <p class="desc">{skill.description}</p>
                {#if expanded === skill.id}
                  <div class="preview">
                    <MarkdownView text={skill.body} />
                    <p class="path">{skill.path}</p>
                  </div>
                {/if}
              </div>
            {/snippet}
          </RowList>

          {#if invalid.length > 0}
            <div class="invalid-head"><TriangleAlert size={12} /> Not usable ({invalid.length})</div>
            <RowList items={invalid} key={(i) => i.path}>
              {#snippet row(item)}
                <div class="bad rl-keep">
                  <Badge variant="tone" tone="error" size="sm" label="invalid" />
                  <span class="why">{item.reason}</span>
                  <span class="path">{item.path}</span>
                </div>
              {/snippet}
            </RowList>
          {/if}
        {/if}
      </div>
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="hint">↑↓ move · Enter switch · Space switch (empty search) · → ← preview · Esc close</span>
        <Button variant="primary" size="sm" onclick={() => store.closeSkills()}>Done</Button>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 10px; height: 100%; min-height: 0; padding: 12px 14px; }
  .bar { display: flex; align-items: center; gap: 8px; }
  .search { flex: 1; min-width: 0; }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; }

  .skill { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; padding: 4px 8px; border-radius: var(--radius-sm); }
  .skill.active { background: var(--bg-hover); box-shadow: inset 2px 0 0 var(--accent); }
  .head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .name { font-family: var(--font-code); font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .grow { flex: 1; }
  .size { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .desc { margin: 0 0 0 40px; font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.45; }
  .preview {
    margin: 6px 0 2px 40px; padding: 8px 12px; max-height: 260px; overflow-y: auto;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
  .path { margin: 6px 0 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); overflow-wrap: anywhere; }

  .invalid-head { display: flex; align-items: center; gap: 6px; margin: 14px 0 4px; font-size: var(--font-size-xs); font-weight: 600; color: var(--warning); }
  .bad { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 8px; flex: 1; min-width: 0; padding: 2px 8px; }
  .why { font-size: var(--font-size-xs); color: var(--text-primary); }
  .bad .path { flex-basis: 100%; margin: 0; }

  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
