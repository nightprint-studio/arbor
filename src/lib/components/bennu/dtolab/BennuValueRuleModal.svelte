<script lang="ts">
  /**
   * One test value: the fields it answers, by name or by constraint, and what it gives them.
   *
   * Names and constraints are typed comma-separated — the way the settings list shows them, one line a
   * rule — so what is read and what is written look the same.
   */
  import { untrack } from 'svelte';
  import { Beaker } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuValueRulesStore as values } from '$lib/stores/bennu/value-rules.svelte';
  import type { DtoLabValueRule } from '$lib/ipc/bennu/dtolab';

  let {
    rule,
    previous,
    onClose,
  }: {
    /** The rule edited or copied; `null` for a blank one. */
    rule: DtoLabValueRule | null;
    /** The name it is saved over; `null` adds a rule. */
    previous: string | null;
    onClose: () => void;
  } = $props();

  const parse = (text: string) => text.split(',').map((s) => s.trim()).filter(Boolean);

  // Seeded once from the rule opened; the dialog owns the fields from then on.
  const initial = untrack(() => rule);
  let name = $state(initial?.name ?? '');
  let fields = $state((initial?.fields ?? []).join(', '));
  let constraints = $state((initial?.constraints ?? []).join(', '));
  let value = $state(initial?.value ?? '');
  let java = $state(initial?.java ?? '');
  let invalid = $state(initial?.invalid ?? '');
  let invalidJava = $state(initial?.invalid_java ?? '');
  let saving = $state(false);

  const trimmedName = $derived(name.trim());
  const nameError = $derived(
    !trimmedName
      ? null
      : !/^\w+$/.test(trimmedName)
        ? 'Letters, digits and “_”.'
        : values.rules.some((r) => r.name === trimmedName && r.name !== previous)
          ? `There is already a value called “${trimmedName}”.`
          : null,
  );
  const answersSomething = $derived(
    [...parse(fields), ...parse(constraints)].some((p) => p.replace(/[*@]/g, '').trim() !== ''),
  );
  const fieldsError = $derived(trimmedName && !answersSomething ? 'Give it a field name or a constraint to answer.' : null);
  const replacesBuiltin = $derived(values.builtins.some((b) => b.name === trimmedName));
  const valid = $derived(!!trimmedName && !nameError && answersSomething && value !== '');

  async function save() {
    if (!valid || saving) return;
    saving = true;
    const saved = await values.upsert(
      {
        name: trimmedName,
        fields: parse(fields),
        constraints: parse(constraints).map((c) => c.replace(/^@/, '')),
        value,
        java: java.trim() || null,
        invalid: invalid === '' ? null : invalid,
        invalid_java: invalidJava.trim() || null,
      },
      previous,
    );
    saving = false;
    if (saved) onClose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save();
    }
  }
</script>

<Modal {onClose} width="640px" height="auto" ariaLabel="Test value">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Beaker size={14} />
      <span class="modal-title">{previous === null ? 'New test value' : 'Edit test value'}</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    <FormField
      label="Name"
      hint={replacesBuiltin ? 'Named like a built-in, so it replaces that one.' : 'What a template reads as value.name.'}
      error={nameError}
    >
      <Input bind:value={name} placeholder="codice_fiscale" autofocus />
    </FormField>
    <div class="grid">
      <FormField label="Field names" hint="Comma-separated. Case, _ and - are ignored; * at either end matches any prefix or suffix." error={fieldsError}>
        <Input bind:value={fields} placeholder="codiceFiscale, *CodiceFiscale, cf" />
      </FormField>
      <FormField label="Constraints" hint="Simple names, for a field carrying one — tried before the names.">
        <Input bind:value={constraints} placeholder="CodiceFiscale" />
      </FormField>
      <FormField label="Value" hint="Fixed. Read as a number or a boolean for a field of that type.">
        <Input bind:value={value} placeholder="RSSMRA80A01H501U" />
      </FormField>
      <FormField label="Java expression" optionalText="(optional)" hint="Written in the test instead of the value — which is still what the JVM checks.">
        <Input bind:value={java} placeholder="TestData.codiceFiscale()" />
      </FormField>
      <FormField label="Invalid value" optionalText="(optional)" hint="The case written for a constraint named above that Bennu does not know.">
        <Input bind:value={invalid} placeholder="RSSMRA80A01H501X" />
      </FormField>
      <FormField label="Invalid Java expression" optionalText="(optional)">
        <Input bind:value={invalidJava} placeholder="TestData.invalidCodiceFiscale()" />
      </FormField>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} disabled={!valid} onclick={() => void save()}>Save</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
</style>
