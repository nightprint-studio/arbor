<script lang="ts">
  /**
   * EnvSelector — which environment requests go to. It lives in the header of the request area, because that
   * is where the question is asked; the type of the environment is what colours it (a dot, a tinted
   * trigger), and a protected type says so next to it. Nothing else on screen is tinted: an environment
   * is a fact about the *next send*, not a theme.
   */
  import { ChevronDown, Lock } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import { typeColor } from '../request-model';

  const env = $derived(hirundoStore.activeEnv);
  const type = $derived(hirundoStore.activeEnvType);
  const color = $derived(type?.color ?? 'var(--text-muted)');

  const items = $derived<DropdownItem[]>([
    { kind: 'item', id: '', label: 'No environment', active: !hirundoStore.envs.local.active_env, onclick: () => void hirundoStore.selectEnvironment('') },
    ...hirundoStore.envs.environments.map((e): DropdownItem => ({
      kind: 'item', id: e.id, label: e.name, iconColor: typeColor(hirundoStore.envs.types, e.type_id),
      meta: hirundoStore.envs.types.find((t) => t.id === e.type_id)?.name,
      active: e.id === hirundoStore.envs.local.active_env,
      onclick: () => void hirundoStore.selectEnvironment(e.id),
    })),
    { kind: 'separator' },
    { kind: 'item', id: 'manage', label: 'Manage environments…', shortcut: 'Ctrl+E', onclick: hirundoCommands.environments },
  ]);
</script>

<div class="es">
  {#if type?.confirm_mutating}
    <Badge variant="tone" tone="warning" size="sm" tooltip={`${type.name} asks before POST, PUT, PATCH and DELETE`}>{#snippet icon()}<Lock size={10} />{/snippet}protected</Badge>
  {/if}
  <Dropdown {items} width="270px" direction="down" position="fixed">
    {#snippet trigger({ open, toggle })}
      <Button variant="outline" size="sm" ariaExpanded={open} ariaHaspopup="menu" ariaLabel="Environment" onclick={toggle}>
        {#snippet iconStart()}<span class="es-dot" style:background={color}></span>{/snippet}
        {env?.name ?? "No environment"}
        <ChevronDown size={12} />
      </Button>
    {/snippet}
  </Dropdown>
</div>

<style>
  .es { display: flex; align-items: center; gap: 8px; }
  .es-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--env); }
</style>
