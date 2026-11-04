<script lang="ts">
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Plans</h1>
<p>A <strong>plan</strong> runs remote git steps — push, create or merge a pull / merge request, start a CI pipeline, tag, pull — in an order you choose, across repositories and workspaces. It is built with buttons and the keyboard, not with a pipeline file.</p>

<h2>The shape of a plan</h2>
<ul>
  <li><strong>Phases</strong> are read left to right. The steps of one phase start together; steps on different repositories run side by side, steps on the same repository run one after the other.</li>
  <li>A phase never starts while the one before it has a step that is not done. The <strong>gate</strong> between two phases adds a condition on top of that: the MR created earlier is merged, a delay has passed, or you say so.</li>
  <li><strong>Finally</strong> holds steps that run when the phases have ended — and also when you stop a paused plan. Use it for tags or a closing pull.</li>
</ul>

<h2>Adding steps</h2>
<table class="shortcuts-table">
  <thead><tr><th>From</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td>Push button (arrow) · <Kbd action="plan_add_push" size="sm" /></td><td>Adds the current branch's push to the plan being composed instead of pushing now.</td></tr>
    <tr><td>Create PR dialog → <em>Add to plan</em></td><td>Keeps the form for later. When the branch is not on the remote yet, its push is added in a phase before.</td></tr>
    <tr><td>Commit button menu</td><td><em>Commit &amp; add push to plan</em> commits now and queues the push; <em>Commit, push &amp; MR to plan…</em> also opens the Create PR form. A commit is never deferred: it captures the staged files as they are now.</td></tr>
    <tr><td>Inside the plan: <Kbd keys={['A']} size="sm" /></td><td>Pick the action, then the repositories — several at once from any workspace. <Kbd keys={['Space']} size="sm" /> ticks a repository (or all of a workspace), <Kbd keys={['Enter']} size="sm" /> adds.</td></tr>
  </tbody>
</table>

<h2>The plan window</h2>
<p><Kbd action="plan_open" size="sm" /> or <strong>Plan: Open</strong> in the Command Palette opens it; the <strong>Plans</strong> tab of the Pipelines panel lists every plan whatever repository is open.</p>
<table class="shortcuts-table">
  <thead><tr><th>Key</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><Kbd keys={['←', '→', '↑', '↓']} size="sm" /></td><td>Move between steps</td></tr>
    <tr><td><Kbd keys={['A']} size="sm" /></td><td>Add a step to the current phase</td></tr>
    <tr><td><Kbd keys={['G']} size="sm" /></td><td>Change the condition that starts the current phase</td></tr>
    <tr><td><Kbd keys={['Alt', 'Arrow']} size="sm" /></td><td>Move the step within its phase or to the next one</td></tr>
    <tr><td><Kbd keys={['Del']} size="sm" /></td><td>Remove the step</td></tr>
    <tr><td><Kbd keys={['Ctrl', 'Enter']} size="sm" /></td><td>Run, or resume a paused plan</td></tr>
  </tbody>
</table>

<h2>Running</h2>
<p>A running plan is one entry in the <strong>Jobs</strong> panel, with a line per step. A plan cannot be edited while it runs.</p>
<ul>
  <li>When a step fails, the plan <strong>pauses</strong>: later phases are shown as waiting. Retry or skip the step, then resume. <em>Stop &amp; finish</em> runs the <strong>Finally</strong> steps and ends the plan.</li>
  <li>A gate waiting for a merge checks the provider every 30 seconds; <em>Skip wait</em> opens it at once. A <em>When I say so</em> gate pauses the plan until you press <em>Go ahead</em>.</li>
  <li>Plans are saved, and survive a restart. A step that was running when the app closed is shown as failed ("interrupted") so you can retry it.</li>
</ul>

<h2>Managing plans</h2>
<p>The <strong>Plans</strong> tab groups plans by what needs you: running, paused, drafts, finished, templates. A row runs or stops a plan, and deletes one (<Kbd keys={['Del']} size="sm" /> on the focused row, or <em>Delete</em> in the plan window) after a confirmation. A running plan cannot be deleted.</p>

<h2>Notes</h2>
<ul>
  <li>Steps use the same credentials and the same code as the buttons they stand for.</li>
  <li><em>Run pipeline</em> on GitHub needs a workflow; a plan does not choose one yet, so use it for GitLab or leave the pipeline to the CI tab.</li>
  <li><em>Merge MR</em> merges an MR a <em>Create MR</em> step opened in an earlier phase of the same plan.</li>
</ul>
