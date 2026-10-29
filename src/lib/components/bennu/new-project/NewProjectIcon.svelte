<script lang="ts">
  /**
   * The mark of a generator or a category in the New Project wizard.
   *
   * Brands that have a real mark in the app (Maven, Bevy) use it; the rest get a lucide glyph that
   * names the *kind* of project. Keyed by the `icon` string the backend sends, so a new generator
   * picks an existing key (or falls back to the generic box) without touching this file.
   */
  import {
    Leaf, Zap, Hammer, Globe, Cog, Flame, Layers, Hexagon, Triangle, Box, Coffee, Rocket,
  } from 'lucide-svelte';
  import type { IconComponent } from '$lib/types/icon';
  import MavenIcon from '../MavenIcon.svelte';
  import BevyIcon from '../BevyIcon.svelte';

  let { name, size = 15 }: { name: string; size?: number } = $props();

  const GLYPHS: Record<string, IconComponent> = {
    spring: Leaf,
    quarkus: Zap,
    gradle: Hammer,
    web: Globe,
    rust: Cog,
    vite: Zap,
    angular: Triangle,
    svelte: Flame,
    vue: Layers,
    next: Rocket,
    node: Hexagon,
    java: Coffee,
  };

  const Glyph = $derived(GLYPHS[name] ?? Box);
</script>

{#if name === 'maven'}
  <MavenIcon {size} />
{:else if name === 'bevy'}
  <BevyIcon {size} />
{:else}
  <Glyph {size} />
{/if}
