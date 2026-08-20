<script lang="ts">
  /**
   * What a tab shows while its product is coming up.
   *
   * ## Why this exists
   *
   * Opening a product used to cross two loading screens with nothing in common. The container
   * showed a spinner labelled "Starting Bennu…" while the backend spawned; the moment the shell
   * mounted, a *different* spinner — unlabelled — took over for the chunk load. So one action
   * produced two screens, the second of which said less than the first, and the visible effect
   * was a spinner that sometimes had a name under it and sometimes did not.
   *
   * One component for both phases fixes that at the root: the screen does not change when the
   * phase does, only the line at the bottom does. Which is what "loading" should look like —
   * one thing happening, with a description that keeps up.
   *
   * ## It is the product's screen, not Arbor's
   *
   * The icon, the name, the one-line role and the accent all come from {@link PRODUCTS} — the
   * one table that says what a product is. So opening Bennu looks like opening Bennu, and the
   * wait is spent looking at the thing you asked for rather than at a neutral spinner that could
   * belong to anything. The accent is scoped to this element, so nothing else in the window
   * takes on the colour.
   *
   * Deliberately NOT the full boot splash: that one has a drifting orb mesh, which is right once
   * at application start and wrong on a surface that may appear several times in a session — a
   * blurred, animated layer is exactly the kind of thing that keeps the compositor busy
   * (CLAUDE.md, hard rule 6).
   */
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import { PRODUCTS, type ProductId } from '$lib/utils/products';
  import type { SurfaceId } from '$lib/stores/surfaces.svelte';
  import { surfaceDef } from '$lib/stores/surfaces.svelte';

  let { id, phase }: {
    id: SurfaceId;
    /**
     * Which half of the wait this is.
     *
     * `backend` — the product's process is starting. `interface` — its code is being fetched
     * and mounted. Two lines rather than one because they fail differently and take different
     * amounts of time, and "it is still on the first one" is worth being able to see.
     */
    phase: 'backend' | 'interface';
  } = $props();

  const product = $derived(PRODUCTS.find((p) => p.id === (id as ProductId)));
  /** The Welcome tab is not a product. It gets Arbor's own mark and its own accent. */
  const isHome = $derived(id === 'home');
  const name = $derived(product?.name ?? surfaceDef(id).label);
  const role = $derived(isHome ? 'The suite, and what you last worked on' : (product?.role ?? ''));
  const accent = $derived(product?.accent ?? 'var(--accent)');

  const line = $derived(
    phase === 'backend' ? `Starting ${name}…` : 'Loading the interface…',
  );
</script>

<div class="sb" style={`--sb-accent: ${accent};`} role="status" aria-live="polite" aria-busy="true">
  <div class="sb-stage">
    <div class="sb-mark">
      <div class="sb-halo" aria-hidden="true"></div>
      <div class="sb-icon">
        {#if isHome}
          <ArborLogo size={52} />
        {:else}
          <ProductIcon {id} size={64} />
        {/if}
      </div>
    </div>

    <div class="sb-name">{name}</div>
    {#if role}<div class="sb-role">{role}</div>{/if}

    <div class="sb-track" aria-hidden="true"><div class="sb-bar"></div></div>
    <!-- The one thing that changes between the two phases. Keyed so the change is a crossfade
         rather than a jump: the screen is the same screen, and it should read that way. -->
    {#key line}
      <div class="sb-line">{line}</div>
    {/key}
  </div>
</div>

<style>
  .sb {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    /* The product's colour, at the strength of ambience rather than of decoration: enough that
       two tabs coming up look different, not enough to compete with the icon that names it. */
    background:
      radial-gradient(
        ellipse 70% 55% at 50% 42%,
        color-mix(in srgb, var(--sb-accent) 9%, transparent) 0%,
        transparent 70%
      ),
      var(--bg-elevated);
    user-select: none;
    -webkit-user-select: none;
  }

  .sb-stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 260px;
    max-width: 380px;
    padding: 0 24px;
    animation: sb-rise 260ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  .sb-mark {
    position: relative;
    width: 88px;
    height: 88px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 18px;
  }
  .sb-halo {
    position: absolute;
    inset: -6px;
    border-radius: 50%;
    background: radial-gradient(circle,
      color-mix(in srgb, var(--sb-accent) 26%, transparent) 0%,
      transparent 70%);
    animation: sb-pulse 2.4s ease-in-out infinite;
  }
  .sb-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--sb-accent);
    filter: drop-shadow(0 4px 14px color-mix(in srgb, var(--sb-accent) 35%, transparent));
  }

  .sb-name {
    font-size: 19px;
    font-weight: 300;
    letter-spacing: 3px;
    text-transform: uppercase;
    color: var(--text-primary);
    line-height: 1;
    margin-bottom: 7px;
  }
  .sb-role {
    font-size: var(--font-size-2xs);
    font-weight: 500;
    letter-spacing: 1.6px;
    text-transform: uppercase;
    color: var(--text-muted);
    text-align: center;
    margin-bottom: 26px;
  }

  .sb-track {
    position: relative;
    width: 100%;
    height: 2px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--bg-overlay) 70%, transparent);
    overflow: hidden;
  }
  /* Indeterminate on purpose, in both phases: neither the backend handshake nor a chunk fetch
     reports a fraction, and a bar that crawls to 90% and waits is a bar that lied. */
  .sb-bar {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 38%;
    border-radius: inherit;
    background: linear-gradient(90deg,
      transparent 0%,
      var(--sb-accent) 50%,
      transparent 100%);
    animation: sb-sweep 1.5s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  .sb-line {
    margin-top: 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    letter-spacing: 0.2px;
    animation: sb-fade 200ms ease both;
  }

  @keyframes sb-rise {
    from { opacity: 0; transform: translateY(6px) scale(0.99); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes sb-pulse {
    0%, 100% { transform: scale(0.94); opacity: 0.5; }
    50%      { transform: scale(1.06); opacity: 0.9; }
  }
  @keyframes sb-sweep {
    0%   { left: -38%; }
    100% { left: 100%; }
  }
  @keyframes sb-fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  @media (prefers-reduced-motion: reduce) {
    .sb-stage, .sb-halo, .sb-bar, .sb-line {
      animation: none !important;
    }
    /* Without the sweep the bar would be a static 38% stub that looks like a stalled
       determinate one. Full width and dimmed reads as "working" without moving. */
    .sb-bar {
      left: 0;
      width: 100%;
      opacity: 0.4;
    }
  }
</style>
