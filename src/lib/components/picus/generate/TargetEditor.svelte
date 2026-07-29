<script lang="ts">
  /**
   * Target editor — one row per destination file, expandable into its rules.
   *
   * This is the component the whole generator hinges on, so it is deliberately
   * explicit:
   *
   *  • The **collapsed** row says everything you need to decide whether to look
   *    closer: dialect, role, path, and a compact summary of the active rules
   *    ("procedural block", "4.12 → 4.13").
   *  • The **expanded** row states, next to every switch, what it turns into in
   *    the emitted SQL — `DECLARE … BEGIN … END; /` versus `DO $$ … END $$;`,
   *    `USER_TABLES` versus `to_regclass`. A rule you cannot picture is a rule
   *    you will get wrong.
   *  • Rule **dependencies apply themselves, visibly**: switching the version
   *    guard on switches the procedural block on (a guard needs somewhere to
   *    live); switching the block off drops the guard.
   *  • "Copy these rules" only reaches destinations with the **same role** —
   *    never from initialisation to update, where a version guard would be
   *    nonsense.
   *  • A rule set the emitter **cannot honour** says so, on the row it belongs
   *    to. One rule genuinely constrains another — a version guard needs a block
   *    to return from — and the emitter reports the clash rather than quietly
   *    dropping the guard, which would leave a destination looking guarded while
   *    running unconditionally.
   */
  import { ChevronRight, Check, Copy, Eye, RotateCcw, Trash2 } from 'lucide-svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusSettingsStore } from '$lib/stores/picus/settings.svelte';
  import { GENERIC_ENGINE, type Target } from '$lib/types/picus';

  /** What a destination that says nothing about it falls back to. */
  const projectFilter = $derived(picusSettingsStore.versionTable.filter);

  /**
   * What each rule turns into, per dialect — shown inline, never hidden.
   *
   * A **portable** destination has no dialect-specific spelling for any of these,
   * which is precisely why it accepts none of them: the two engines write a
   * procedural block, an existence check and a savepoint in ways the other cannot
   * parse. The hints say that rather than picking one engine's answer, and the
   * controls below are disabled to match — the backend refuses them anyway, and a
   * control that can only produce a refusal should not be operable.
   */
  const PORTABLE_BLOCK_HINT = 'no form runs on both engines';

  function wrapHint(t: Target): string {
    if (t.dialect === GENERIC_ENGINE) return PORTABLE_BLOCK_HINT;
    return t.dialect === 'oracle' ? 'DECLARE … BEGIN … END; /' : 'DO $$ … END $$;';
  }
  function objectHint(t: Target): string {
    if (t.dialect === GENERIC_ENGINE) return 'needs a block, which a portable script cannot have';
    return t.dialect === 'oracle' ? 'checked against USER_TABLES' : 'checked with to_regclass';
  }
  function txHint(t: Target): string {
    if (t.dialect === GENERIC_ENGINE) return 'needs a block, which a portable script cannot have';
    return t.dialect === 'oracle'
      ? 'SAVEPOINT + ROLLBACK TO on error'
      : 'the DO block is already one transaction';
  }

  /** Portable destinations take plain statements and nothing that needs a block. */
  const portable = (t: Target) => t.dialect === GENERIC_ENGINE;
</script>

{#each dmlStore.targets as target (target.id)}
  {@const expanded = dmlStore.expandedTargetId === target.id}
  {@const conflict = dmlStore.ruleConflictFor(target.id)}
  <div class="te-row" class:te-off={!target.enabled}>
    <!-- Collapsed head: the whole row expands; the checkbox arms the target. -->
    <div class="te-head">
      <button
        type="button"
        class="te-check"
        class:te-on={target.enabled}
        aria-pressed={target.enabled}
        aria-label={`${target.enabled ? 'Disable' : 'Enable'} ${target.file}`}
        onclick={() => dmlStore.toggleTarget(target.id)}
      >
        {#if target.enabled}<Check size={11} />{/if}
      </button>

      <button
        type="button"
        class="te-main"
        aria-expanded={expanded}
        onclick={() => dmlStore.expandTarget(target.id)}
      >
        <PicusDialectChip engine={target.dialect} />
        <PicusRoleChip role={target.role} />
        <span class="te-path">{target.file}</span>
        <span class="te-summary">
          <Badge
            variant="tone"
            tone={target.wrap === 'block' ? 'accent' : 'neutral'}
            size="sm"
            label={target.wrap === 'block' ? 'procedural block' : 'bare statements'}
          />
          {#if target.guards.version}
            <Badge
              variant="tone"
              tone="warning"
              size="sm"
              label={`${target.guards.version.from || '?'} → ${target.guards.version.to || '?'}`}
            />
          {/if}
          {#if target.guards.skipIfPresent}
            <Badge variant="tone" tone="neutral" size="sm" label="skip existing" />
          {/if}
          {#if conflict}
            <span use:tooltip={conflict}>
              <Badge variant="tone" tone="error" size="sm" label="rule conflict" />
            </span>
          {/if}
        </span>
        <span class="te-twist" class:te-twist-open={expanded}><ChevronRight size={13} /></span>
      </button>
    </div>

    {#if expanded}
      <div class="te-rules">
        {#if conflict}
          <div class="te-conflict">
            <Alert variant="warning" compact text={conflict} />
          </div>
        {/if}

        <div class="te-rule">
          <Toggle
            checked={target.wrap === 'block'}
            size="sm"
            disabled={portable(target)}
            label="Wrap in a procedural block"
            ariaLabel="Wrap in a procedural block"
            onchange={(on) => dmlStore.setWrap(target.id, on ? 'block' : 'plain')}
          />
          <span class="te-why">{wrapHint(target)}</span>
        </div>

        <div class="te-rule">
          <Toggle
            checked={!!target.guards.version}
            size="sm"
            disabled={portable(target)}
            label="Run only when the database is at version"
            ariaLabel="Version guard"
            onchange={(on) => dmlStore.setVersionGuard(target.id, on)}
          />
          {#if target.guards.version}
            <span class="te-inline">
              <Input
                value={target.guards.version.from}
                size="sm"
                narrow
                block={false}
                ariaLabel="Starting version"
                placeholder="4.12"
                oninput={(v) => dmlStore.setVersionBound(target.id, 'from', String(v))}
              />
              <span class="te-inline-text">and carry it to</span>
              <Input
                value={target.guards.version.to}
                size="sm"
                narrow
                block={false}
                ariaLabel="Resulting version"
                placeholder="4.13"
                oninput={(v) => dmlStore.setVersionBound(target.id, 'to', String(v))}
              />
            </span>
          {/if}
          <span class="te-why">
            {#if target.wrap === 'plain'}
              needs the procedural block — switching this on turns it on
            {:else}
              reads the version table, returns early on a mismatch
            {/if}
          </span>

          {#if target.guards.version}
            <!-- Which ROW of the version table, for a repository that installs
                 several products into one. Shown only under the guard, because
                 outside it there is nothing that reads or stamps a version — an
                 always-visible field here would be a question with no consequence.
                 Pre-filled from the destination folder's declared product; typing
                 in it is the one-off escape hatch. -->
            <div class="te-sub">
              <span class="te-sub-label">Version row</span>
              <Input
                value={target.versionFilter ?? ''}
                size="sm"
                block={false}
                ariaLabel="Predicate selecting this destination's version row"
                placeholder={projectFilter || 'the table holds one row'}
                oninput={(v) => dmlStore.setVersionFilter(target.id, String(v))}
              />
              <span class="te-why">
                {#if target.versionFilter === undefined}
                  the project's own — this destination says nothing about it
                {:else if target.versionFilter.trim() === ''}
                  no predicate: reads and stamps the table's only row
                {:else}
                  <code>WHERE {target.versionFilter}</code>
                {/if}
              </span>
              {#if target.versionFilter !== undefined}
                <Button
                  variant="ghost"
                  size="xs"
                  tooltip={'Go back to the project’s own predicate'}
                  onclick={() => dmlStore.setVersionFilter(target.id, null)}
                >
                  Clear
                </Button>
              {/if}
            </div>
          {/if}
        </div>

        <div class="te-rule">
          <Toggle
            checked={target.guards.skipIfPresent}
            size="sm"
            label="Skip rows that are already there"
            ariaLabel="Skip existing rows"
            onchange={(on) => dmlStore.setGuard(target.id, 'skipIfPresent', on)}
          />
          <span class="te-why">
            checked on the comparison key ({dmlStore.keyColumns.map((c) => c.name).join(', ') || 'none selected'})
          </span>
        </div>

        <div class="te-rule">
          <Toggle
            checked={target.guards.requireObject}
            size="sm"
            disabled={portable(target)}
            label="Stop if the table doesn't exist"
            ariaLabel="Require the table to exist"
            onchange={(on) => dmlStore.setGuard(target.id, 'requireObject', on)}
          />
          <span class="te-why">{objectHint(target)}</span>
        </div>

        <div class="te-rule">
          <Toggle
            checked={target.guards.transactional}
            size="sm"
            disabled={target.dialect === 'postgres' || portable(target)}
            label="Savepoint and roll back on error"
            ariaLabel="Transactional"
            onchange={(on) => dmlStore.setGuard(target.id, 'transactional', on)}
          />
          <span class="te-why">{txHint(target)}</span>
        </div>

        <div class="te-actions">
          <Button
            variant="secondary"
            size="xs"
            tooltip={`Applies to the other “${target.role}” destinations only — never across roles`}
            onclick={() => {
              const n = dmlStore.copyRulesToSameRole(target.id);
              toastStore.show(
                n ? `Rules copied to ${n} other ${target.role} destination${n === 1 ? '' : 's'}.`
                  : `No other ${target.role} destination to copy to.`,
                n ? 'success' : 'info',
              );
            }}
          >
            {#snippet iconStart()}<Copy size={12} />{/snippet}
            Copy these rules to the same role
          </Button>
          <Button
            variant="ghost"
            size="xs"
            title="Reset this destination to its role's preset"
            onclick={() => dmlStore.resetTargetToPreset(target.id)}
          >
            {#snippet iconStart()}<RotateCcw size={12} />{/snippet}
            Reset to preset
          </Button>
          <span class="te-spacer"></span>
          <Button variant="ghost" size="xs" title="Show this destination in the preview" onclick={() => dmlStore.setPreviewTarget(target.id)}>
            {#snippet iconStart()}<Eye size={12} />{/snippet}
            Preview
          </Button>
          <Button
            variant="ghost"
            size="xs"
            tooltip={'Remove this destination — the file itself is untouched'}
            ariaLabel="Remove this destination"
            onclick={() => {
              dmlStore.removeTarget(target.id);
              toastStore.show(`${target.file} is no longer a destination.`, 'info');
            }}
          >
            {#snippet iconStart()}<Trash2 size={12} />{/snippet}
            Remove
          </Button>
        </div>
      </div>
    {/if}
  </div>
{/each}

<style>
  .te-row { border-bottom: 1px solid var(--border-subtle); }
  .te-row:last-child { border-bottom: none; }
  .te-off { opacity: 0.5; }

  .te-head { display: flex; align-items: center; gap: 9px; padding: 0 12px; }

  .te-check {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    padding: 0;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: transparent;
    cursor: pointer;
  }
  .te-check:hover { border-color: var(--border-focus); }
  .te-check.te-on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--text-on-accent);
  }

  .te-main {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 9px 0;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .te-main:hover .te-path { color: var(--text-primary); }

  .te-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .te-summary { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; }

  .te-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .te-twist-open { transform: rotate(90deg); }

  /* Expanded rules sit on a slightly recessed ground, indented past the checkbox
     so they read as belonging to the row above. */
  .te-rules {
    padding: 6px 14px 12px 46px;
    background: var(--bg-input);
    border-top: 1px dashed var(--border-subtle);
  }

  /* The emitter's own verdict on this rule set — above the switches that caused
     it, so the fix is the next thing under the cursor. */
  .te-conflict { padding: 6px 0 2px; }

  .te-rule {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 5px 0;
    font-size: 11.5px;
  }

  .te-inline { display: inline-flex; align-items: center; gap: 7px; }
  .te-inline-text { color: var(--text-secondary); }

  /* A field that belongs to the rule above it — indented so the rule still reads
     as one thing rather than as two switches at the same level. */
  .te-sub {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    width: 100%;
    padding-left: 26px;
    margin-top: 6px;
  }
  .te-sub-label { font-size: 11.5px; color: var(--text-muted); }
  .te-sub code { font-family: var(--font-code); }

  /* The translation note: what this switch becomes in the emitted SQL. */
  .te-why {
    margin-left: auto;
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-disabled);
    white-space: nowrap;
  }

  .te-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }
  .te-spacer { flex: 1; }
</style>
