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
  The project-intelligence family includes three read-only profiler tools. <code>bennu_profile_recordings</code> lists the recordings the Profiler tool window made. <code>bennu_profile_diff</code> compares two of them method by method (the share of the run each took before and after, biggest changes first), to check that a change made the code faster. <code>bennu_profile_summary</code> is a compact summary of one recording (the newest unless Imber names another): the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses. Imber uses it to find the method worth optimising and to propose a change. These three only read: with no recording they tell you to make one first. Recording by hand is described on the <strong>Profiler</strong> page; Imber recording for you is below.
</p>

<h2>Recording a running JVM</h2>
<p>
  The <strong>Profile</strong> family (<code>profile_start</code>, <code>profile_stop</code>) lets Imber record a JVM that is already running on this machine with Flight Recorder, through <code>jcmd</code>. <strong>Imber asks at every start</strong>, naming the JVM, because a recording samples a running program (nothing in it is changed, it runs a little slower). Imber names the JVM by process id or by part of its main class; with neither, it gets the list to choose from. One recording per conversation, started in one message and stopped in a later one. <code>profile_stop</code> ends the recording, waits until it has been read and saved, and returns its summary; the recording is then in the Profiler window like any other. A recording keeps sampling until it is stopped. Sub-agents never have these tools, and the family can be switched off like any other.
</p>
