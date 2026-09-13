<script lang="ts">
  /**
   * Settings › Rust › Code Style — how generated Rust is written, by every code template.
   *
   * Deliberately narrower than Java's page, and the reason is worth saying once: **`rustfmt` owns
   * the formatting.** Indentation, line breaks, where a brace goes — a Rust project answers all of
   * that in its own `rustfmt.toml`, and a second set of switches here would either agree with it
   * (pointless) or fight it (worse). What is left is the set of decisions a *generator* has to make
   * and a formatter never touches: what is public, what a type derives, whether items are
   * documented, how failure is spelled.
   *
   * A template reads all of it as `style.rust.*`.
   */
  import { FileStack, ShieldCheck, Sparkles } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;

  const visibilityOptions = [
    { value: 'pub(crate)', label: 'pub(crate) — visible inside this crate' },
    { value: 'pub', label: 'pub — part of the crate’s API' },
    { value: 'private', label: 'private — visible in its module' },
  ];

  const errorOptions = [
    { value: 'anyhow', label: 'anyhow::Result<T>' },
    { value: 'thiserror', label: 'a crate error type (thiserror)' },
    { value: 'std', label: 'Result<T, E>, spelled out' },
  ];

  /** What the visibility is written as — the empty string for a private item. */
  const vis = $derived(
    s.rustVisibility === 'private' ? '' : `${s.rustVisibility} `,
  );

  const derives = $derived(s.rustDerivesList());

  /** The signature of a fallible function, in the style chosen. */
  const resultType = $derived(
    s.rustErrorStyle === 'anyhow'
      ? 'anyhow::Result<Self>'
      : s.rustErrorStyle === 'thiserror'
        ? 'Result<Self, AccountError>'
        : 'Result<Self, std::io::Error>',
  );

  /** A small generated type, written exactly the way the cards above say. */
  const snippet = $derived.by(() => {
    const i = s.indentStyle === 'tabs' ? '\t' : ' '.repeat(s.tabSize);
    const doc = (text: string, indent = '') => (s.rustDocComments ? [`${indent}/// ${text}`] : []);
    const returned = s.rustSelfInImpl ? 'Self' : 'Account';
    return [
      ...doc('An account, as the store holds it.'),
      ...(derives.length ? [`#[derive(${derives.join(', ')})]`] : []),
      `${vis}struct Account {`,
      `${i}${vis}id: String,`,
      `${i}${vis}name: String,`,
      '}',
      '',
      'impl Account {',
      ...doc('Read one by id.', i),
      `${i}${vis}fn load(id: &str) -> ${resultType} {`,
      `${i}${i}Ok(${returned} { id: id.to_string(), name: String::new() })`,
      `${i}}`,
      '}',
    ].join('\n');
  });
</script>

<div class="section-header">
  <h2>Code Style</h2>
  <p>
    How generated Rust is written, by every code template. Formatting is
    <code>rustfmt</code>’s — the project’s own <code>rustfmt.toml</code> — and nothing here competes
    with it: these are the decisions a generator has to make and a formatter never touches.
  </p>
</div>

<div class="card">
  <div class="card-section-title"><FileStack size={12} /> Declarations</div>
  <FormRow label="Visibility" description="What a generated item is declared as — a type, its fields, and the functions in its impl.">
    <Select
      value={s.rustVisibility}
      options={visibilityOptions}
      ariaLabel="Visibility of generated items"
      onchange={(v) => s.setRustVisibility(v as typeof s.rustVisibility)}
    />
  </FormRow>
  <FormRow label="Documentation lines" description="Give each generated item a /// line. What missing_docs asks for on anything public.">
    <Toggle checked={s.rustDocComments} onchange={(v) => s.setRustDocComments(v)} ariaLabel="Documentation lines" />
  </FormRow>
  <FormRow label="Self inside impl" description="Write Self rather than the type’s name in a return type or a constructor — Clippy’s use_self.">
    <Toggle checked={s.rustSelfInImpl} onchange={(v) => s.setRustSelfInImpl(v)} ariaLabel="Self inside impl" />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><Sparkles size={12} /> Derives</div>
  <p class="set-hint">
    Written on a generated <code>struct</code> or <code>enum</code>, in this order. Leave it empty
    for no <code>#[derive(…)]</code> line at all — a template can still add its own.
  </p>
  <div class="pad">
    <Input
      value={s.rustDerives}
      placeholder="Debug, Clone"
      ariaLabel="Derived traits"
      oninput={(v) => s.setRustDerives(v)}
    />
  </div>
</div>

<div class="card">
  <div class="card-section-title"><ShieldCheck size={12} /> Errors</div>
  <FormRow label="How a function reports failure" description="What a generated fallible function returns. A template reads it as style.rust.error_style and writes the type it names.">
    <Select
      value={s.rustErrorStyle}
      options={errorOptions}
      ariaLabel="Error style"
      onchange={(v) => s.setRustErrorStyle(v as typeof s.rustErrorStyle)}
    />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><FileStack size={12} /> Preview</div>
  <pre class="set-snippet" aria-label="Rust style preview">{snippet}</pre>
</div>

<style>
  .pad { padding: 0 14px 12px; }
</style>
