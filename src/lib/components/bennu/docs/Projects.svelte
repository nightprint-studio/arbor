<!-- Bennu docs — Projects, JDK & capabilities. -->
<h1>Projects, JDK &amp; capabilities</h1>
<p class="doc-lead">
  A Bennu project is the folder holding a root manifest — a <strong>Maven</strong>
  <code>pom.xml</code> or a <strong>Cargo</strong> <code>Cargo.toml</code>. Opening it resolves the
  build model: the display name, the modules or workspace crates, and — for Maven — the JDK language
  level and the domain frameworks the code relies on.
</p>

<h2>Maven and Cargo</h2>
<p>
  <strong>Maven</strong> projects get the whole of Bennu: the symbol index, completion, go-to
  declaration, find usages, rename, capability detection, JDK resolution, validation, Generate, the
  Structure / Maven / Dependencies / Services / Forms tool windows, and Tomcat hot-swap.
</p>
<p>
  <strong>Cargo</strong> projects get the <strong>editor</strong>: the file tree, go-to file,
  find in files, TODOs, the terminal, Rust and TOML highlighting, and
  <strong>Check project</strong> — <code>cargo check</code> over the workspace, whose errors and
  warnings land in the Problems panel and on the editor gutter like any other build. What is
  <em>not</em> there is everything that would need a Rust symbol index: completion, navigation,
  rename. Those need a language server, and until one is wired the actions are hidden rather than
  offered and silent. The Java-only tool windows and the JDK footer are hidden too, so the window
  never shows a panel that can only ever be empty.
</p>
<p>
  A folder holding <em>both</em> manifests opens as the Maven project: it is the model that has more
  to say. The footer names which kind is open — the JDK and capability count for Maven, the
  toolchain and crate count for Cargo.
</p>

<h2>Workspaces</h2>
<p>
  A <strong>workspace</strong> is a named, colored group of projects you can switch between as a
  unit. Hold several projects in one workspace and keep several workspaces side by side — the same
  project may belong to more than one. Each workspace remembers its own open tabs, so switching
  reopens exactly where you left off.
</p>
<p>
  The <strong>switcher</strong> in the titlebar is a tree: every workspace is a row, its member
  projects nested underneath. Click a <strong>workspace</strong> to switch to it, or a
  <strong>project</strong> to jump straight into it (switching workspace first if needed). Every
  project keeps its tabs, tree and index in memory, so switching — a project or a whole workspace —
  never reopens anything. A file opened from a different project of the workspace stays in the
  current tab strip, <strong>badged with its owning project</strong>. From the command palette,
  <strong>Switch project</strong> and <strong>Switch workspace</strong> do the same from the
  keyboard.
</p>
<p>
  The <strong>workspace manager</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>W</kbd>, or
  <em>Manage workspaces…</em> in the dropdown) is where you create, rename, recolor and delete
  workspaces and add or remove their member projects. <strong>Open project</strong>
  (<kbd>Ctrl</kbd> + <kbd>O</kbd>) resets the active workspace to a single project; the whole set is
  remembered and reopened on the next launch. <strong>Find in project</strong> gains a scope toggle
  to search the active project only or the <strong>entire workspace</strong>.
</p>

<h2>New files</h2>
<p>
  The Project tree's <strong>＋</strong> button — or <strong>New file…</strong> from a right-click
  (or the tool-window menu) — scaffolds a file in the chosen directory: a Java
  <strong>class / interface / enum / record / annotation</strong> (the <code>package</code> is
  <strong>inferred</strong> from the directory, following <code>src/main/java</code> and friends), a
  <strong>JSP</strong> or <strong>XML</strong> file with the right header, or a plain file. It opens
  the new file and reveals it in the tree; it never overwrites an existing one.
</p>

<h2>The JDK</h2>
<p>
  The footer shows the resolved Java language level and where it came from — usually
  <code>maven.compiler.source</code>, but also <code>maven.compiler.release</code>,
  <code>&lt;java.version&gt;</code>, the compiler plugin, toolchains, or a manual override. When it
  can't be inferred the footer reads <code>JDK —</code>.
</p>
<p>
  The <em>install</em> Bennu resolves the standard library against is looked for in the extra JDK
  directories from Settings first, then <code>JAVA_HOME</code>, then each platform's usual
  locations: the <code>JavaVirtualMachines</code> bundles on macOS, the Program Files vendor
  directories on Windows, <code>/usr/lib/jvm</code> on Linux, the Homebrew <code>openjdk</code>
  formula, and the directories a version manager or an IDE installs JDKs into. The one whose level
  matches the project wins; failing that, the newest installed. When none is found the title bar
  carries a <strong>No JDK</strong> warning, because without one nothing — not even
  <code>String</code> — resolves.
</p>

<h2>Dependencies</h2>
<p>
  For a Maven project Bennu resolves the dependency jars from your local repository so completion,
  navigation and validation see library types, not just the JDK and your own sources. The resolve is
  <strong>offline</strong>: a dependency that has never been downloaded can't be resolved, so build
  the project once. Every module of a multi-module project contributes its own dependencies.
</p>
<p>
  The result is cached against your poms' timestamps — editing a <code>pom.xml</code> re-resolves,
  and <strong>Rebuild index</strong> re-resolves unconditionally. When the resolve can't happen at
  all, Bennu says so with the reason rather than leaving you with unresolvable library types: Maven
  is looked for on <code>PATH</code>, then in the usual install directories, then as the project's
  own <code>mvnw</code> wrapper.
</p>

<h2>Capabilities</h2>
<p>
  Bennu detects the domain frameworks a project uses and shows the count in the footer. Each
  capability is backed by <strong>evidence</strong> at one of three tiers:
</p>
<ul>
  <li><strong>Tier A</strong> — a dependency coordinate in the <code>pom.xml</code> (strongest).</li>
  <li><strong>Tier B</strong> — a configuration file (e.g. a <code>struts.xml</code> or a TLD).</li>
  <li><strong>Tier C</strong> — a source pattern (corroborating; a C-only hit is provisional).</li>
</ul>
<p>
  The detected set gates which features light up — for example, JSP taglib awareness only when a
  taglib is actually in use. The demo project shows Struts (convention + XML), JSP taglibs, the
  OGNL value stack, a JDBC DAO and Entando.
</p>

<h2>Form analysis</h2>
<p>
  The <strong>Forms</strong> tool window (bottom dock, toggled from the right rail with
  <kbd>Alt</kbd> + <kbd>3</kbd> — offered only on a project that actually has JSP pages) analyses
  the open JSP and lists every <code>&lt;form&gt;</code>
  relevant to it with the <strong>complete set of parameters</strong> it posts. It resolves the action each form targets — the mapped action class
  and the <code>struts.xml</code> fragment that declares it, even when the target is written as an
  Entando <code>&lt;wp:action path=…&gt;</code> — and lists every input, including
  <strong>hidden</strong> ones, with the <code>value</code> each posts (a fixed value or an
  <code>$&lbrace;…&rbrace;</code>/<code>%&lbrace;…&rbrace;</code> expression). A field inside a
  <code>&lt;c:if&gt;</code>/<code>&lt;s:if&gt;</code> is marked <strong>if</strong> (hover for the
  condition) since it is submitted only when that test holds. Two badges flag each field:
  <strong>bound</strong> when the field name is a writable property of the action class, and
  <strong>valid</strong> when it carries a Struts validation rule. A field that is neither reads as
  muted — the signal a name is a typo or an unmapped request parameter. Clicking a form or field
  jumps the editor to it; the config button opens the declaring fragment.
</p>
<p>
  It is <strong>include-aware</strong>. A JSP form is often split across
  <code>&lt;jsp:include&gt;</code>s — the page opens the <code>&lt;form&gt;</code> and the hidden
  tokens, wizard-step inputs and button bar come from included fragments. So each form gathers those
  fields too: on a parent page you see all the parameters, the children's included, each tagged with
  the fragment it comes from. And the reverse — when you are on an included fragment, the parent form
  it feeds surfaces (a chip names the page it lives on) with its whole parameter set, and the fields
  <em>this</em> fragment contributes are highlighted. The walk is recursive and cycle-safe; a very
  large include graph shows a "…more" hint rather than silently dropping pages.
</p>

<h2>Encodings</h2>
<p>
  Legacy projects often declare <code>Cp1252</code> in their <code>pom.xml</code>
  (<code>project.build.sourceEncoding</code>). Bennu decodes each file with the pom-declared
  encoding and shows which one won in the footer, so a mojibake surprise never slips in silently.
</p>
<p>
  A <strong>Cargo</strong> project is always <code>UTF-8</code>: Rust source is UTF-8 by language
  definition, so the encoding default configured for a legacy Java tree never reaches it.
</p>

<h2>Spring</h2>
<p>
  On a project that uses Spring — detected the same way every other capability is, from the pom and
  the sources — Bennu reads the wiring and puts it where you are already looking. Nothing here
  appears on a project without Spring.
</p>
<p>
  <strong>In the gutter.</strong> A <code>◆</code> marks a bean declaration and lists
  where it is injected; a <code>→</code> marks an injection point and points at the beans
  that can satisfy it; a <code>»</code> marks a request handler and names its route.
  Click one to jump. Injection points include the ones with no constructor in sight — a single
  constructor needs no <code>@Autowired</code>, and Lombok's generated one makes the
  <code>final</code> fields themselves the injection points.
</p>
<p>
  <strong>In annotation strings.</strong> <code>@Value("$&#123;app.timeout:30&#125;")</code>
  is one opaque
  string to Java and three different things to Spring, so the key, the default and any embedded
  SpEL are coloured apart. <kbd>Ctrl</kbd> + <kbd>B</kbd> on the key opens the
  <code>application*.yml</code> line that declares it; hovering shows the value it resolves to;
  typing inside <code>$&#123;</code> completes from the project's own keys. The same works for
  <code>@Qualifier("…")</code> and for a SpEL <code>@beanName</code>, which navigate to the bean.
</p>
<p>
  <strong>Which <code>application.yml</code>?</strong> A project has several — a base file, one per
  profile, one per module — and which one runs is a launch argument, not something the sources
  reveal. So Bennu asks instead of guessing: open <strong>Spring configuration</strong> from the
  palette and pick one under <em>Resolve against</em>. The choice is remembered per project; with
  none set, the profile-less files answer (what Spring always loads).
</p>
<p>
  <strong>In bean XML.</strong> <code>&lt;bean class=&gt;</code>, <code>ref=</code> and
  <code>&lt;property name=&gt;</code> all navigate and complete, and a
  <code>&lt;property&gt;</code> that names nothing writable on the bean's class is flagged. That
  check only speaks when it is sure: if the class extends something outside the project, or carries
  a Lombok annotation whose generated accessors Bennu doesn't model, it stays quiet rather than
  guess. Same discipline elsewhere — a missing <code>class=</code> is reported only when its package
  is one the project itself declares, an unknown <code>ref=</code> only when it looks like a typo of
  a bean that does exist (a bean can legitimately come from a jar), and an unresolved
  <code>$&#123;key&#125;</code> only when it has no default and the project already
  configures other keys in the same namespace.
</p>
<p>
  <strong>An annotation is checked by origin, not by name.</strong> <code>@Service</code> is not a
  reserved word — a project can declare its own, and several do. Bennu resolves each annotation
  through the file's imports exactly as the compiler would: a qualified use decides outright, then
  an explicit <code>import</code> of that name, then a wildcard import of the expected package, and
  a bare name with no import at all can only be a class from the same package, so it is not
  Spring's. Your <code>com.acme.Service</code> therefore declares no bean, gets no gutter icon and
  is counted in no panel. The one thing this misses is a meta-annotation — your own
  <code>@MyService</code> that is itself annotated <code>@Service</code> is a real stereotype and is
  not recognised, which loses a bean rather than inventing one.
</p>
<p>
  <strong>Configuration properties.</strong> Hovering a field of a
  <code>@ConfigurationProperties</code> class shows the <em>full key it binds</em> — the string that
  appears nowhere in the source and that you otherwise assemble in your head from the prefix, the
  chain of field names above it, and Spring's relaxed-binding rules. <code>readTimeout</code> three
  levels down reads as <code>app.http.client.read-timeout</code>, with the value it currently has,
  and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line that sets it. Nesting is followed, a
  <code>Map</code> binds <code>…&lt;key&gt;…</code> and a <code>List</code> binds
  <code>…[0]…</code>, and <code>@Name</code> overrides the field name. A class reached from two
  different roots shows both keys rather than picking one.
</p>
<p>
  <strong>Conditional beans.</strong> A bean behind a <code>@ConditionalOn…</code> is a different
  thing from a bean, so it says which condition gates it — in the Beans panel and on hover.
  <code>@ConditionalOnProperty</code>, <code>OnBean</code>, <code>OnMissingBean</code>,
  <code>OnClass</code>, <code>OnExpression</code> and the rest of the family are read; the
  property one goes further, because its key is a real key: hovering it shows the value it has
  right now, and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line that sets it.
</p>
<p>
  <strong>From the yaml side.</strong> Open an <code>application*.yml</code> and each key that
  something reads carries a count in the gutter — <code>2</code> means two places read it, and
  clicking asks which one. A key with no mark is the useful signal: nothing in this project reads
  it. The count includes <code>@Value</code>, <code>@ConditionalOnProperty</code>,
  <code>@ConfigurationProperties</code> fields and XML <code>value="$&#123;…&#125;"</code>, and
  both spellings of a relaxed-binding key (<code>readTimeout</code> and
  <code>read-timeout</code>) count as one.
</p>
<p>
  <strong>When a jump has more than one destination</strong> — a bean injected in six places, a
  key read from three — Bennu asks instead of picking. The menu opens at the pointer for a gutter
  icon and at the caret for <kbd>Ctrl</kbd> + <kbd>B</kbd>, and each entry says what kind of site
  it is, which is how you tell two injections of the same bean apart.
</p>
<p>
  <strong>The Endpoints panel</strong> (right activity bar, <kbd>Alt</kbd> + <kbd>4</kbd>) lists
  every route with the class-level and method-level mappings already joined. It groups — by path,
  by controller or by method — filters across paths, handlers, return types <em>and</em> parameter
  names, and each route expands to show what it takes: which values come from the path, the query
  string or the body, which are optional, and what each is called when the annotation renames it.
  Verbs are coloured the way an API console colours them, so the list is skimmable rather than
  readable.
</p>
<p>
  <strong>Four panels</strong>, the rest from the command palette: <strong>Spring beans</strong>
  (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd>) lists every bean with its stereotype, scope and
  profile; <strong>Spring endpoints</strong> lists every route with the class-level and method-level
  mappings already joined, so <code>GET /orders/&#123;id&#125;</code> is one searchable line;
  <strong>Spring configuration</strong> lists every property key with its value and source file.
  Each row opens its declaration.
</p>

<h2>Tomcat hot-swap</h2>
<p>
  Link the project to a local Tomcat and push changed JSPs straight into the running server — no
  redeploy, no restart. Open <strong>Tomcat hot-swap…</strong> from the command palette and pick the
  Tomcat root (the folder holding <code>webapps/</code>, <code>bin/</code>, <code>conf/</code>).
  Bennu validates it, lists the deployed web applications, and auto-selects the one this project maps
  to — by <code>&lt;finalName&gt;</code>, artifactId or folder name, or the only app deployed. The
  link is remembered per project.
</p>
<p>
  <strong>Deploy current JSP</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F10</kbd>) saves the
  open JSP and copies it into the deployed webapp at the same path it has under the project's webapp
  source dir; Tomcat recompiles it on the next request. <strong>Deploy all JSPs to Tomcat</strong>
  (command palette) copies every page at once. Files are copied byte-for-byte, so each JSP keeps its
  own page encoding. A toast confirms what was deployed, or explains why it couldn't be (no Tomcat
  linked, no webapp source dir, or an ambiguous target — pick one in the settings).
</p>
