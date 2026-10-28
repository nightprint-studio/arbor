<script lang="ts">
  /**
   * BennuAddParameterModal — the *Add parameter* refactor with a per-file preview.
   *
   * Opens off the caret context the editor captured (`bennuRefactorStore.addParameterCtx`): the
   * method is found from the caret, on its declaration or on a call. The user writes the new
   * parameter's type and name, the value every existing call passes for it (empty means the type's
   * default — `0`, `false`, `null`), and where it goes. Each change re-plans through
   * `bennu_add_parameter_plan` (debounced) and the edits — the declaration, every override, every
   * call — are listed per file by the same preview a rename shows. On confirm the whole set goes to
   * `projectStore.applyEdits`, the one write path that checks each edit against the text it was
   * planned against and treats the set as a single bulk edit.
   *
   * Keyboard-first: the type field auto-focuses (via <Modal>), Tab walks type → name → value →
   * position, Ctrl/Cmd+Enter applies, Esc cancels.
   */
  import { ListPlus } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuRefactorPreview from './BennuRefactorPreview.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { addParameterPlan, type AddParameterPreview } from '$lib/ipc/bennu/refactor';

  let { onClose }: { onClose: () => void } = $props();

  const ctx = $derived(bennuRefactorStore.addParameterCtx);

  let typeText = $state('');
  let name = $state('');
  let value = $state('');
  /** `'end'`, or the index of the parameter the new one goes before. */
  let position = $state<string | number>('end');
  let plan = $state<AddParameterPreview | null>(null);
  let planning = $state(false);
  let applying = $state(false);

  const JAVA_IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
  // A loose check — the compiler is the judge of the type — that only keeps the plan from being
  // asked about text that is plainly not one.
  const JAVA_TYPE = /^[A-Za-z_$][\w$.]*(<[\w$.,<>?\s&\[\]]*>)?(\[\])*$/;
  const valid = $derived(JAVA_TYPE.test(typeText.trim()) && JAVA_IDENT.test(name.trim()));
  const preview = $derived(plan?.preview ?? null);
  const canApply = $derived(
    valid && !!preview && preview.total_edits > 0 && !preview.blocked && !applying,
  );

  const positions = $derived([
    { value: 'end', label: 'Last' },
    ...(plan?.parameters ?? []).map((p, i) => ({ value: i, label: `Before ${p}` })),
  ]);

  // Re-plan (debounced) whenever anything the plan depends on changes. Asked even while the input is
  // still invalid: the answer carries the method's parameters, which is what fills the position list
  // before the user has typed anything.
  let planTimer: ReturnType<typeof setTimeout> | undefined;
  let planSeq = 0;
  $effect(() => {
    const c = ctx;
    const request = {
      type_text: typeText.trim(),
      name: name.trim(),
      default_value: value.trim() || undefined,
      before: typeof position === 'number' ? position : undefined,
    };
    if (planTimer) clearTimeout(planTimer);
    if (!c) { plan = null; planning = false; return; }
    planning = true;
    const seq = ++planSeq;
    planTimer = setTimeout(() => {
      void addParameterPlan({ file: c.file, source: c.source, offset: c.offset, ...request })
        .then((p) => { if (seq === planSeq) { plan = p; planning = false; } })
        .catch(() => { if (seq === planSeq) { plan = null; planning = false; } });
    }, 250);
  });

  async function apply() {
    const p = preview;
    if (!p || !canApply) return;
    applying = true;
    try {
      // Through the store, not a loop of our own: that is the one write path that verifies each
      // edit still matches the text it was computed against, and that treats the whole set as ONE
      // bulk edit — a method called from hundreds of places must not schedule hundreds of
      // whole-project re-validations behind itself.
      const failed = await projectStore.applyEdits(p.files.flatMap((f) => f.edits));
      const summary = `${p.total_edits} edit(s) in ${p.files.length} file(s)`;
      if (failed) {
        toastStore.show(`Added, but ${failed} file(s) could not be written · ${summary}`, 'error');
      } else {
        toastStore.show(`Added parameter “${typeText.trim()} ${name.trim()}” · ${summary}`, 'success');
      }
      bennuRefactorStore.closeAddParameter();
    } catch {
      toastStore.show('Add parameter failed', 'error');
    } finally {
      applying = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void apply();
    }
  }
</script>

<Modal {onClose} width="700px" height="600px" padBody={false} ariaLabel="Add parameter">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ListPlus size={14} />
      <span class="modal-title">Add parameter</span>
      {#if ctx}<span class="hdr-method">{ctx.method}()</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    {#if !ctx}
      <EmptyState message="Place the caret on a method, then run Add parameter." />
    {:else}
      <div class="fields">
        <FormField label="Type">
          <Input bind:value={typeText} placeholder="String" />
        </FormField>
        <FormField label="Name">
          <Input bind:value={name} placeholder="label" />
        </FormField>
        <FormField label="Value in existing calls" hint="Empty passes the type's default: 0, false or null.">
          <Input bind:value placeholder="null" />
        </FormField>
        <FormField label="Position">
          <Select bind:value={position} options={positions} />
        </FormField>
      </div>

      <BennuRefactorPreview
        {preview}
        {planning}
        {valid}
        invalidMessage="Enter the parameter's type and a valid Java name."
        emptyMessage="No edits — the caret isn't on a method this project declares, or the index is still building."
      />
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        onclick={apply}
        disabled={!canApply}
        tooltip={{ content: 'Add the parameter', shortcut: 'Ctrl+Enter' }}
      >
        {applying ? 'Adding…' : 'Add parameter'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-method { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }

  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 14px;
    padding: 14px 16px 8px;
    flex-shrink: 0;
  }
</style>
