<script module lang="ts">
  export interface CommitRequest {
    message: string;
    amend: boolean;
    /** Commit & Push was chosen. Only ever true when `allowPush` is set. */
    push: boolean;
    /** Commit now, then hand the remote steps to a plan instead of running them. Only set when `allowPlan` is. */
    plan?: 'push' | 'push_mr';
  }
</script>

<script lang="ts">
  /**
   * The commit box under the staging panel: message, template restore, *Amend last* and the
   * Commit split button. Props-driven — it knows nothing about tabs, repos or stores. The
   * caller turns `onSubmit` into a real commit and answers whether it landed; only then is the
   * form reset, so a failed commit never loses the message.
   */
  import type { Snippet } from 'svelte';
  import { FileText, GitCommitHorizontal, GitPullRequest, ListChecks, Upload } from 'lucide-svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';

  interface Props {
    /** Files in the index — a commit needs at least one unless amending. */
    stagedCount: number;
    /** Text to pre-fill an empty message with (git's commit.template, or a global fallback). */
    template?: string;
    /** Offer *Commit & Push* next to *Commit*. */
    allowPush?: boolean;
    /** Offer the *…to plan* variants: the commit runs now, the push (and MR) are queued in a plan. */
    allowPlan?: boolean;
    /** Resolves `true` once the commit landed. */
    onSubmit: (req: CommitRequest) => Promise<boolean>;
    /** Extra buttons next to the commit button (plugin contributions). */
    extra?: Snippet;
  }

  let { stagedCount, template = '', allowPush = false, allowPlan = false, onSubmit, extra }: Props = $props();

  let message    = $state('');
  let amend      = $state(false);
  let committing = $state(false);
  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);

  const canCommit = $derived(message.trim().length > 0 && (stagedCount > 0 || amend));

  // Pre-fill once the template arrives, but never over something the user already typed.
  $effect(() => {
    if (template && !message) message = template;
  });

  // The palette's "Commit" / "Amend Last Commit" land here.
  $effect(() => {
    function onFocus(e: Event) {
      if ((e as CustomEvent<{ amend?: boolean }>).detail?.amend) amend = true;
      textareaEl?.focus();
      // Caret at the end so template text isn't overwritten on type.
      if (textareaEl) textareaEl.setSelectionRange(textareaEl.value.length, textareaEl.value.length);
    }
    window.addEventListener('arbor:focus-commit-form', onFocus);
    return () => window.removeEventListener('arbor:focus-commit-form', onFocus);
  });

  async function submit(push: boolean, plan?: CommitRequest['plan']) {
    if (!canCommit || committing) return;
    committing = true;
    try {
      const done = await onSubmit({ message: message.trim(), amend, push: push && allowPush, plan: allowPlan ? plan : undefined });
      if (done) {
        message = '';
        amend = false;
      }
    } finally {
      committing = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void submit(e.shiftKey);
    }
  }

  const placeholder = $derived(
    allowPush
      ? 'Commit message (Ctrl+Enter to commit, Ctrl+Shift+Enter to commit & push)'
      : 'Commit message (Ctrl+Enter to commit)',
  );
</script>

<div class="commit-form">
  <div class="textarea-wrap">
    <textarea
      bind:this={textareaEl}
      class="message-input"
      {placeholder}
      bind:value={message}
      onkeydown={onKeydown}
      rows="3"
    ></textarea>
    {#if template && message !== template}
      <span class="template-btn">
        <IconButton size={20} tooltip="Apply commit template" onclick={() => (message = template)}>
          <FileText size={11} />
        </IconButton>
      </span>
    {/if}
  </div>
  <div class="form-footer">
    <Checkbox bind:checked={amend} label="Amend last" />
    {@render extra?.()}
    {#if allowPush}
      <SplitButton
        label={committing ? 'Committing…' : 'Commit'}
        variant="primary"
        direction="up"
        disabled={!canCommit || committing}
        tooltip={tooltipForAction('Commit', 'commit')}
        options={[
          { id: 'commit',      label: 'Commit',        icon: GitCommitHorizontal, iconColor: 'var(--success)', action: 'commit' },
          { id: 'commit-push', label: 'Commit & Push', icon: Upload,              iconColor: 'var(--accent)',  action: 'commit_and_push' },
          ...(allowPlan ? [
            { id: 'plan-group',       label: 'Plan', group: true },
            { id: 'commit-plan-push', label: 'Commit & add push to plan', icon: ListChecks,     iconColor: 'var(--accent)', description: 'Commit now; the push joins the plan' },
            { id: 'commit-plan-mr',   label: 'Commit, push & MR to plan…', icon: GitPullRequest, iconColor: 'var(--accent)', description: 'Commit now; push and merge request join the plan' },
          ] : []),
        ]}
        onclick={() => submit(false)}
        onselect={(id) => submit(id === 'commit-push', id === 'commit-plan-push' ? 'push' : id === 'commit-plan-mr' ? 'push_mr' : undefined)}
      />
    {:else}
      <Button variant="primary" disabled={!canCommit || committing} onclick={() => submit(false)}>
        {committing ? 'Committing…' : 'Commit'}
      </Button>
    {/if}
  </div>
</div>

<style>
  .commit-form { padding: 8px 8px 6px; display: flex; flex-direction: column; gap: 6px; }
  .textarea-wrap { position: relative; }

  .message-input {
    width: 100%;
    resize: none;
    padding: 6px 8px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    transition: border-color var(--transition-fast);
    box-sizing: border-box;
  }
  .message-input:focus { outline: none; border-color: var(--border-focus); }
  .message-input::placeholder { color: var(--text-disabled); }

  /* Template restore button — top-right corner of the textarea. */
  .template-btn { position: absolute; top: 4px; right: 4px; opacity: 0.6; transition: opacity var(--transition-fast); }
  .textarea-wrap:hover .template-btn,
  .template-btn:focus-within { opacity: 1; }

  .form-footer { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: var(--font-size-xs); }
</style>
