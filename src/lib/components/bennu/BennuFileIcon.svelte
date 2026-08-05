<script lang="ts">
  /**
   * The icon for a Java-project FILE, from the two sources that answer different questions.
   *
   * A `.java` file gets the **kind it declares** — class / interface / enum / record /
   * annotation — off the project's class index, which is what IntelliJ shows and what no
   * by-extension table can know. Everything else goes to the shared resolver
   * (`utils/file-icons`), the same one Corvus's tree and Sitta's explorer use, so a
   * `pom.xml` looks like a `pom.xml` wherever you meet it.
   *
   * That rule was written once inside the project tree and nowhere else, so every other
   * list of files in Bennu — the Go-to navigator most visibly — fell back to one generic
   * glyph for everything. This is the rule as a component, so a list of files is one prop
   * away from looking like the tree.
   *
   * Takes `size` (ignored: the mark scales with the row's font-size, like it does in the
   * tree) so it satisfies the `IconComponent` shape a host list expects.
   */
  import IconifyIconView from '@iconify/svelte';
  import { getFileIcon } from '$lib/utils/file-icons';
  import JavaKindIcon from './JavaKindIcon.svelte';
  import { javaKindStore } from '$lib/stores/bennu/java-kinds.svelte';

  let { path, size = 13 }: { path: string; size?: number } = $props();

  const norm = $derived(path.replace(/\\/g, '/'));
  const isJava = $derived(norm.endsWith('.java'));
  const name = $derived(norm.split('/').pop() ?? norm);
</script>

{#if isJava}
  <!-- A `.java` whose kind isn't indexed yet reads as a class — the overwhelmingly common
       answer, and it settles the moment the index does. -->
  <JavaKindIcon kind={javaKindStore.kindOf(norm)} />
{:else}
  <IconifyIconView icon={getFileIcon(name)} width={size} height={size} />
{/if}
