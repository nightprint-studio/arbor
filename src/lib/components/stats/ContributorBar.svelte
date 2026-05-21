<script lang="ts">
  import type { ContributorStat } from '$lib/types/git';
  import { tooltip } from '$lib/actions/tooltip';

  let { contributors }: { contributors: ContributorStat[] } = $props();

  const BAR_MAX_W = 260;  // px for 100%

  function initials(name: string): string {
    return name
      .split(/\s+/)
      .slice(0, 2)
      .map(w => w[0]?.toUpperCase() ?? '')
      .join('');
  }

  // Consistent color per email (deterministic hash → hue)
  function emailHue(email: string): number {
    let h = 0;
    for (let i = 0; i < email.length; i++) h = (h * 31 + email.charCodeAt(i)) & 0xffff;
    return h % 360;
  }
</script>

<div class="contributor-card">
  {#each contributors as c, i}
    {@const hue = emailHue(c.email)}
    <div class="contrib-row">
      <!-- Avatar -->
      <div
        class="avatar"
        style="background: hsl({hue},45%,32%);"
        use:tooltip={c.email}
      >
        <span class="avatar-initials">{initials(c.name) || '?'}</span>
      </div>

      <!-- Name + bar -->
      <div class="contrib-info">
        <div class="contrib-meta">
          <span class="contrib-name">{c.name || c.email}</span>
          <span class="contrib-pct">{c.percentage.toFixed(1)}%</span>
          <span class="contrib-count">{c.commit_count.toLocaleString()} commits</span>
        </div>
        <div class="bar-track">
          <div
            class="bar-fill"
            style="
              width: {c.percentage}%;
              background: hsl({hue},55%,50%);
            "
          ></div>
        </div>
      </div>

      <!-- Rank badge for top 3 -->
      {#if i < 3}
        <div class="rank rank-{i + 1}">{i === 0 ? '🥇' : i === 1 ? '🥈' : '🥉'}</div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .contributor-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 4px 0;
    display: flex;
    flex-direction: column;
  }

  .contrib-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .contrib-row:last-child {
    border-bottom: none;
  }

  .avatar {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .avatar-initials {
    font-size: 12px;
    font-weight: 700;
    /* Avatar initials sit on a coloured circle (workspace palette) — pick
       the same foreground the workspace dots use so light themes get
       readable dark initials and dark themes keep the white-ish ones. */
    color: color-mix(in srgb, var(--ws-color-fg, #ffffff) 90%, transparent);
    font-family: var(--font-ui-sans);
  }

  .contrib-info { flex: 1; min-width: 0; }

  .contrib-meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 5px;
  }

  .contrib-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .contrib-pct {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    font-family: var(--font-ui-sans);
  }
  .contrib-count {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    margin-left: auto;
  }

  .bar-track {
    height: 5px;
    background: var(--bg-hover);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    border-radius: var(--radius-sm);
    min-width: 2px;
    transition: width 0.4s cubic-bezier(0.16,1,0.3,1);
  }

  .rank {
    font-size: 18px;
    flex-shrink: 0;
  }
</style>
