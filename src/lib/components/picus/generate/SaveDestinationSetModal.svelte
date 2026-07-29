<script lang="ts">
  /**
   * Name the current destinations, and keep them with the repository.
   *
   * Mounted by the shell rather than by the panel that offers it: naming a set is
   * reachable from the sidebar, from the Destinations card and from the palette,
   * and a dialog owned by one of the three would only work from that one.
   */
  import { BookmarkPlus, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import ChipBar from '$lib/components/shared/ui/ChipBar.svelte';
  import { destinationSetsStore } from '$lib/stores/picus/destination-sets.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';

  interface Props {
    onClose: () => void;
    /** Opens on an existing set — how one is replaced. `''` is a new set. */
    initialName?: string;
  }

  let { onClose, initialName = '' }: Props = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(initialName);

  const replacing = $derived(!!name.trim() && destinationSetsStore.has(name));
  /** What an update destination would be stored *instead of* — the point of the note. */
  const anUpdateFile = $derived(
    dmlStore.targets.find((t) => t.role === 'update')?.file.split('/').pop() ?? 'a file name',
  );

  async function save() {
    if (!name.trim() || destinationSetsStore.saving) return;
    if (await destinationSetsStore.save(name)) onClose();
  }
</script>

<Modal {onClose} width="560px" height="360px" ariaLabel="Save these destinations">
  {#snippet header()}
    <ModalHeader {onClose}>
      <BookmarkPlus size={14} />
      <span class="ds-title">Save these destinations</span>
    </ModalHeader>
  {/snippet}

  <div class="ds-body">
    <Input
      value={name}
      autofocus
      placeholder="Release"
      ariaLabel="Name for this set"
      oninput={(v) => (name = String(v))}
      onkeydown={(e) => { if (e.key === 'Enter') void save(); }}
    />
    {#if destinationSetsStore.sets.length}
      <!-- Overwriting used to mean retyping an existing name character for
           character, with nothing anywhere saying that was even possible. One
           click fills the field; the button below then reads Replace. -->
      <div class="ds-existing">
        <span class="ds-existing-label">Replace</span>
        <ChipBar
          items={destinationSetsStore.sets.map((s) => ({
            id: s.name,
            label: s.name,
            count: s.destinations.length,
            tooltip: `Overwrite ${s.name} with the armed destinations`,
          }))}
          selected={replacing ? name.trim() : ''}
          size="sm"
          onSelect={(id) => (name = String(id))}
        />
      </div>
    {/if}
    <p class="ds-note">
      Saved with the repository, so a colleague opening the same folder finds it too.
      An <b>update</b> destination is stored as its folder rather than as
      <code>{anUpdateFile}</code> — so applying the set next release picks up that release's
      file, and the version guard's bounds with it.
    </p>
    {#if replacing}
      <p class="ds-replace">
        <TriangleAlert size={12} />
        A set called <b>{name.trim()}</b> already exists and will be replaced.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <span class="ds-spacer"></span>
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!name.trim() || destinationSetsStore.saving}
        onclick={() => void save()}
      >
        {replacing ? 'Replace' : 'Save'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .ds-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .ds-body { display: flex; flex-direction: column; gap: 10px; }
  .ds-existing { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .ds-existing-label {
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .ds-note { font-size: 11.5px; line-height: 1.55; color: var(--text-muted); }
  .ds-note code { font-family: var(--font-code); font-size: 11px; }
  .ds-replace {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--warning);
  }
  .ds-spacer { flex: 1; }
</style>
