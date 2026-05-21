<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';

  let { name = '', email = '', size = 24 }: { name?: string; email?: string; size?: number } = $props();

  const initials = $derived(
    name.split(' ').map(p => p[0]).slice(0, 2).join('').toUpperCase() || '?'
  );

  // Deterministic hue from email/name
  const hue = $derived(
    [...(email || name)].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  );
</script>

<div
  class="avatar"
  style="width: {size}px; height: {size}px; font-size: {Math.floor(size * 0.38)}px; background: hsl({hue}, 40%, 30%); border: 1px solid hsl({hue}, 40%, 40%)"
  use:tooltip={email ? { content: name || '?', description: email } : (name || '?')}
>
  {initials}
</div>

<style>
  .avatar {
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    font-weight: 600;
    color: hsl(var(--hue, 0), 60%, 90%);
    flex-shrink: 0;
    user-select: none;
  }
</style>
