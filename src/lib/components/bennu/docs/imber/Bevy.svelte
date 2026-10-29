<span class="eyebrow">Imber</span>
<h1>Imber: Bevy games and the profiler</h1>

<p class="doc-lead">
  Imber can look into a running Bevy game over its Remote Protocol, change it only if you allow that, and read the newest profiler recording to propose a performance change.
</p>

<h2>Looking into a running game</h2>
<p>
  The <em>Look into a running Bevy game</em> family (Settings › Imber › Tools it may use, for projects with a <code>Cargo.toml</code>) connects to a game over the Bevy Remote Protocol. The game needs <code>RemotePlugin</code> and <code>RemoteHttpPlugin</code>, and the connection is to this machine only.
</p>
<ul>
  <li><code>brp_connect</code>: connects, and waits for the game to come up if it is still starting.</li>
  <li><code>brp_query</code>: entities with their component values.</li>
  <li><code>brp_get</code>: one entity in full.</li>
  <li><code>brp_resources</code> and <code>brp_get_resource</code>: the resources and one resource's value.</li>
  <li><code>brp_types</code>: the type registry, so it knows the shape of a component before it asks for it.</li>
</ul>
<p>Reading never asks. The connection itself and the protocol are explained on the <strong>Bevy Remote</strong> page.</p>

<h2>Changing a running game</h2>
<p>
  <em>Let it change a running Bevy game</em> in Settings is a separate switch, off by default, and it needs the Bevy family on. It adds <code>brp_mutate</code>, <code>brp_insert</code>, <code>brp_remove</code>, <code>brp_spawn</code>, <code>brp_despawn</code> and <code>brp_set_resource</code>. These act on live state with no review and no undo. Imber asks once per conversation ("Let Imber change the running game?"), the yes covers every change in that conversation, and every change is listed in the run's summary. Autonomous mode does not skip the question.
</p>

<h2>Reading a profiler recording</h2>
<p>
  The project-intelligence family includes <code>bennu_profile_summary</code>: a compact summary of the newest recording the Profiler tool window made (the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses). Imber uses it to find the method worth optimising and to propose a change. It only reads: it never starts a recording, and with no recording it tells you to make one first. Recording is described on the <strong>Profiler</strong> page.
</p>
