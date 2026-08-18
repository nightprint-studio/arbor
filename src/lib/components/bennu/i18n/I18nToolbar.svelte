<script lang="ts">
  /**
   * The four constructs, as four buttons.
   *
   * ## Why buttons at all, in an editor
   *
   * Because the names are not in your head. `$red.bold{…}` is easy to type and impossible to type
   * *correctly* without knowing that the stylesheet calls it `red` and not `danger` — and getting it
   * wrong produces a file that is valid, renders, and has quietly lost its emphasis. The pickers turn
   * "what is this style called" from a trip to `styles.toml` into a list, which is the same reason
   * completion exists for anything else.
   *
   * Which is also why the style and glossary pickers offer **only what the project declares** while
   * the control and placeholder ones accept anything typed. That asymmetry is the model, not an
   * oversight: a style name that is not in `styles.toml` is a defect, and a control name is whatever
   * the engine implements — i18n knows the *form* of `~slow{…}` and nothing about its meaning, so a
   * closed list here would be an invented one.
   *
   * ## Keyboard
   *
   * Every menu is `searchable`, so it opens, you type, you press Enter. The two open-ended ones use
   * the typed text itself when it matches nothing — which is how a control the project has never used
   * gets written without leaving the keyboard.
   */
  import { Braces, BookMarked, Palette, Timer } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import type { StudioView } from '$lib/ipc/bennu/i18n';
  import { safeName, safeParam, type Insert } from './markup-edit';

  let {
    view,
    /** Whether the value can be written into at all — see `StudioView.content_start`. */
    writable,
    onInsert,
  }: {
    view: StudioView;
    writable: boolean;
    onInsert: (insert: Insert) => void;
  } = $props();

  const styleItems: DropdownItem[] = $derived(
    view.styles.map((s) => ({
      kind: 'item' as const,
      id: s.name,
      label: s.name,
      // What it looks like, in the row that offers it — the fields it sets, in the file's own words.
      // A style that sets none is legal and worth saying so: it will change nothing.
      meta: [s.weight, s.size, s.decoration, s.color].filter(Boolean).join(' · ') || 'sets nothing',
      onclick: () => onInsert({ what: 'style', names: [s.name] }),
    })),
  );

  const glossaryItems: DropdownItem[] = $derived(
    view.glossary.map((g) => ({
      kind: 'item' as const,
      id: g.key,
      label: g.key,
      subtitle: g.description || '',
      meta: g.name || '',
      onclick: () => onInsert({ what: 'glossary', key: g.key }),
    })),
  );

  /**
   * The placeholders worth offering: the ones the other languages use.
   *
   * Not the ones *this* value already has — those are already written, and re-inserting one is
   * almost never what the button is for. What it is for is the sentence you are translating from,
   * whose `{amount}` you now need somewhere in yours.
   */
  const paramNames: string[] = $derived.by(() => {
    const out: string[] = [];
    for (const s of view.siblings) {
      for (const p of s.params) if (!out.includes(p)) out.push(p);
    }
    for (const p of view.params) if (!out.includes(p)) out.push(p);
    return out;
  });

  /** A control insert, narrowed — the template reads `.name` and `.args` off it directly. */
  type ControlInsert = Extract<Insert, { what: 'control' }>;

  /** `sleep(0.8)` and `sleep 0.8` both mean the name `sleep` with the argument `0.8`. */
  function parseControl(typed: string): ControlInsert | null {
    const t = typed.trim();
    if (!t) return null;
    const call = t.match(/^([^(\s]+)\s*\(([^)]*)\)?\s*$/);
    if (call) return { what: 'control', name: call[1], args: call[2] };
    const spaced = t.match(/^(\S+)\s+(.+)$/);
    if (spaced) return { what: 'control', name: spaced[1], args: spaced[2] };
    return { what: 'control', name: t };
  }
</script>

<div class="tb" role="toolbar" tabindex="-1" aria-label="Insert markup">
  <!-- Styles: closed list. A name the stylesheet lacks is a defect, so it is not offered. -->
  <Dropdown
    items={styleItems}
    searchable
    searchPlaceholder="Style name…"
    emptyMessage={view.has_stylesheet
      ? 'No style matches.'
      : 'This project has no styles.toml, so there are no styles to name.'}
    position="fixed"
    direction="down"
    width="260px"
  >
    {#snippet trigger({ open, toggle })}
      <IconButton
        tooltip="Wrap the selection in a style — $red.bold{'{…}'}"
        size={24}
        active={open}
        disabled={!writable || !view.has_stylesheet}
        ariaHasPopup
        ariaExpanded={open}
        onclick={toggle}
      >
        <Palette size={13} />
      </IconButton>
    {/snippet}
  </Dropdown>

  <!-- Glossary: closed for the same reason. -->
  <Dropdown
    items={glossaryItems}
    searchable
    searchPlaceholder="Glossary key…"
    emptyMessage={view.has_glossary
      ? 'No entry matches.'
      : 'This project has no glossary.toml.'}
    position="fixed"
    direction="down"
    width="280px"
  >
    {#snippet trigger({ open, toggle })}
      <IconButton
        tooltip="Wrap the selection in a glossary reference — @potion{'{…}'}"
        size={24}
        active={open}
        disabled={!writable || !view.has_glossary}
        ariaHasPopup
        ariaExpanded={open}
        onclick={toggle}
      >
        <BookMarked size={13} />
      </IconButton>
    {/snippet}
  </Dropdown>

  <!-- Controls: open. The project's vocabulary first, then whatever you type. -->
  <Dropdown searchable searchPlaceholder="slow, or sleep(0.8)…" position="fixed" direction="down" width="280px">
    {#snippet trigger({ open, toggle })}
      <IconButton
        tooltip="Insert a control — ~slow{'{…}'} or ~sleep(0.8)"
        size={24}
        active={open}
        disabled={!writable}
        ariaHasPopup
        ariaExpanded={open}
        onclick={toggle}
      >
        <Timer size={13} />
      </IconButton>
    {/snippet}
    {#snippet children({ filter, close })}
      {@const typed = parseControl(filter)}
      {@const used = view.controls.filter((c) => c.includes(safeName(filter)))}
      {#if used.length}
        <div class="tb-group">used in this project</div>
        {#each used as name (name)}
          <button
            class="tb-item"
            type="button"
            onclick={() => { onInsert({ what: 'control', name }); close(); }}
          >
            <code>~{name}</code>
          </button>
        {/each}
      {/if}
      {#if typed && !used.includes(safeName(filter))}
        <div class="tb-group">write it out</div>
        <button
          class="tb-item"
          type="button"
          onclick={() => { onInsert(typed); close(); }}
        >
          <code>~{safeName(typed.name)}{typed.args ? `(${typed.args})` : ''}</code>
        </button>
      {/if}
      {#if !used.length && !typed}
        <div class="tb-empty">
          {view.controls.length
            ? 'No control matches.'
            : 'This project uses no controls yet — type a name to write one.'}
        </div>
      {/if}
    {/snippet}
  </Dropdown>

  <!-- Placeholders: open, and the offered set is what the OTHER languages pass. -->
  <Dropdown searchable searchPlaceholder="Parameter name…" position="fixed" direction="down" width="260px">
    {#snippet trigger({ open, toggle })}
      <IconButton
        tooltip="Insert a placeholder — {'{amount}'}"
        size={24}
        active={open}
        disabled={!writable}
        ariaHasPopup
        ariaExpanded={open}
        onclick={toggle}
      >
        <Braces size={13} />
      </IconButton>
    {/snippet}
    {#snippet children({ filter, close })}
      {@const typed = safeParam(filter)}
      {@const known = paramNames.filter((p) => p.includes(typed))}
      {#if known.length}
        <div class="tb-group">used by this label</div>
        {#each known as name (name)}
          <button
            class="tb-item"
            type="button"
            onclick={() => { onInsert({ what: 'placeholder', name }); close(); }}
          >
            <code>{'{'}{name}{'}'}</code>
            {#if view.params.includes(name)}<span class="tb-note">already here</span>{/if}
          </button>
        {/each}
      {/if}
      {#if typed && !known.includes(typed)}
        <div class="tb-group">write it out</div>
        <button
          class="tb-item"
          type="button"
          onclick={() => { onInsert({ what: 'placeholder', name: typed }); close(); }}
        >
          <code>{'{'}{typed}{'}'}</code>
        </button>
      {/if}
      {#if !known.length && !typed}
        <div class="tb-empty">
          {paramNames.length
            ? 'No parameter matches.'
            : 'This label takes no parameters yet — type a name to add one.'}
        </div>
      {/if}
    {/snippet}
  </Dropdown>
</div>

<style>
  .tb {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  /* The freeform menus render their own rows, so they carry the item styling the declarative
     `items` path would have given them. */
  .tb-group {
    padding: 5px 8px 2px;
    font-size: var(--font-size-3xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-disabled);
  }
  .tb-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: var(--font-size-xs);
    text-align: left;
    cursor: pointer;
  }
  .tb-item:hover { background: var(--bg-hover); }
  .tb-item code { font-family: var(--font-code); color: var(--accent); }
  .tb-note { margin-left: auto; color: var(--text-disabled); font-size: var(--font-size-3xs); }
  .tb-empty {
    padding: 8px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.4;
  }
</style>
