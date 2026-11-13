<script lang="ts">
  /**
   * Add or edit one external tool: a program on this machine Imber may run with `run_tool`.
   *
   * The program is picked with the Sitta explorer; its fixed arguments are one per line, because nothing here is
   * split like a shell line — what you write on a line is one argument. What is wrong with the tool comes from the
   * backend, from the same rules a run uses to decide whether it is offered, so the page cannot call valid a tool
   * the run would drop. Saving is allowed anyway (a tool can be finished later); it is simply not offered until
   * it is right. Ctrl/Cmd+Enter saves, Esc closes.
   */
  import { untrack } from 'svelte';
  import { FolderOpen, TerminalSquare } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { externalToolProblems } from '$lib/ipc/bennu/assistant';
  import type { ExternalTool, ExternalToolApproval } from '$lib/ipc/bennu/config';
  import {
    APPROVAL_OPTIONS, argsFromLines, blankTool, envFromLines, keysFromText, linesFromArgs, linesFromEnv, nameFromProgram,
  } from './external-tool-form';

  let { tool, onSave, onClose }: {
    /** The tool to edit; `null` adds a new one. */
    tool: ExternalTool | null;
    onSave: (tool: ExternalTool) => void;
    onClose: () => void;
  } = $props();

  // The form is a copy taken when the modal opens: closing without saving leaves the tool as it was.
  const start = untrack(() => tool ?? blankTool());
  let name = $state(start.name);
  let description = $state(start.description);
  let program = $state(start.program);
  let argsText = $state(linesFromArgs(start.args));
  let extraArgs = $state(start.extra_args);
  let envText = $state(linesFromEnv(start.env));
  let keysText = $state(start.env_keys.join(', '));
  let writes = $state(start.writes);
  let approval = $state<ExternalToolApproval>(start.approval);
  let timeout = $state<number | null>(start.timeout_s);
  let enabled = $state(start.enabled);
  let picking = $state(false);
  let nameInput = $state<HTMLElement | undefined>();

  const draft = $derived<ExternalTool>({
    name: name.trim(), description: description.trim(), program: program.trim(), args: argsFromLines(argsText), extra_args: extraArgs,
    env: envFromLines(envText), env_keys: keysFromText(keysText), writes, approval, timeout_s: Math.round(Number(timeout) || 0), enabled,
  });

  let problems = $state<string[]>([]);
  $effect(() => {
    const snapshot = draft;
    // Asked a moment after typing stops, not at every key.
    const t = setTimeout(() => {
      externalToolProblems([snapshot]).then((p) => (problems = p[0] ?? [])).catch(() => (problems = []));
    }, 250);
    return () => clearTimeout(t);
  });

  $effect(() => {
    nameInput?.querySelector('input')?.focus();
  });

  function pick(path: string) {
    program = path;
    if (!name.trim()) name = nameFromProgram(path);
    picking = false;
  }

  function save() {
    onSave(draft);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    }
  }
</script>

<Modal {onClose} width="680px" height="640px" ariaLabel={tool ? `Edit the external tool ${tool.name}` : 'Add an external tool'}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <TerminalSquare size={14} />
      <span class="title">{tool ? `External tool · ${tool.name}` : 'Add an external tool'}</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={keydown}>
    <FormRow stacked label="Program" description="The executable, as an absolute path. It runs directly — no shell, so nothing in the arguments is expanded or chained.">
      <div class="program">
        <Input bind:value={program} placeholder="/usr/local/bin/tool" ariaLabel="Program path" />
        <Button variant="secondary" size="sm" onclick={() => (picking = true)}><FolderOpen size={12} /> Choose…</Button>
      </div>
    </FormRow>
    <div class="pair">
      <FormRow stacked label="Name" description="What Imber calls it: lowercase, digits, - and _.">
        <div bind:this={nameInput}><Input bind:value={name} placeholder="lint" ariaLabel="Tool name" /></div>
      </FormRow>
      <FormRow stacked label="Time limit" description="Seconds before a call is stopped.">
        <NumberStepper bind:value={timeout} min={1} max={3600} step={10} ariaLabel="Time limit in seconds" />
      </FormRow>
    </div>
    <FormRow stacked label="What it does" description="Imber reads this to decide when to use it: say what it is for and what its arguments mean.">
      <Textarea framed bind:value={description} minRows={2} maxRows={5} placeholder="Checks a file against our style rules. Pass the file paths as arguments." ariaLabel="What the tool does" />
    </FormRow>
    <div class="pair">
      <FormRow stacked label="Fixed arguments" description="One per line, always passed first.">
        <Textarea framed bind:value={argsText} minRows={2} maxRows={5} placeholder="--format&#10;json" ariaLabel="Fixed arguments, one per line" />
      </FormRow>
      <FormRow stacked label="Fixed environment" description="KEY=value, one per line.">
        <Textarea framed bind:value={envText} minRows={2} maxRows={5} placeholder="RUST_LOG=info" ariaLabel="Fixed environment variables" />
      </FormRow>
    </div>
    <FormRow label="Imber may add arguments" description="After the fixed ones, one per item. Off: the tool always runs exactly as above.">
      <Toggle bind:checked={extraArgs} ariaLabel="Imber may add arguments" />
    </FormRow>
    <FormRow stacked label="Variables Imber may set" description="Separated by commas. Any other variable in a call is refused.">
      <Input bind:value={keysText} placeholder="LEVEL, TARGET" ariaLabel="Variables Imber may set" />
    </FormRow>
    <FormRow label="It changes things" description="Files, services or data, rather than only reading. In manual mode every call asks, whatever the approval below.">
      <Toggle bind:checked={writes} ariaLabel="The tool changes things" />
    </FormRow>
    <FormRow stacked label="Ask before a call">
      <RadioGroup value={approval} options={APPROVAL_OPTIONS} appearance="card" size="sm" block onchange={(v) => (approval = v as ExternalToolApproval)} />
    </FormRow>
    <FormRow label="Offered to Imber" description="Off keeps the tool here without offering it.">
      <Toggle bind:checked={enabled} ariaLabel="Offer this tool to Imber" />
    </FormRow>
    {#if problems.length}
      <Callout variant="warning" title="Not offered until this is fixed">
        <ul class="problems">{#each problems as p (p)}<li>{p}</li>{/each}</ul>
      </Callout>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button variant="primary" size="sm" onclick={save}>{tool ? 'Save' : 'Add'}</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="file"
    title="Choose the program"
    initialPath={program.trim() || undefined}
    onConfirm={pick}
    onCancel={() => (picking = false)}
  />
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; }
  .program { display: flex; gap: 8px; align-items: center; min-width: 0; }
  .program :global(input) { font-family: var(--font-code); }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .problems { margin: 0; padding-left: 16px; }
</style>
