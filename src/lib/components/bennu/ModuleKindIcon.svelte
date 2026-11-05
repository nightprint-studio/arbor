<script lang="ts">
  /**
   * The mark of a build ecosystem — Maven, Gradle, Cargo (Rust) or Node — for a row that names a module.
   * One component so every list of modules (the selector tree of the tool windows, a picker) says "which
   * tool is this" the same way, with the brand rather than a generic package glyph.
   */
  import IconifyIconView from '@iconify/svelte';
  import { PROJECT_ICON } from '$lib/utils/brand-icons';
  import type { ModuleKind } from '$lib/ipc/bennu/modules';

  let { kind, size = 13 }: { kind: ModuleKind; size?: number } = $props();

  const ICONS = {
    Maven: PROJECT_ICON.java_maven,
    Gradle: PROJECT_ICON.java_gradle,
    Node: PROJECT_ICON.node_js,
    Cargo: PROJECT_ICON.rust,
  } as const;
</script>

<IconifyIconView icon={ICONS[kind]} width={size} height={size} />
