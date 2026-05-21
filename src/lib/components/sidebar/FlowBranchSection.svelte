<script lang="ts">
  import { CheckCircle, GitMerge, GitPullRequest } from 'lucide-svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';

  interface Props {
    kind:        'feature' | 'release' | 'hotfix';
    title:       string;
    dotClass:    'feature-dot' | 'release-dot' | 'hotfix-dot';
    icon:        any;
    names:       string[];
    currentName: string | null;
    forcedPr:    boolean;
    defaultPr:   boolean;
    busy:        boolean;
    onFinish:    (name: string, forcePr: boolean) => void;
  }

  let {
    kind, title, dotClass, icon: Icon, names, currentName,
    forcedPr, defaultPr, busy, onFinish,
  }: Props = $props();

  const tooltip = $derived(
    forcedPr || defaultPr ? `Finish ${kind} → PR/MR` : `Finish ${kind} locally`,
  );
</script>

<div class="branch-section">
  <div class="section-head">
    <span class="section-dot {dotClass}"></span>
    {title}
    <Badge variant="pill" size="sm" label={String(names.length)} />
  </div>
  {#each names as name}
    {@const isCurrent = currentName === name}
    <div class="branch-row" class:branch-row-current={isCurrent}>
      <Icon size={11} />
      <span class="branch-row-name">{name}</span>
      {#if isCurrent}
        <Badge variant="tone" tone="accent" size="sm" label="current" />
      {/if}
      <div class="row-finish-wrap" class:row-finish-wrap-visible={isCurrent}>
        <SplitButton
          variant="ghost"
          size="xs"
          direction="down"
          disabled={busy}
          title={tooltip}
          onclick={() => onFinish(name, forcedPr || defaultPr)}
          onselect={(id) => onFinish(name, id === 'pr')}
          options={forcedPr ? [] : [
            { id: 'normal', label: 'Finish locally',    icon: GitMerge },
            { id: 'pr',     label: 'Finish with PR/MR', icon: GitPullRequest },
          ]}
        >
          {#if forcedPr || defaultPr}
            <GitPullRequest size={11} />
          {:else}
            <CheckCircle size={11} />
          {/if}
        </SplitButton>
      </div>
    </div>
  {/each}
</div>

<style>
  .branch-section { padding: 0 0 4px; }
  .section-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px 3px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .section-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .feature-dot { background: var(--success); }
  .release-dot { background: var(--warning); }
  .hotfix-dot  { background: var(--error); }

  .branch-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 12px 3px 18px;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: var(--font-code);
    transition: background var(--transition-fast);
  }
  .branch-row:hover { background: rgba(255,255,255,0.03); }
  .branch-row-current { background: rgba(255,255,255,0.02); }

  .branch-row-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-finish-wrap {
    flex-shrink: 0;
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .branch-row:hover .row-finish-wrap,
  .row-finish-wrap-visible { opacity: 1; }
</style>
