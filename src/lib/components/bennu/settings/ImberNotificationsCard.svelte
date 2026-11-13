<script lang="ts">
  /**
   * Settings › Imber › Conversations: when Imber calls you through the system. A run can stop on a question for
   * as long as nobody looks at Bennu, so a waiting question is announced — only while no Bennu window has the
   * focus, since someone looking at it sees the card already.
   */
  import { Bell } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuAssistantStore as s } from '$lib/stores/bennu/assistant.svelte';
</script>

<div class="card">
  <div class="card-section-title"><Bell size={12} /> Notifications</div>
  <FormRow label="When it waits for you" description="A system notification, and the Arbor icon asking for attention, when Imber stops on a question, an approval or a card while Bennu is not the window in front. On by default.">
    <Toggle checked={s.notifyQuestions} onchange={(v) => s.setNotifyQuestions(v)} ariaLabel="Notify when Imber waits for an answer" />
  </FormRow>
  <FormRow label="When a run ends" description="The same when a run finishes or stops on an error while Bennu is not in front — for long work you leave running.">
    <Toggle checked={s.notifyDone} onchange={(v) => s.setNotifyDone(v)} ariaLabel="Notify when a run ends" />
  </FormRow>
</div>
