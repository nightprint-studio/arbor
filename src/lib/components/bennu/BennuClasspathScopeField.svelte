<script lang="ts">
  /**
   * Which Maven scopes a launch sees — the field every JVM-launching configuration shares.
   *
   * `runtime` is what `mvn spring-boot:run` and a packaged application see, and it is the default
   * because the alternative is a launch that quietly differs from production. The other choices
   * exist because the exceptions are real: a launcher that wants a test-scoped H2, or an
   * application server that provides an API the project marks `provided` and a plain run has to
   * supply itself.
   */
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';

  let {
    config,
    patch,
  }: {
    config: RunConfig;
    patch: (p: Partial<Omit<RunConfig, 'id'>>) => void;
  } = $props();
</script>

<FormField
  label="Classpath"
  hint="Which Maven scopes the launch sees. Editing and completion always see every scope — this is only the run."
>
  <Select
    value={config.classpathScope}
    options={[
      { value: 'runtime', label: 'Runtime — what mvn spring-boot:run sees' },
      { value: 'compile', label: 'Compile' },
      { value: 'test', label: 'Test — adds the test-scoped dependencies' },
      { value: '', label: 'Every scope — the same classpath the editor indexes' },
    ]}
    onchange={(v) => patch({ classpathScope: v })}
  />
</FormField>
