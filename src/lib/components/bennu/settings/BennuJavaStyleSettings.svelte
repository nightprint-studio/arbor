<script lang="ts">
  /**
   * Settings › Java › Code Style — how generated Java is written, by Generate and by every code
   * template, with a preview that reflects each choice.
   */
  import { Wand2 } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const s = bennuSettingsStore;

  /** Whether the open project has Lombok — it changes the wording of one row, not whether it can
   *  be chosen: the setting is yours for every project, and each applies it only where Lombok is. */
  const hasLombok = $derived(projectStore.capabilities?.lombok ?? false);

  const indentUnit = $derived(s.indentStyle === 'tabs' ? '\t' : ' '.repeat(s.tabSize));

  /** A minimal class whose formatting reflects the cards above: final params, val / var locals,
   *  arrow switch, brace spacing, and the blank line between members. */
  const styleSnippet = $derived.by(() => {
    const i = indentUnit;
    const fin = s.finalParams ? 'final ' : '';
    // A local with an initializer — the only kind `val` and `var` can declare.
    const localDecl = s.useLombokVal ? 'val' : s.useLocalVar ? `${fin}var` : `${fin}Account`;
    // Spaces-in-braces only affects single-line bodies (the getter here).
    const openB = s.spaceInBraces ? '{ ' : '{';
    const closeB = s.spaceInBraces ? ' }' : '}';
    const gap = s.blankLineBetweenMembers ? '\n' : '';

    const getter = `${i}public String getName() ${openB}return this.name;${closeB}`;

    const copy = [
      `${i}public Account copy() {`,
      `${i}${i}${localDecl} copy = new Account();`,
      `${i}${i}copy.name = this.name;`,
      `${i}${i}return copy;`,
      `${i}}`,
    ].join('\n');

    const label = s.switchWithReturn
      ? [
          `${i}public String label(${fin}int code) {`,
          `${i}${i}return switch (code) {`,
          `${i}${i}${i}case 0 -> "off";`,
          `${i}${i}${i}default -> "on";`,
          `${i}${i}};`,
          `${i}}`,
        ].join('\n')
      : [
          `${i}public String label(${fin}int code) {`,
          `${i}${i}String result;`,
          `${i}${i}switch (code) {`,
          `${i}${i}${i}case 0: result = "off"; break;`,
          `${i}${i}${i}default: result = "on";`,
          `${i}${i}}`,
          `${i}${i}return result;`,
          `${i}}`,
        ].join('\n');

    return [
      'public class Account {',
      `${i}private String name;`,
      gap ? '' : null,
      getter,
      gap ? '' : null,
      copy,
      gap ? '' : null,
      label,
      '}',
    ].filter((l) => l !== null).join('\n');
  });
</script>

<div class="section-header">
  <h2>Code Style</h2>
  <p>How generated Java is written — by <strong>Generate</strong> and by every code template.</p>
</div>
<div class="card">
  <div class="card-section-title"><Wand2 size={12} /> Declarations</div>
  <FormRow label="Final generated params and locals" description="Declare generated constructor, setter and with-method parameters as final.">
    <Toggle checked={s.finalParams} onchange={(v) => s.setFinalParams(v)} ariaLabel="Final generated params and locals" />
  </FormRow>
  <!-- Not disabled without Lombok: the setting is yours for every project, and each one applies it
       only where Lombok is — so the project open now decides the wording, not whether it can be
       chosen. -->
  <FormRow label="Use Lombok val for locals" description={hasLombok ? 'Declare generated locals with Lombok’s val, in projects that have Lombok.' : 'Declare generated locals with Lombok’s val, in projects that have Lombok — this one does not, so it keeps the type.'}>
    <Toggle checked={s.useLombokVal} onchange={(v) => s.setUseLombokVal(v)} ariaLabel="Use Lombok val for locals" />
  </FormRow>
  <FormRow label="Use var for locals" description="Declare generated locals with var in projects on Java 10 or later. Where Lombok’s val is chosen too and the project has Lombok, val wins.">
    <Toggle checked={s.useLocalVar} onchange={(v) => s.setUseLocalVar(v)} ariaLabel="Use var for locals" />
  </FormRow>
  <FormRow label="Switch with return" description="Prefer arrow-style switch expressions that return a value when generating switches.">
    <Toggle checked={s.switchWithReturn} onchange={(v) => s.setSwitchWithReturn(v)} ariaLabel="Switch with return" />
  </FormRow>
</div>
<div class="card">
  <div class="card-section-title"><Wand2 size={12} /> Spacing</div>
  <FormRow label="Spaces inside braces" description="Add a space just inside the braces on single-line generated bodies.">
    <Toggle checked={s.spaceInBraces} onchange={(v) => s.setSpaceInBraces(v)} ariaLabel="Spaces inside braces" />
  </FormRow>
  <FormRow label="Blank line between members" description="Separate each generated method with a blank line.">
    <Toggle checked={s.blankLineBetweenMembers} onchange={(v) => s.setBlankLineBetweenMembers(v)} ariaLabel="Blank line between members" />
  </FormRow>
</div>
<div class="card">
  <div class="card-section-title"><Wand2 size={12} /> Formatter</div>
  <p class="set-hint">
    What <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> does to a <code>.java</code> file.
    Indentation comes from Editor → Indentation, so the formatter and the editor never disagree. A
    language with a language server is formatted by it instead, reading the project's own
    <code>rustfmt.toml</code> or <code>.prettierrc</code> — nothing here applies to those.
  </p>
  <FormRow label="Blank lines between members" description="The most consecutive blank lines the formatter keeps. 0 removes them all.">
    <NumberStepper value={s.javaBlankLines} min={0} max={5} narrow
                   onchange={(v) => s.setJavaBlankLines(v)} ariaLabel="Blank lines between members" />
  </FormRow>
  <FormRow label="Indent case bodies" description="Indent the statements under a case label one level in from it — the Sun/Oracle convention, and IntelliJ's.">
    <Toggle checked={s.javaIndentCaseBody} onchange={(v) => s.setJavaIndentCaseBody(v)} ariaLabel="Indent case bodies" />
  </FormRow>
</div>
<div class="card">
  <div class="card-section-title"><Wand2 size={12} /> Preview</div>
  <pre class="set-snippet" aria-label="Java style preview">{styleSnippet}</pre>
</div>
