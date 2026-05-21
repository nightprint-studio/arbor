<h1>Git Bisect</h1>

<p class="doc-lead">
  <strong>Git Bisect</strong> uses binary search to find the exact commit that introduced a bug.
  You tell Arbor which commit is bad and which is good — the bisect engine narrows the range in
  <em>O(log n)</em> steps until it pinpoints the culprit.
</p>

<div class="callout info">
  Arbor runs bisect in <strong>no-checkout mode</strong> — your working tree is never touched.
  Mark commits based on knowledge or history, and use the <em>Checkout</em> button only when you
  actually need to run tests against a specific commit.
</div>

<h2>Starting a session</h2>
<ol>
  <li>Right-click the commit you know is <strong>bad</strong> in the graph → <em>Bisect — Mark as Bad</em>.</li>
  <li>A banner appears at the top of the graph asking you to select a good commit.</li>
  <li>Right-click any commit you know was <strong>good</strong> → <em>Bisect — Mark as Good</em>.</li>
  <li>Arbor computes the midpoint and the banner updates to show the next commit to test.</li>
</ol>

<h2>The bisect banner</h2>
<p>The banner changes appearance based on the current state:</p>
<table class="shortcuts-table">
  <thead><tr><th>State</th><th>What you see</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>Waiting for good</strong></td>
      <td>Gray banner — "right-click a known good commit in the graph". No midpoint is shown yet.</td>
    </tr>
    <tr>
      <td><strong>Midpoint ready</strong></td>
      <td>Accent banner — shows the next commit hash and approximate remaining steps. Action buttons: <em>Checkout, Good, Bad, Skip, Undo, Save &amp; Pause, Reset</em>.</td>
    </tr>
    <tr>
      <td><strong>Result found</strong></td>
      <td>Red banner — "First bad commit found" with the culprit hash (click to scroll to it in the graph). The session is auto-saved.</td>
    </tr>
  </tbody>
</table>

<h2>Action buttons</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Checkout</div>
    <div class="fc-desc">Switches your working tree to the current midpoint so you can run tests. Optional — skip it if you can judge the commit from its diff or history.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Good / Bad</div>
    <div class="fc-desc">Mark the current midpoint. The graph scrolls automatically to the next commit to test.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Skip</div>
    <div class="fc-desc">Skip a commit you cannot test (e.g. broken build). Available only after a good commit has been selected.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Undo</div>
    <div class="fc-desc">Reverts the last mark by replaying the bisect log without the final command. Available as long as there is at least one mark.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Save &amp; Pause</div>
    <div class="fc-desc">Saves the session to <code>.arbor/bisect/</code> and resets git bisect so you can do other work. Resume at any time from the sidebar.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Reset</div>
    <div class="fc-desc">Ends the current bisect session without saving. Git restores the original HEAD.</div>
  </div>
</div>

<h2>Graph indicators</h2>
<p>Commits involved in the bisect session are highlighted with colored rings in the graph:</p>
<table class="shortcuts-table">
  <thead><tr><th>Ring</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><span style="color:#e05252">■</span> Red solid</td><td>Marked as <strong>Bad</strong>. All bad commits keep their ring throughout the session.</td></tr>
    <tr><td><span style="color:#3fb950">■</span> Green solid</td><td>Marked as <strong>Good</strong>.</td></tr>
    <tr><td><span style="color:#e0a93a">■</span> Orange dashed (pulsing)</td><td>Current midpoint — <strong>next commit to test</strong>.</td></tr>
    <tr><td><span style="color:#e05252">■</span> Red double-glow (pulsing)</td><td><strong>Result</strong> — the first bad commit found.</td></tr>
  </tbody>
</table>

<h2>Bisect sessions</h2>
<p>
  Sessions are stored under <code>.arbor/bisect/&lt;id&gt;/session.json</code> inside your repository.
  The <strong>Bisect Sessions</strong> collapsible appears in the sidebar whenever at least one session exists.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Description</th></tr></thead>
  <tbody>
    <tr>
      <td><strong>▶ Play</strong></td>
      <td>Replays all marks from the session. For paused sessions this restores the midpoint and scrolls to it. For completed sessions it reloads the result state and rings into the graph.</td>
    </tr>
    <tr>
      <td><strong>⌖ Go to result</strong></td>
      <td>Scrolls the graph to the result commit (completed sessions only).</td>
    </tr>
    <tr>
      <td><strong>✎ Rename</strong></td>
      <td>Click the pencil icon and type a new name. Press Enter or click away to confirm, Escape to cancel.</td>
    </tr>
    <tr>
      <td><strong>✕ Delete</strong></td>
      <td>Removes the session directory permanently.</td>
    </tr>
  </tbody>
</table>

<div class="callout tip">
  <strong>Auto-save on result</strong> — when bisect finds the culprit commit, the session is saved
  automatically with a name like <em>"Found: abc1234 — commit message"</em>. You never lose a
  completed bisect result.
</div>

<h2>Under the hood</h2>
<p>The backend runs <code>git bisect start --no-checkout</code> and manages <code>BISECT_HEAD</code>
directly. State is read from <code>.git/BISECT_LOG</code> and <code>.git/BISECT_HEAD</code>:</p>
<table class="shortcuts-table">
  <thead><tr><th>File</th><th>Content</th></tr></thead>
  <tbody>
    <tr><td><code>.git/BISECT_HEAD</code></td><td>Current midpoint OID (set by git after range is established)</td></tr>
    <tr><td><code>.git/BISECT_LOG</code></td><td>Ordered list of <code>git bisect good/bad/skip</code> commands — parsed to reconstruct all marks</td></tr>
    <tr><td><code>.arbor/bisect/&lt;id&gt;/session.json</code></td><td>Persisted session: id, name, status, bad/good hashes, result, timestamps</td></tr>
  </tbody>
</table>
