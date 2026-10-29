<script lang="ts">
  /**
   * Settings › Assistant — the key, the defaults, and what the assistant may reach.
   *
   * The key is real: it goes straight to the shell and into the assistant's own credential vault (apart
   * from the default one, which a long key would crowd), is never written to a config file and is never
   * held by the frontend after it is typed — the page only learns whether one is stored.
   */
  import { onMount } from 'svelte';
  import { KeyRound, Sliders, ShieldCheck, FolderOpen, X, Plus, Wallet, GitBranch, Wrench, WandSparkles } from 'lucide-svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { createModelsTemplate, deleteAssistantKey, hasAssistantKey, storeAssistantKey } from '$lib/ipc/bennu/assistant';
  import { formatEur, formatPrice, formatTokens } from '../assistant/assistant-format';
  import { MODES } from '../assistant/assistant-mode';
  import { DECISION_LEVELS, MAX_DECISION_NOTES } from '../assistant/assistant-decision';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import type { DecisionLevel } from '../assistant/assistant-decision';
  import type { AccessScope, AssistantEffort, AssistantMode } from '../assistant/assistant-types';

  const s = bennuAssistantStore;

  let key = $state('');
  let saved = $state(false);

  const modelOptions = $derived(s.models.map((m) => ({
    value: m.id,
    label: `${m.name} — ${formatPrice(m.inputPrice, m.outputPrice)}`,
  })));
  const sizeOptions = $derived(s.contextSizes.map((n) => ({
    value: n, label: formatTokens(n), disabled: n > s.model.contextMax,
  })));

  const EFFORT_OPTIONS = [
    { value: 'low', label: 'Low' }, { value: 'medium', label: 'Medium' }, { value: 'high', label: 'High' },
    { value: 'xhigh', label: 'Extra high' }, { value: 'max', label: 'Max' },
  ];

  /** The switchable groups of tools. A group that is off is not offered at all: it cannot be called, and its definitions cost nothing. */
  const TOOL_FAMILIES: { id: string; label: string; description: string }[] = [
    { id: 'read', label: 'Read and search files', description: 'Reading a file, searching text, finding files by name.' },
    { id: 'project', label: 'Project intelligence', description: "Bennu's own knowledge: symbols, usages, beans, endpoints, problems." },
    { id: 'edit', label: 'Edit files', description: 'Exact edits (one or several at once), replace across files, create and delete.' },
    { id: 'refactor', label: 'Refactoring', description: 'Rename a symbol and add a parameter, across every file that mentions it.' },
    { id: 'structural', label: 'Structural search and replace', description: 'Rewrite code by its shape — safer than a regular expression.' },
    { id: 'history', label: 'Local History', description: 'Read what a file used to say, and restore it or a deleted file.' },
    { id: 'run', label: 'Build, run and test', description: 'Anything that executes the project. Each asks first.' },
    { id: 'debug', label: 'Debugger', description: 'Set breakpoints and logpoints, run or attach to a Java program under the debugger (each launch asks first), read the stack and values at a stop, step. You watch the same stop in the Debug panel.' },
    { id: 'bevy', label: 'Look into a running Bevy game', description: 'Connect to a game over its Remote Protocol and read entities, components, resources and types. Only for projects with a Cargo.toml.' },
    { id: 'screenshot', label: 'Screenshots of a page', description: 'Look at a front end on localhost: a picture, plus console errors and failed requests with Playwright. Pictures are expensive in tokens; only the latest two stay in the conversation.' },
    { id: 'live', label: 'Live services', description: 'Start services of the project (each start asks first; a recipe once per file), read their logs, call them over localhost, follow a request through every log, run scenarios.' },
  ];

  const MODE_OPTIONS = MODES.map((m) => ({ value: m.id, label: m.label, description: m.description }));

  const SCOPES: { value: AccessScope; label: string; description: string }[] = [
    { value: 'file', label: 'This file', description: 'Only the file you are in.' },
    { value: 'project', label: 'This project', description: 'Every file and every Bennu tool of the project. The default.' },
    { value: 'workspace', label: 'All open projects', description: 'Across the projects in the workspace.' },
    { value: 'disk', label: 'Anywhere on disk', description: 'Beyond the workspace, without asking. Outside the other levels, one prompt decides read-only or read & write.' },
  ];

  let picking = $state(false);

  onMount(() => {
    void s.start();
    void s.refreshSkills();
    hasAssistantKey().then((has) => (saved = has)).catch(() => {});
  });

  async function save() {
    if (!key.trim()) return;
    try {
      await storeAssistantKey(key);
      saved = true;
      key = '';
      toastStore.show('Key stored in Imber’s vault.', 'success');
    } catch (e) {
      toastStore.show(`Could not store the key: ${e}`, 'error');
    }
  }

  const LIMIT_ROWS = [
    { period: 'session', key: 'session_eur', label: 'Per conversation', hint: 'The most one conversation may cost in total.' },
    { period: 'day', key: 'day_eur', label: 'Per day', hint: 'From midnight, your time.' },
    { period: 'week', key: 'week_eur', label: 'Per week', hint: 'From Monday.' },
    { period: 'month', key: 'month_eur', label: 'Per month', hint: 'From the first.' },
  ] as const;

  /** What has been spent against a limit, as a line under its field. */
  function spentLine(period: string, hint: string): string {
    const m = s.spend?.meters.find((x) => x.period === period);
    if (!m) return hint;
    const used = m.limitEur ? `${formatEur(m.spentEur)} of ${formatEur(m.limitEur)}` : `${formatEur(m.spentEur)} so far, no limit`;
    return `${hint} ${used}.`;
  }

  async function makePriceFile() {
    try {
      const path = await createModelsTemplate();
      toastStore.show(`Price list at ${path} — edit it, then reload.`, 'info');
      await s.reloadModels();
    } catch (e) {
      toastStore.show(`Could not create the price file: ${e}`, 'error');
    }
  }

  async function remove() {
    try {
      await deleteAssistantKey();
      saved = false;
    } catch (e) {
      toastStore.show(`Could not remove the key: ${e}`, 'error');
    }
  }
</script>

<div class="section-header">
  <h2>Imber</h2>
  <p>The key it uses, how it starts a conversation, and what it may reach.</p>
</div>

<div class="card">
  <div class="card-section-title"><KeyRound size={12} /> Connection</div>
  <Callout variant="info" title="Your key stays on this machine">
    The key lives in a vault of its own — apart from the one holding your git tokens — and never in a config file.
    Only the backend reads it, when a request is made.
  </Callout>
  <FormRow label="API key" description="Your Anthropic API key. Stored once, in the vault; shown here only as set or not set.">
    <div class="key">
      <Input type="password" bind:value={key} placeholder={saved ? '••••••••••••••••' : 'sk-ant-…'} />
      <Button variant="primary" size="sm" disabled={!key.trim()} onclick={save}>Save</Button>
      {#if saved}
        <Badge variant="tone" tone="success" size="sm">Set</Badge>
        <Button variant="ghost" size="sm" onclick={remove}>Remove</Button>
      {:else}
        <Badge variant="tone" tone="neutral" size="sm">Not set</Badge>
      {/if}
    </div>
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><Sliders size={12} /> Models and prices</div>
  {#if s.modelsError}
    <Callout variant="warning" title="Your price file could not be read">
      {s.modelsError} — the built-in list is in use.
    </Callout>
  {/if}
  <FormRow label="Price list" description="The built-in list of models and what they cost. To change a price or add a model, keep your own copy: it overrides the built-in entry with the same id.">
    <div class="key">
      <Button variant="secondary" size="sm" onclick={makePriceFile}>Create a price file</Button>
      <Button variant="ghost" size="sm" onclick={() => s.reloadModels()}>Reload</Button>
    </div>
  </FormRow>
  {#if s.modelsFile}<p class="file-note">{s.modelsFile}</p>{/if}
</div>

<div class="card">
  <div class="card-section-title"><GitBranch size={12} /> Git</div>
  <Callout variant="warning" title="Off unless you turn it on">
    With git on, Imber can read the repository's status, diff, log and blame, and stage and commit. What it
    reads includes commit messages and who wrote what, and that goes to the provider. A commit always asks first,
    in every mode. It cannot push.
  </Callout>
  <FormRow label="Let Imber use git" description="Status, diff, log, blame, stage, unstage and commit, through Corvus. Not available in a chat.">
    <Toggle checked={s.gitEnabled} onchange={(v) => s.setGitEnabled(v)} ariaLabel="Let Imber use git" />
  </FormRow>
  <FormRow label="Hide authors" description="The model sees each person as a made-up id like Author-7f3a9c — the same person is always the same id, so “who wrote most of this” still works. The ids are turned back into names in what you read, and in anything it sends to git (a commit message that credits Author-7f3a9c credits the real person). The mapping stays on this machine." >
    <Toggle checked={s.hideAuthors} disabled={!s.gitEnabled} onchange={(v) => s.setHideAuthors(v)} ariaLabel="Hide authors from the model" />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><Wallet size={12} /> Spending limits</div>
  <Callout variant="info" title="Brakes, not a budget">
    A conversation or a period at its limit does not start another request, and a run in progress stops before the one
    that would go over. Leave a limit at 0 to switch it off. Amounts are in euros; prices are in dollars, so they are
    converted at the rate below.
  </Callout>
  {#each LIMIT_ROWS as row (row.key)}
    <FormRow label={row.label} description={spentLine(row.period, row.hint)}>
      <NumberStepper value={s.limits[row.key]} min={0} step={1} ariaLabel={row.label}
                     onchange={(v) => s.setLimits({ [row.key]: Math.max(0, Number(v) || 0) })} />
    </FormRow>
  {/each}
  <FormRow label="Exchange rate" description="Euros per dollar. Kept by hand, so a limit never moves on its own.">
    <NumberStepper value={s.limits.eur_per_usd} min={0.01} step={0.01}
                   ariaLabel="Euros per dollar" onchange={(v) => s.setLimits({ eur_per_usd: Number(v) || 1 })} />
  </FormRow>
  <FormRow label="Warn from" description="The composer flags a limit once spending reaches this share of it.">
    <NumberStepper value={s.limits.warn_percent} min={10} max={99} step={5}
                   ariaLabel="Warning percentage" onchange={(v) => s.setLimits({ warn_percent: Math.min(99, Math.max(10, Number(v) || 80)) })} />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><Sliders size={12} /> New conversations</div>
  <FormRow label="Model" description="What a new conversation starts with. You can change it from the composer at any time.">
    <Select value={s.model.id} options={modelOptions} onchange={(v) => s.setModel(v)} ariaLabel="Model" />
  </FormRow>
  <FormRow label="Largest context window" description="The most Imber may put in one conversation. A bigger window costs more and answers slower.">
    <Select value={s.maxContext} options={sizeOptions} narrow onchange={(v) => s.setMaxContext(Number(v))} ariaLabel="Largest context window" />
  </FormRow>
  <FormRow label="Effort" description="How hard it thinks before answering. Higher is more thorough and costs more; the models that have no such setting ignore it.">
    <Select value={s.effort ?? ''} options={EFFORT_OPTIONS} narrow disabled={s.effortLevels.length === 0}
            placeholder="Not available" onchange={(v) => s.setEffort(v as AssistantEffort)} ariaLabel="Effort" />
  </FormRow>
  <FormRow label="Cheaper model for side jobs" description="Compacting a conversation is written by this model instead of the conversation's own, which is a fraction of the cost. Left on the default, it is the cheapest Haiku-class model in the price list. Also what read-only helper agents will use.">
    <Select value={s.sideModel} options={[{ value: '', label: 'Default (cheapest Haiku)' }, ...modelOptions]} onchange={(v) => s.setSideModel(String(v))} ariaLabel="Model for side jobs" />
  </FormRow>
  <FormRow label="Keep instructions cached for an hour" description="The instructions and tool definitions are kept in the provider's one-hour cache instead of the five-minute one. Costs a little more the first time, and saves the rewrite whenever you pause for more than five minutes between messages or open another conversation in the same project.">
    <Toggle checked={s.longCache} onchange={(v) => s.setLongCache(v)} ariaLabel="Keep instructions cached for an hour" />
  </FormRow>
  <FormRow label="Step plan" description="For work with several steps Imber writes a plan and ticks it off, with a percentage. Costs a few cents per such task; off removes the tool and its instruction.">
    <Toggle checked={s.planEnabled} onchange={(v) => s.setPlanEnabled(v)} ariaLabel="Show a step plan" />
  </FormRow>
  <FormRow label="Sub-agents" description="Lets Imber start helpers with a clean context for independent side tasks and exploration; only their summary comes back. They never do more than Imber can, ask you for anything beyond that, and cost is counted in the conversation. A conversation can switch them on or off for itself. Off by default.">
    <Toggle checked={s.subagentsEnabled} onchange={(v) => s.setSubagentsEnabled(v)} ariaLabel="Offer sub-agents" />
  </FormRow>
  <FormRow label="Wrap pasted code in a fenced block" description="Code pasted into the message box becomes a ```block``` with its language, shown highlighted in the conversation. Ctrl+Shift+V pastes plain text instead.">
    <Toggle checked={s.wrapPastedCode} onchange={(v) => s.setWrapPastedCode(v)} ariaLabel="Wrap pasted code in a fenced block" />
  </FormRow>
  <FormRow label="Project instructions"description="Read the project's CLAUDE.md, .claude/CLAUDE.md and AGENTS.md — and your own assistant-instructions.md in Bennu's config folder — into Imber's instructions, as other coding assistants do. Long files are cut; a copy of the same text is sent once.">
    <Toggle checked={s.useInstructions} onchange={(v) => s.setUseInstructions(v)} ariaLabel="Read project instruction files" />
  </FormRow>
  <FormRow label="Chat only" description="A conversation with no tools: it cannot read, search or change anything, and sees only what you type or select. Also switchable from the settings menu of the composer.">
    <Toggle checked={s.chatOnly} onchange={(v) => s.setChatOnly(v)} ariaLabel="Chat only" />
  </FormRow>
  <FormRow stacked label="Mode" description="How much it decides for itself. The default for new conversations; the composer changes it for the one you are in. Deleting, running programs and anything outside the workspace always ask, in every mode.">
    <RadioGroup value={s.mode} options={MODE_OPTIONS} appearance="card" size="sm" block onchange={(v) => s.setMode(v as AssistantMode)} />
  </FormRow>
  <FormRow stacked label="Decisions" description="How much it decides alone when a choice is yours. The default for new conversations; the composer's settings menu changes it for one.">
    <RadioGroup value={s.decisionLevel} options={DECISION_LEVELS.map((l) => ({ value: l.id, label: l.label, description: l.description }))} appearance="card" size="sm" block onchange={(v) => s.setDecisionLevel(v as DecisionLevel)} />
  </FormRow>
  <FormRow stacked label="Your instructions about decisions" description="Free text, at any level: for example 'always ask before touching public API' or 'never add a dependency'. A conversation can add its own, appended to this. Up to {MAX_DECISION_NOTES} characters.">
    <Textarea framed maxLength={MAX_DECISION_NOTES} bind:value={() => s.decisionNotes, (v) => s.setDecisionNotes(v)} placeholder="Nothing extra" minRows={3} maxRows={8} ariaLabel="Instructions about decisions" />
  </FormRow>
  <FormRow label="Debug session idle time" description="A program Imber starts under the debugger stays alive between messages. It is ended (stopped, or detached and resumed) after this many minutes with no debug tool used, when it exits, or when the project closes. The Run console shows how long is left, with a Stop button.">
    <NumberStepper value={s.debugIdleMinutes} min={5} max={240} step={5} ariaLabel="Minutes before an unused debug session ends" onchange={(v) => s.setDebugIdleMinutes(Number(v) || 30)} />
  </FormRow>
  <FormRow label="Confirm costly changes" description="Changing the model or the context window in a long conversation makes it re-read its history at full price. Ask first, with the estimate.">
    <Toggle checked={s.confirmCostChanges} onchange={(v) => s.setConfirmCostChanges(v)} ariaLabel="Confirm model and context changes" />
  </FormRow>
  {#if s.mode === 'autonomous'}
    <Callout variant="warning" title="Autonomous opens files by itself">
      When its own build reports an error in a file of the project, it reads and fixes that file without asking — up to 25 files a run, never build output, dependency manifests or secrets. Each one is shown in the thread and can be revoked, and the access ends with the run.
    </Callout>
  {/if}
</div>

<div class="card">
  <div class="card-section-title"><ShieldCheck size={12} /> What it may reach</div>
  <Callout variant="info" title="Narrow by default, never a shell">
    It works through Bennu's own tools — files, search, usages, beans, endpoints, rename, add parameter — and a few text
    tools of its own. It cannot run a program, and <strong>deleting</strong> follows the mode: free in its Temp folder, free inside what you allowed unless the mode is Manual, always asked outside it.
  </Callout>
  <FormRow stacked label="Reach" description="Where it may read and change. The composer shows it and lets you narrow it for one conversation.">
    <RadioGroup value={s.scope} options={SCOPES} appearance="card" size="sm" block onchange={(v) => s.setScope(v as AccessScope)} />
  </FormRow>
  <FormRow label="Projects at once" description="With the reach set to all open projects, how many of them one conversation works across — the one you are in first. It reads, searches and changes in all of them; each is named in its instructions, so a higher number costs a little on every request.">
    <NumberStepper value={s.maxProjects} min={1} max={20} step={1} ariaLabel="Projects at once"
                   onchange={(v) => s.setMaxProjects(Number(v) || 6)} />
  </FormRow>
  <FormRow label="Edit files" description="Off, and it can only read and answer.">
    <Toggle checked={s.allowEdits} onchange={(v) => s.setAllowEdits(v)} ariaLabel="Edit files" />
  </FormRow>
  <FormRow label="Use the Temp folders" description="Scratch files and downloaded logs live outside the project; it may use these folders and nothing else beyond it.">
    <Toggle checked={s.allowTemp} onchange={(v) => s.setAllowTemp(v)} ariaLabel="Use the Temp folders" />
  </FormRow>

  <div class="folders">
    <div class="folders-head">
      <span><FolderOpen size={12} /> Folders it may work in, besides the project</span>
      <Button variant="ghost" size="xs" onclick={() => (picking = true)}>
        <Plus size={11} /> Add folder…
      </Button>
    </div>
    {#each s.extraFolders as f (f)}
      <div class="folder">
        <span class="path">{f}</span>
        <IconButton tooltip="Remove" size={20} onclick={() => s.removeFolder(f)}><X size={11} /></IconButton>
      </div>
    {:else}
      <p class="empty">None — only the project.</p>
    {/each}
  </div>
</div>

<div class="card">
  <div class="card-section-title"><Wrench size={12} /> Tools it may use</div>
  {#each TOOL_FAMILIES as f (f.id)}
    <FormRow label={f.label} description={f.description}>
      <Toggle checked={s.familyOn(f.id)} onchange={(v) => s.setFamily(f.id, v)} ariaLabel={f.label} />
    </FormRow>
  {/each}
  <FormRow label="Let it change a running Bevy game" description="Mutate, insert, remove, spawn, despawn, set a resource. Off until you turn it on: the game is live state with no review and no undo. Even then it asks once per conversation, and every change is listed in the run's summary.">
    <Toggle checked={s.bevyWrite} disabled={!s.familyOn('bevy')} onchange={(v) => s.setBevyWrite(v)} ariaLabel="Let Imber change a running Bevy game" />
  </FormRow>
  <FormRow label="Let it change the program it debugs" description="Throw an exception in it, make a method return early, drop a frame, write a value. Off until you turn it on: the program changes while it runs and there is no undo. Even then it asks once per conversation, and every change is listed in the thread and in the Debug panel. Autonomous mode does not skip the question.">
    <Toggle checked={s.debugMutate} disabled={!s.familyOn('debug')} onchange={(v) => s.setDebugMutate(v)} ariaLabel="Let Imber change the program it debugs" />
  </FormRow>
  <FormRow label="Start a run when the program stops" description="When the program a conversation is debugging stops at a breakpoint and Imber is idle, start a run by itself to look at it (at most 3 an hour per conversation). It spends money and counts toward your limits, so it is off by default; a conversation can turn it on or off for itself.">
    <Toggle checked={s.autoContinueOnStop} onchange={(v) => s.setAutoContinueOnStop(v)} ariaLabel="Start an Imber run when the debugged program stops" />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><WandSparkles size={12} /> Skills</div>
  <FormRow label="Skills" description="Folders with a SKILL.md — in the project's .bennu/skills or .claude/skills, or in your own — that teach Imber how you want a kind of task done. None is known to it until you enable it; it sees only a one-line index and loads a skill when a task calls for it. Type / in the composer to use one.">
    <div class="key">
      <Badge variant="count" size="sm" label={`${s.skillCounts.on} of ${s.skillCounts.total} enabled`} />
      <Button variant="secondary" size="sm" onclick={() => s.openSkills()}>Skills…</Button>
    </div>
  </FormRow>
</div>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Add a folder Imber may work in"
    onConfirm={(path) => { s.addFolder(path); picking = false; }}
    onCancel={() => (picking = false)}
  />
{/if}

<style>
  .file-note { margin: 0; padding: 0 14px 10px; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); overflow-wrap: anywhere; }
  .key { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .key :global(input) { min-width: 220px; }

  .folders { display: flex; flex-direction: column; gap: 4px; padding: 10px 14px 12px; }
  .folders-head { display: flex; align-items: center; justify-content: space-between; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .folders-head span { display: inline-flex; align-items: center; gap: 6px; }
  .folder {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 4px 6px 4px 10px; border-radius: var(--radius-sm);
    background: var(--bg-overlay);
  }
  .path { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .empty { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
