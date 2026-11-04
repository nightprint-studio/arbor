<script lang="ts">
  /**
   * Generate Spring Security configuration — a SecurityFilterChain (or the pre-5.7 adapter) for a JWT
   * resource server, several issuers, form login, HTTP Basic, or an API and a web UI together; with CORS,
   * CSRF, method security, a user store and a password encoder. The DSL follows the Spring Security
   * version on the classpath, so the same choices write `requestMatchers` on 6.x and `antMatchers` on 5.x.
   */
  import { onMount } from 'svelte';
  import { Plus, ShieldCheck, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { createGeneratorSession, type GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import GeneratorShell from './GeneratorShell.svelte';
  import StringListEditor from './StringListEditor.svelte';

  interface Rule { method: string; path: string; access: string }
  interface Issuer { issuer_uri: string; audience: string; roles_claim: string }
  interface Options {
    class_name: string;
    package: string;
    scenario: string;
    style: string;
    swagger: boolean;
    health: boolean;
    cors: boolean;
    cors_origins: string[];
    cors_credentials: boolean;
    csrf: boolean;
    csrf_cookie: boolean;
    method_security: boolean;
    user_store: string;
    password_encoder: string;
    h2_console: boolean;
    rest_entry_point: boolean;
    jwt_source: string;
    issuers: Issuer[];
    rules: Rule[];
    any_request: string;
    authority_prefix: string;
  }

  let { host, onClose }: { host: GeneratorHost; onClose: () => void } = $props();

  const session = createGeneratorSession<Options>('spring-security', host);
  onMount(() => void session.open());
  const o = $derived(session.options);

  const SCENARIOS = [
    { value: 'jwt', label: 'API with JWT', description: 'Stateless resource server, one issuer.' },
    { value: 'multi', label: 'Multi-issuer API', description: 'Several identity providers, each with its own audience and roles.' },
    { value: 'form', label: 'Web with form login', description: 'Session, login page, CSRF.' },
    { value: 'webapi', label: 'Web and API together', description: '/api/** with JWT, the rest with form login — two ordered chains.' },
    { value: 'basic', label: 'API with HTTP Basic', description: 'Stateless, user store and password encoder.' },
  ];
  const STYLES = [
    { value: 'auto', label: 'Auto', description: 'The newest DSL the Spring Security version allows' },
    { value: 'lambda', label: 'Lambda DSL' },
    { value: 'chained', label: 'Chained .and()', description: 'Deprecated since 6.1, gone in 7' },
  ];
  const METHODS = ['', 'GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => ({ value: m, label: m || 'Any method' }));
  const ACCESS = ['permitAll()', 'authenticated()', 'denyAll()', 'hasRole("ADMIN")', 'hasRole("USER")', 'hasAuthority("SCOPE_read")'];
  const accessOptions = (current: string) =>
    (ACCESS.includes(current) || !current ? ACCESS : [current, ...ACCESS]).map((a) => ({ value: a, label: a }));
  const JWT_SOURCES = [
    { value: 'issuer', label: 'Issuer URI (discovery)' },
    { value: 'jwk', label: 'JWK set URI' },
    { value: 'key', label: 'Public key file' },
  ];
  const USER_STORES = [
    { value: 'memory', label: 'In memory (development)' },
    { value: 'jdbc', label: 'JDBC (users / authorities tables)' },
    { value: 'custom', label: 'My own UserDetailsService' },
    { value: 'none', label: 'None' },
  ];
  const ENCODERS = [
    { value: 'bcrypt', label: 'BCrypt' },
    { value: 'delegating', label: 'Delegating ({bcrypt}…)' },
    { value: 'none', label: 'None' },
  ];

  const usesJwt = $derived(o.scenario === 'jwt' || o.scenario === 'multi' || o.scenario === 'webapi');
  const usesLogin = $derived(o.scenario === 'form' || o.scenario === 'webapi' || o.scenario === 'basic');

  function addIssuer() {
    o.issuers = [...o.issuers, { issuer_uri: '', audience: '', roles_claim: '' }];
  }
  function addRule() {
    o.rules = [...o.rules, { method: '', path: '', access: 'authenticated()' }];
  }

  /** Scenario changes carry the defaults that go with them, so the common choice needs no more clicks. */
  function pickScenario(value: string) {
    const stateless = value === 'jwt' || value === 'multi' || value === 'basic';
    o.csrf = !stateless;
    o.cors = stateless || value === 'webapi';
    if (value === 'multi' && o.issuers.length < 2) addIssuer();
  }
</script>

<GeneratorShell {session} icon={ShieldCheck} title="Spring Security configuration" {onClose}>
  {#snippet form()}
    {#if o}
      <FormSection label="Setup" first>
        <RadioGroup bind:value={o.scenario} options={SCENARIOS} appearance="card" size="sm" direction="vertical" onchange={pickScenario} />
        <FormField label="DSL style">
          <RadioGroup bind:value={o.style} options={STYLES} appearance="segment" size="sm" block />
        </FormField>
      </FormSection>

      <FormSection label="Class">
        <FormField label="Name"><Input block size="sm" bind:value={o.class_name} ariaLabel="Class name" /></FormField>
        <FormField label="Package"><Input block size="sm" bind:value={o.package} ariaLabel="Package" /></FormField>
      </FormSection>

      {#if usesJwt}
        <FormSection label={o.scenario === 'multi' ? 'Issuers' : 'Token'}>
          {#if o.scenario !== 'multi'}
            <FormField label="Keys come from">
              <Select bind:value={o.jwt_source} options={JWT_SOURCES} size="sm" />
            </FormField>
          {/if}
          {#each o.issuers as issuer, i (i)}
            <div class="sg-issuer">
              <Input block size="sm" placeholder="Issuer URI" ariaLabel={`Issuer URI ${i + 1}`} bind:value={issuer.issuer_uri} />
              <div class="sg-pair">
                <Input block size="sm" placeholder="Audience (optional)" ariaLabel={`Audience ${i + 1}`} bind:value={issuer.audience} />
                <Input block size="sm" placeholder="Roles claim, e.g. realm_access.roles" ariaLabel={`Roles claim ${i + 1}`} bind:value={issuer.roles_claim} />
                {#if o.scenario === 'multi' && o.issuers.length > 2}
                  <IconButton tooltip="Remove issuer" size={12} onclick={() => (o.issuers = o.issuers.filter((_, n) => n !== i))}><X size={12} /></IconButton>
                {/if}
              </div>
            </div>
          {/each}
          {#if o.scenario === 'multi'}
            <div>
              <Button variant="ghost" size="sm" onclick={addIssuer}>
                {#snippet iconStart()}<Plus size={12} />{/snippet}
                Issuer
              </Button>
            </div>
          {/if}
          <FormField label="Authority prefix" hint="Added to each role taken from the token">
            <Input block size="sm" bind:value={o.authority_prefix} ariaLabel="Authority prefix" />
          </FormField>
        </FormSection>
      {/if}

      <FormSection label="Paths">
        <div class="sg-fixed">
          <Toggle bind:checked={o.swagger} size="sm" label="Swagger UI public" />
          <Toggle bind:checked={o.health} size="sm" label="Health endpoint public" />
        </div>
        {#each o.rules as rule, i (i)}
          <div class="sg-rule">
            <Select bind:value={rule.method} options={METHODS} size="sm" narrow ariaLabel={`Method ${i + 1}`} />
            <Input size="sm" block placeholder="/path/**" ariaLabel={`Path ${i + 1}`} bind:value={rule.path} />
            <Select bind:value={rule.access} options={accessOptions(rule.access)} size="sm" ariaLabel={`Access ${i + 1}`} />
            <IconButton tooltip="Remove rule" size={12} onclick={() => (o.rules = o.rules.filter((_, n) => n !== i))}><X size={12} /></IconButton>
          </div>
        {/each}
        <div>
          <Button variant="ghost" size="sm" onclick={addRule}>
            {#snippet iconStart()}<Plus size={12} />{/snippet}
            Rule
          </Button>
        </div>
        <FormField label="Everything else">
          <Select bind:value={o.any_request} options={accessOptions(o.any_request).slice(0, 4)} size="sm" />
        </FormField>
      </FormSection>

      <FormSection label="Behaviour">
        <Toggle bind:checked={o.cors} size="sm" label="CORS" description="Writes a CorsConfigurationSource bean" />
        {#if o.cors}
          <StringListEditor bind:items={o.cors_origins} placeholder="https://app.example.com" addLabel="Origin" ariaLabel="Origin" />
          <Toggle bind:checked={o.cors_credentials} size="sm" label="Allow credentials" />
        {/if}
        <Toggle bind:checked={o.csrf} size="sm" label="CSRF protection" description="Off for stateless APIs" />
        {#if o.csrf}
          <Toggle bind:checked={o.csrf_cookie} size="sm" label="Token in a cookie" description="For a single-page app that reads XSRF-TOKEN" />
        {/if}
        <Toggle bind:checked={o.method_security} size="sm" label="Method security" description="@PreAuthorize / @PostAuthorize" />
        <Toggle bind:checked={o.rest_entry_point} size="sm" label="401 instead of a login redirect" />
        <Toggle bind:checked={o.h2_console} size="sm" label="H2 console frames" />
      </FormSection>

      {#if usesLogin}
        <FormSection label="Users">
          <FormField label="User store"><Select bind:value={o.user_store} options={USER_STORES} size="sm" /></FormField>
          <FormField label="Password encoder"><RadioGroup bind:value={o.password_encoder} options={ENCODERS} appearance="segment" size="sm" block /></FormField>
        </FormSection>
      {/if}
    {/if}
  {/snippet}
</GeneratorShell>

<style>
  .sg-issuer { display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .sg-pair { display: flex; gap: 4px; align-items: center; }
  .sg-pair > :global(.input-wrap) { flex: 1; min-width: 0; }
  .sg-fixed { display: flex; flex-direction: column; gap: 6px; }
  .sg-rule { display: grid; grid-template-columns: 96px minmax(0, 1fr) minmax(0, 150px) auto; gap: 4px; align-items: center; }
</style>
