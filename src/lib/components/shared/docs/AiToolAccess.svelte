<h1>AI tool access</h1>

<p>
  Arbor can let an AI client — Claude Code, or anything else that speaks
  <strong>MCP</strong> — call its backends on this machine. The client sees the same
  projects you have open, read through the same configuration: a file is decoded in the
  encoding the project actually declares, a symbol is resolved against the same semantic
  index the editor uses.
</p>
<p>
  It is off by default and reaches nothing until you turn it on. Open it from
  <strong>Settings → AI tool access</strong> — the gear menu on the Canopy launcher, or
  the same menu on the Welcome page when products open as tabs.
</p>

<h2>Connecting a client</h2>
<p>
  Turning on <strong>Accept connections from AI clients</strong> starts a loopback HTTP
  endpoint and shows the whole line your client needs:
</p>
<pre><code>claude mcp add --transport http arbor http://127.0.0.1:8787/mcp --header "Authorization: Bearer &lt;token&gt;"</code></pre>
<p>
  Only this machine can reach it — requests carrying a browser origin are refused, so a
  web page cannot quietly drive Arbor through your own browser.
</p>
<p>
  The token is minted once and kept, so registering a client is genuinely a one-off: it
  goes into that client's own configuration, and a credential that rotated on its own
  would mean re-registering after every restart. Rotate it deliberately with
  <strong>Regenerate token</strong> — every client holding the old one then stops working
  and has to be registered again, which is the point of pressing it.
</p>
<p>
  If your client has no command line, put the same three values into its MCP
  configuration by hand: the URL, the header name <code>Authorization</code>, and
  <code>Bearer &lt;token&gt;</code> as its value.
</p>
<p>
  Arbor must be running. The endpoint lives in the app, alongside the state, the backends
  and your consent — closing Arbor closes it.
</p>

<h2>Four gates</h2>
<p>
  Four independent questions, all starting closed, because one switch would force the
  strictest answer on all of them.
</p>
<ul>
  <li><strong>Endpoint</strong> — is anything listening at all.</li>
  <li><strong>Products</strong> — which backends contribute tools. A product that is off
    is not merely refused: the client is never told it exists.</li>
  <li><strong>Projects</strong> — which paths on disk are in play.</li>
  <li><strong>Permissions</strong> — what may happen, per class of action.</li>
</ul>

<h3>Project scope</h3>
<ul>
  <li><strong>Open projects</strong> — anything you have opened in Arbor, in any product.
    The default, and the one that needs no maintenance: opening a project is the grant.</li>
  <li><strong>What each product has open</strong> — the same idea, but each product reaches
    only its own projects. Under <em>Open projects</em> every product's projects go into one
    pool, so a repository you only ever opened in Corvus is reachable by Bennu's
    file-reading tools — which is not what opening it in Corvus meant. Here the grant
    matches the act: you opened this in Bennu, so Bennu's tools may work on it, and nothing
    else gained anything. A refusal says which of the two you hit — the project is not open
    at all, or it is open somewhere else — because those need different things from you.</li>
  <li><strong>Listed projects</strong> — only the projects on the list, open or not.</li>
  <li><strong>Anywhere</strong> — any file this account can read, including keys and
    credentials outside your projects.</li>
</ul>
<p>
  A path outside scope is refused <em>without prompting</em>. That ordering is the point:
  a request for something private never becomes a dialog you might click through.
</p>

<h3>Classes of action</h3>
<p>
  Every tool declares what class of thing it does, and each class is set to allow, ask, or
  refuse. Prompting for everything trains you to approve without reading, so the prompts
  are spent where they carry information.
</p>
<ul>
  <li><strong>Read</strong> — observe without changing anything. Allowed by default.</li>
  <li><strong>Modify</strong> — change something you can undo. Asks by default.</li>
  <li><strong>Destructive</strong> — delete, rewrite in bulk, or run code. Refused by default.</li>
</ul>

<h2>Per-project rules</h2>
<p>
  A project can answer the product and permission questions for itself. This is the grant
  a single switch cannot express: that <em>this one</em> checkout may also be written to,
  while everything else stays read-only.
</p>
<p>
  Edit a rule from two places — the <strong>Projects</strong> page, which lists everything
  you have granted at once, and the settings menu of the window a project is open in
  (Bennu: <strong>AI access for this project…</strong>), which is where you already are
  when you want to change it.
</p>
<p>
  Every field is an override with an explicit <strong>Inherit</strong> position, and each
  inherited row shows what it currently resolves to — <code>Inherit (Ask)</code>. A rule
  states only what it disagrees with, so tightening the defaults still tightens every
  project that never objected.
</p>
<p>
  Rules live in your Arbor profile, keyed by path — never inside the project. A permission
  file in a repository is one that gets committed, and a shared repo could otherwise hand
  <code>destructive: allow</code> to everyone who clones it. What an AI client may do here
  is a fact about your trust in this checkout.
</p>
<p>Two edges worth knowing:</p>
<ul>
  <li>When roots nest, the innermost project decides — it is the more specific statement
    about the file in question.</li>
  <li>A call naming paths in several projects gets the <em>strictest</em> of their
    answers, and a call naming no path at all — a screen capture — is decided by the
    defaults, since attributing it to whichever project happens to be open would be a
    guess dressed up as a permission.</li>
</ul>

<h2>Approving an action</h2>
<p>
  When a call needs your approval, Arbor shows the tool, what it says it does, and
  <em>the actual arguments</em> — "write a file" and "write that file" are different
  questions, and only the second one can be answered.
</p>
<ul>
  <li><strong>Ctrl/Cmd+Enter</strong> approves. <strong>Enter</strong> refuses.</li>
  <li>Escape, the backdrop and the X all refuse, as does walking away: the prompt answers
    no on your behalf when its timer runs out, so a waiting call never holds the model's
    turn open indefinitely.</li>
  <li><em>Allow this tool for the rest of this session</em> is remembered in memory only.
    It is gone on restart, and whenever these settings change — a tightened policy that
    left old grants standing would not be tightened.</li>
</ul>
<p>
  The prompt appears in one window, chosen so you actually see it: the focused window when
  Arbor has focus, otherwise a visible one. It asks for your attention rather than stealing
  focus, since you are most likely typing in the client that caused it.
</p>

<h2>Rules by project</h2>
<p>
  <strong>Manage projects…</strong> — from the project list, or from a product window's
  settings menu — opens the list and the rule together: pick a project on the left, set what
  it allows on the right. Changes apply as you make them, like the rest of these settings: a
  permission you can see is a permission that is in force.
</p>
<p>
  You pick from the projects Arbor already knows: everything you have opened, in any
  product, is offered and joins the list in one click. Opened from a product window, that
  product's own projects come first — from Bennu you see Bennu's — with the rest one fold
  away, since a rule is about a path and a path does not belong to a product. A path Arbor
  has never seen still goes through the folder picker, but a project you have been working
  in all week does not need finding on disk.
</p>
<p>
  The same list is the scope under <em>Listed projects</em>, so the window says which of the
  two it is at the moment — a set of exceptions to the defaults, or the only paths that are
  reachable at all — and lets you switch between them there.
</p>

<h2>One tool at a time</h2>
<p>
  A project's rule has a second tab, <strong>Per tool</strong>, for the grant a class
  cannot express. "This project may be written to, but only by the file-saving tool" is a
  sentence about one endpoint; saying it by loosening the whole Modify class would grant
  every other endpoint in that class too.
</p>
<p>
  A tool named there answers for itself and ignores its class — most specific wins, which
  is the only ordering that makes the override worth having. Everything you do not name
  keeps following the classes, so the list stays short and a rule written today still
  follows the tool set as it grows.
</p>

<h2>When the tools change</h2>
<p>
  A client asks Arbor for its tool list once, when it connects, and keeps it. So switching a
  product on — or rebuilding a backend, if you develop Arbor — used to leave it offering
  tools that were gone and unable to see the ones that had arrived, with nothing saying so.
  Arbor now tells connected clients the set has moved, and they re-read it themselves.
</p>
<p>
  It is the only thing Arbor sends a client unasked, beyond progress on a call that asked
  for it. There is no other server-initiated traffic.
</p>

<h2>Who is connected</h2>
<p>
  The <strong>Endpoint</strong> page lists every client that has introduced itself since
  Arbor started, with when it last did. Connections are not kept open, so this is a record
  of handshakes rather than a list of who is there this second — a client that quit leaves
  nothing behind to notice.
</p>
<p>
  A client only introduces itself when it first connects, and Arbor restarting gives it no
  reason to do it again — so after a restart its calls are counted here while naming
  nobody. That is what the call count is for: "something is talking to this" needs no
  identity, and is the only form the answer can take.
</p>
<p>
  The one live figure is how many clients are <em>listening for updates</em>. A client that
  is not shows up in the list all the same, but will not learn about a change to the tool
  list until it reconnects.
</p>

<h2>Seeing what is exposed</h2>
<p>
  <strong>Show AI tools</strong> — in the Command Palette, or from the settings menu on the
  home surface — lists every tool Arbor can offer,
  grouped by product, each with what it does, the class of action it belongs to, and the
  backend handler behind it — the same string the call log names. Nothing on it changes
  anything; it is the reference for a question you would otherwise have to ask the
  assistant.
</p>
<p>
  Products you have not switched on are listed too, dimmed. Deciding whether to expose one
  is a decision about what its tools would let an assistant do, and a list you can only see
  after switching it on is no help in making it.
</p>

<h2>A project you do not have open</h2>
<p>
  Scope is about projects you <em>have opened</em>, not about what is on screen now — so a
  client can work on a project you opened in Bennu last week with no window for it anywhere.
  Opening it is the first call: it reads the manifest, registers the frameworks, and starts
  the language server, exactly as opening it in a tab would. Nothing needs to be running
  first; the backend starts itself.
</p>
<p>
  What that costs is a language server, and rust-analyzer holds most of a gigabyte. So a
  server with no window behind it is treated differently from one you are looking at. It runs
  <em>lean</em>: it analyses a crate when a question needs it instead of indexing the whole
  workspace up front, keeps a bounded cache, and takes two threads rather than every core.
  The settings that make code resolve at all — proc macros, build scripts — are untouched,
  because a project whose derives do not expand reads as broken rather than as cheap.
</p>
<p>
  And it does not stay forever. Once it goes quiet it is stopped, after a delay you choose in
  <strong>Bennu → Settings → Language servers</strong> — five minutes to an hour, or never. A
  server for a project you have open is never stopped this way, and one that was started for a
  client and then opened in a tab stops being reclaimable from that moment. Changing the delay
  applies to servers already running.
</p>

<h2>Calls that take minutes</h2>
<p>
  Running a project's tests — or compiling it — is a tool call like any other, except that
  it can take minutes.
  Arbor answers those on a live stream instead of in one payload: the client is told what is
  happening while it happens — which class is running, which crate is compiling, how each
  target finished — and gets the result at the end. Nothing is buffered until the run is
  over, so a client waiting on a build is never left unable to tell it apart from a hang.
</p>
<p>
  The stream is offered only when the client asked to read one and said how to correlate the
  updates. A client that does neither gets the same call as a single answer, with the same
  result — it simply waits in silence for it.
</p>
<p>
  While the run goes, Bennu's own <strong>Tests</strong> panel fills in exactly as it would
  have if you had started the run yourself: it is the same run, and its events still reach
  the window. Watching what a model is running is a matter of opening the panel.
</p>

<h2>Activity</h2>
<p>
  <strong>Show AI Activity</strong> — from any product's Command Palette, or the home
  surface's settings menu — lists every call, newest first, <em>from the moment it
  arrives</em> rather than when it finishes. A row updates as the call moves: starting,
  waiting for your approval, running, and however it ended. A call that is running shows
  the output it is producing, so "it is running the tests" and "it is on OrderTest, 12
  passed" are not the same amount of knowing.
</p>
<p>
  Calls are bucketed by what became of them — ran without needing permission, you approved,
  you declined, refused by rules, failed — and a bucket only appears when it has something
  in it. <em>Ran freely</em> is the one worth reviewing: it is what your settings let
  through without asking. Refusals are rows too, carrying the reason the model was given in
  the same words it read. A picker narrows the log to one product once more than one has
  been called, so "what has Bennu been asked to do" is a question the page answers.
</p>
<p>
  The log survives a restart, so the call you want to look at can be the one from the
  session where you noticed something odd — including a call that was still running when
  Arbor stopped, which is read back as <em>interrupted</em> rather than left claiming to be
  in flight. Rows carried over from earlier runs say so and
  carry their date, and <em>This run only</em> narrows to the current session. It lives in
  your Arbor profile beside your settings — never in a project, where it would be committed
  — it is capped rather than kept forever, and <strong>Clear</strong> deletes the file
  rather than emptying it.
</p>

<h2>What the client can reach</h2>
<p>
  Arbor also offers the projects you have open as MCP <em>resources</em>, so a session can
  start knowing what you are working on instead of spending its first call asking. A
  resource reports <em>that</em> a project is open and where, never what is inside it —
  reading a file is still a tool call, gated like every other one.
</p>
<p>Tools, by product:</p>
<ul>
  <li><strong>Bennu</strong> — open and summarise a project, list its types and files,
    read a file in the project's real encoding, resolve the symbol at a line and column,
    find a type or member by name, find every place a symbol is used, plan a rename across
    the whole project from its reference index, list the framework catalogs (Spring beans,
    endpoints, MyBatis mappers, Bevy components, systems, and the pairs of systems that
    contend over the same state), map the project's own modules with what a change to each
    would rebuild, describe a Cargo workspace and its resolved dependencies, list the
    dependencies with a newer release on crates.io and the versions a crate has published,
    list the tests a project declares with the selector each runner needs,
    <strong>run those tests and report what failed</strong>,
    <strong>compile the project and report the compiler's errors</strong>, check one file
    for problems without building it, list TODO markers, check the index state, and write a
    file back in the encoding it is actually in.
    Two engines answer behind these, and both take a while to be ready: Bennu's own semantic
    index on a Java project, and rust-analyzer on a Rust one. An empty answer while either is
    still loading means "not yet", not "nothing there" — and the tools say which of the two
    they are in, so a client is never told a method is unused when the truth is that nothing
    has finished reading the project.</li>
  <li><strong>Tyto</strong> — list capturable screens and windows, capture one (by window
    title, returned as an image), read the on-screen accessibility layout, check whether a
    recording is running.</li>
</ul>
