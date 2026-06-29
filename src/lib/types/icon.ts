import type { ComponentType, SvelteComponent } from 'svelte';

/**
 * A lucide-svelte icon component.
 *
 * lucide-svelte (through v0.468) ships its icons as **Svelte-4 class-based**
 * components (`extends SvelteComponentTyped`), not Svelte-5 functional
 * `Component`s. Typing an icon slot/map as `Component<…>` therefore rejects every
 * icon (`typeof Play is not assignable to Component<{}, {}, string>`). Use this
 * alias — the legacy `ComponentType<SvelteComponent>` — for any `icon` prop or
 * icon map so lucide icons (and any other class-based icon) assign cleanly.
 */
export type IconComponent = ComponentType<SvelteComponent>;
