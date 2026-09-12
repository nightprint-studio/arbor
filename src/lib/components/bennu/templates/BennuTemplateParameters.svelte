<script lang="ts">
  /**
   * The values a template preview lays over its data: a JSON object, edited beside the output.
   *
   * A parameter is only worth writing when it names something the template reads, so the kind's
   * top-level variables are listed under the editor.
   */
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { templateSchema, type TemplateKindId } from '$lib/ipc/bennu/templates';
  import { languageForPath } from '../languages';

  let {
    kind,
    value,
    onchange,
  }: {
    kind: TemplateKindId;
    /** The JSON as typed — possibly not valid yet. */
    value: string;
    onchange: (text: string) => void;
  } = $props();

  const language = languageForPath('parameters.json');
  let variables = $state<string[]>([]);

  $effect(() => {
    const requested = kind;
    let current = true;
    templateSchema(requested)
      .then((schema) => {
        const properties = (schema.context as { properties?: Record<string, unknown> }).properties ?? {};
        if (current) variables = Object.keys(properties);
      })
      .catch(() => { if (current) variables = []; });
    return () => { current = false; };
  });
</script>

<div class="tpp">
  <div class="tpp-editor">
    <CodeEditor
      {value}
      {language}
      oninput={onchange}
      lineNumbers={false}
      folding={false}
      highlightActiveLine={false}
      placeholder={'{ "name": "value" }'}
    />
  </div>
  <span class="tpp-hint">
    Laid over the data: objects merge key by key, anything else replaces.
    {#if variables.length > 0}Top level: <code>{variables.join(', ')}</code>{/if}
  </span>
</div>

<style>
  .tpp { display: flex; flex-direction: column; gap: 4px; padding: 6px 10px; border-bottom: 1px solid var(--border-subtle); }
  .tpp-editor { height: 110px; display: flex; flex-direction: column; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); overflow: hidden; }
  .tpp-hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .tpp-hint code { font-family: var(--font-code); color: var(--text-secondary); }
</style>
