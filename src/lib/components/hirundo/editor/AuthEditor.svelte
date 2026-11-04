<script lang="ts">
  /**
   * AuthEditor — Bearer / Basic / API key, or "inherit" / "none". Used by the request, and by the settings of
   * a collection or folder (where there is nothing above to inherit from at the top, so `allowInherit` is off).
   * Values may hold `{{variables}}`; a token is usually `{{token}}`, with the real value a secret variable.
   */
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { Auth } from '$lib/types/hirundo';

  interface Props {
    auth: Auth;
    onchange: (auth: Auth) => void;
    allowInherit?: boolean;
    /** What "inherit" resolves to, in words (`Bearer from Bandi`), shown under the selector. */
    inheritedLabel?: string;
  }

  let { auth, onchange, allowInherit = true, inheritedLabel = '' }: Props = $props();

  const options = $derived([
    ...(allowInherit ? [{ value: 'inherit', label: 'Inherit from the level above' }] : []),
    { value: 'none', label: 'No auth' },
    { value: 'bearer', label: 'Bearer token' },
    { value: 'basic', label: 'Basic' },
    { value: 'api_key', label: 'API key' },
    { value: 'ws_security', label: 'WS-Security (SOAP)' },
  ]);

  function choose(type: string) {
    switch (type) {
      case 'bearer': onchange({ type: 'bearer', token: '' }); break;
      case 'basic': onchange({ type: 'basic', username: '', password: '' }); break;
      case 'api_key': onchange({ type: 'api_key', name: 'X-API-Key', value: '', location: 'header' }); break;
      case 'ws_security': onchange({ type: 'ws_security', username: '', password: '' }); break;
      case 'none': onchange({ type: 'none' }); break;
      default: onchange({ type: 'inherit' });
    }
  }
</script>

<div class="auth">
  <FormField label="Type">
    <Select value={auth.type} {options} onchange={(v) => choose(String(v))} />
  </FormField>

  {#if auth.type === 'inherit'}
    <Alert variant="info" compact text={inheritedLabel ? `Uses ${inheritedLabel}.` : 'Nothing above sets auth: the request goes out without it.'} />
  {:else if auth.type === 'none'}
    <Alert variant="info" compact text="No auth, even if a collection or folder above has some." />
  {:else if auth.type === 'bearer'}
    <FormField label="Token" hint="Sent as Authorization: Bearer …">
      <Input value={auth.token} placeholder="{'{{token}}'}" oninput={(v) => onchange({ ...auth, token: v })} />
    </FormField>
  {:else if auth.type === 'basic'}
    <FormField label="Username"><Input value={auth.username} oninput={(v) => onchange({ ...auth, username: v })} /></FormField>
    <FormField label="Password"><Input value={auth.password} type="password" oninput={(v) => onchange({ ...auth, password: v })} /></FormField>
  {:else if auth.type === 'api_key'}
    <FormField label="Name"><Input value={auth.name} oninput={(v) => onchange({ ...auth, name: v })} /></FormField>
    <FormField label="Value"><Input value={auth.value} placeholder="{'{{apiKey}}'}" oninput={(v) => onchange({ ...auth, value: v })} /></FormField>
    <FormField label="Send it in">
      <Select value={auth.location} options={[{ value: 'header', label: 'Header' }, { value: 'query', label: 'Query string' }]} onchange={(v) => onchange({ ...auth, location: v === 'query' ? 'query' : 'header' })} />
    </FormField>
  {:else if auth.type === 'ws_security'}
    <Alert variant="info" compact text="A UsernameToken (plain-text password, so use https) is put in the Header of a SOAP envelope. For a SOAP request only." />
    <FormField label="Username"><Input value={auth.username} oninput={(v) => onchange({ ...auth, username: v })} /></FormField>
    <FormField label="Password"><Input value={auth.password} type="password" placeholder={'{{password}}'} oninput={(v) => onchange({ ...auth, password: v })} /></FormField>
  {/if}
</div>

<style>
  .auth { display: flex; flex-direction: column; gap: 12px; padding: 12px; max-width: 560px; }
</style>
