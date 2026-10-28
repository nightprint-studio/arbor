<script lang="ts">
  /**
   * How a connection signs in — a stored password, or one of the methods the backend registers
   * (Google IAM today). Data-driven from the method descriptors, so the next cloud adds a backend
   * crate and nothing here.
   *
   * It owns no state: the choice and the method's fields live in the connection's `params`, which
   * the parent already persists, and the parent hands in how to read and write them.
   */
  import { Cloud } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { AuthMethodDescriptor } from '$lib/ipc/picus/db';

  interface Props {
    /** Methods that can sign in to the current engine. */
    methods: AuthMethodDescriptor[];
    /** The chosen method id, or `password`. */
    value: string;
    /** Read one of the connection's params. */
    param: (key: string) => string;
    onSelect: (id: string) => void;
    onParam: (key: string, value: string) => void;
    /** Whether the connection requires TLS right now. */
    tls: boolean;
    onRequireTls: () => void;
  }

  let { methods, value, param, onSelect, onParam, tls, onRequireTls }: Props = $props();

  const active = $derived(methods.find((m) => m.id === value) ?? null);
  const options = $derived([
    { value: 'password', label: 'Password' },
    ...methods.map((m) => ({ value: m.id, label: m.label })),
  ]);
</script>

{#if methods.length}
  <FormField label="Authentication">
    <Select fill ariaLabel="Authentication" value={active ? active.id : 'password'} {options} onchange={onSelect} />
  </FormField>

  {#if active}
    <Alert variant="info" compact>
      <span class="auth-note"><Cloud size={12} /><span>{active.help}</span></span>
    </Alert>
    {#each active.fields as f (f.id)}
      <FormField label={f.label} hint={f.help ?? undefined}>
        <Input value={param(f.id)} placeholder={f.placeholder ?? ''} oninput={(v) => onParam(f.id, v)} />
      </FormField>
    {/each}
    {#if active.requiresTls && !tls}
      <Alert variant="warning" compact>
        <span class="auth-note">
          <span>{active.label} sends a token, which the server refuses over a plaintext connection.</span>
          <Button size="xs" variant="ghost" onclick={onRequireTls}>Require TLS</Button>
        </span>
      </Alert>
    {/if}
  {/if}
{/if}

<style>
  .auth-note { display: inline-flex; align-items: center; gap: 6px; flex-wrap: wrap; }
</style>
