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
  Structure / Maven / Dependencies / Forms tool windows, and Tomcat hot-swap.
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
  remembered and reopened on the next launch. <strong>Find in project</strong> and <strong>Go
  to</strong> both gain a toggle beside their field that reaches into every member project at
  once, and a row from another one says which.
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
<p>
  <strong>Builds and test runs use that same install</strong>: it is handed to Maven as
  <code>JAVA_HOME</code>, so the level your code is analysed at is the level it is compiled at. If no
  JDK of that level is installed, the build inherits whatever <code>JAVA_HOME</code> your environment
  already sets rather than being pointed at a different one — a compiler of the wrong version fails
  with a message about the target release, which says nothing about the JDK that caused it.
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
<p>
  <strong>The Dependencies tool window</strong> (<kbd>Alt</kbd> + <kbd>N</kbd>) shows that as a list,
  one group per module. Each row carries the coordinate, the version <em>you actually get</em> — with
  <code>$&#123;…&#125;</code> expanded and <code>&lt;dependencyManagement&gt;</code> applied — the scope, and
  where the answer came from: declared here, pinned by a parent's management, or inherited whole
  from a parent's own <code>&lt;dependencies&gt;</code>. Clicking a row opens the pom that decides
  it, which is usually not the one you were reading. Rows are also tagged
  <code>optional</code>, and a dependency that only exists under a <code>&lt;profile&gt;</code> says
  which one — whether that profile is active depends on the JDK, the OS and the command line, so it
  is shown and labelled rather than guessed at.
</p>
<p>
  A declared dependency with no jar in your local repository is called out, because that is exactly
  what "cannot find symbol" looks like in a file that is fine. Until the classpath has been resolved
  the panel says the column is unknown instead of marking everything missing. The last group,
  <strong>Pulled in transitively</strong>, is every jar on the classpath that no module asked for —
  where "why is <em>this</em> version of that library here" gets answered.
</p>
<p>
  Reading it runs nothing: the poms are files, and the classpath is the one already resolved for the
  index. Imported BOMs and version ranges are the two things it will not compute — a version only
  they can answer stays blank unless the resolved classpath settles it, which is not a guess but the
  jar the compiler is being handed.
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
  <strong>Writing a property file.</strong> An <code>application*.yml</code> or
  <code>*.properties</code> gets completion over every key Spring and the project's libraries
  document, <em>and</em> over the project's own <code>@ConfigurationProperties</code> paths — the
  second half being the one that matters on a legacy tree, where nobody wrote documentation for
  your own namespace and everybody misspells it. Under a nested mapping the candidates are offered
  relative to where you are, so <code>u</code> under <code>spring: datasource:</code> completes to
  <code>url</code>. Values complete too where the set is closed: an enum, a boolean, a log level.
</p>
<p>
  <strong>Where the vocabulary comes from.</strong> Every Spring starter packages a description of
  the properties it accepts inside its own jar, so Bennu reads the ones this project resolves. That
  makes the list version-exact, covers third-party and in-house starters, and needs no network. A
  curated table of the common keys stands in until the dependencies have been resolved once, so the
  feature is useful on a cold checkout rather than silently empty. Browse the whole thing with
  <strong>Spring property reference</strong> from the palette — it marks the keys this project
  already sets.
</p>
<p>
  <strong>Ghost text</strong> appears only where the answer is single-valued: a documented default
  for a key you left empty (<code>server.port:</code> proposes <code>8080</code>), or a prefix
  exactly one known key can continue. <kbd>Tab</kbd> accepts. Anywhere else it stays away and lets
  the completion popup present the alternatives honestly — it is never a guess, and never a repeat
  of what the line already says: a caret parked in the middle of a finished key or in front of an
  existing value gets nothing, because ghost text is inserted where the caret is.
</p>
<p>
  <strong>Hovering a key</strong> answers what the file itself cannot: its type, its documented
  default, the library's own description of it, and who reads it. The type comes from the
  declaration when Spring provides one and from the readers otherwise — <code>30</code> against a
  <code>Duration</code> field means thirty seconds, which the line alone never says. Two readers
  that disagree about the type are reported rather than resolved, because that is usually a bug
  worth seeing. <code>$&#123;…&#125;</code> inside a value is coloured exactly as it is inside a
  <code>@Value</code>: it is the same expression, and reading it as prose is how a typo in one
  survives.
</p>
<p>
  <strong>As an environment variable.</strong> Right-click a property line →
  <em>Show as environment variable</em>. Nothing is written to the file; you get the name and the
  ready-to-paste line for a <code>.env</code>, a shell, <code>docker run</code> and a compose file.
  Worth computing rather than typing because of one rule everybody forgets: dashes are
  <em>removed</em>, not replaced — <code>spring.jpa.show-sql</code> is
  <code>SPRING_JPA_SHOWSQL</code>.
</p>
<p>
  <strong>When a jump has more than one destination</strong> — a bean injected in six places, a
  key read from three — Bennu asks instead of picking. The menu opens at the pointer for a gutter
  icon and at the caret for <kbd>Ctrl</kbd> + <kbd>B</kbd>, and each entry says what kind of site
  it is, which is how you tell two injections of the same bean apart.
</p>
<p>
  <strong>The Endpoints panel</strong> (right activity bar, <kbd>Alt</kbd> + <kbd>4</kbd>) lists
  every URL the application answers, <em>whoever routes it</em>. A Spring route arrives with its
  class-level and method-level mappings already joined; a <strong>Struts action</strong> arrives
  as its URL — the package namespace joined to the action name — with the bean id resolved to the
  class that actually runs. An application mid-migration has both, and they are one list. It
  groups — by path, by handler or by method — filters across paths, handlers, return types
  <em>and</em> parameter names, and each route expands to show what it takes: which values come
  from the path, the query string or the body, which are optional, and what each is called when
  the annotation renames it.
  Verbs are coloured the way an API console colours them and the <code>{'{'}variables{'}'}</code>
  in a path are lit apart from its literal segments, so the list is skimmable rather than readable.
  The count beside the filter says how much of it survived what you typed.
</p>
<p>
  <strong>An action expands into the whole request.</strong> Under a Struts row are the
  <code>&lt;interceptor-ref&gt;</code>s the action declares for itself, then one row per
  <code>&lt;result&gt;</code>: its name, its type, what the config says the target is and — when
  the two differ — the page it finally reaches. A <code>tiles</code> result reads
  <code>admin.Cat.tree → /WEB-INF/jsp/tree.jsp</code>, because the definition name on its own
  tells you nothing about which file you are about to open, and following it by hand means the
  action fragment, then <code>tiles.xml</code>, then the parent definition it extends. Clicking
  the row opens the page. A result that is a <code>chain</code> or a redirect names another
  action rather than a view, and says so instead of pretending to a page. An action with no
  interceptor rows is not an action with no interceptors — it is one that does not override its
  package's default stack.
</p>
<p>
  <strong>A type is a door.</strong> Any chip naming a composite type — the return type of a route,
  the type of a parameter — expands into that class's <strong>fields</strong>, and each field that
  is itself composite expands in turn, as deep as you keep clicking. An interface, or a class read
  out of a jar, is listed by the <strong>properties its getters expose</strong> and says so.
  Wrappers are seen through: a handler declared to return <code>ResponseEntity&lt;OrderDto&gt;</code>
  opens on the <code>OrderDto</code>. Nothing is resolved until you ask — a catalog of two hundred
  routes names two hundred types and you came to look at one.
</p>
<p>
  <strong>Export</strong> (the ⭳ in the panel header) takes the list out of Bennu as
  <strong>CSV</strong>, <strong>JSON</strong> or a <strong>Markdown table</strong>, to the
  clipboard or to a file you name. What leaves is <em>what is on screen</em> — the filter and the
  grouping applied — with each route's parameters flattened onto its row, so the spreadsheet you
  hand to somebody else does not send them back to the panel to see what a route takes.
</p>
<p>
  The media types a mapping produces are shown by their <strong>short names</strong> —
  <code>JSON</code>, <code>SSE</code>, <code>XML</code>, <code>form</code> — rather than as
  <code>MediaType.APPLICATION_JSON_VALUE</code>, which is thirty characters of ceremony around one
  fact and was the widest thing on the row. An unfamiliar one keeps its own spelling: an unusual
  media type is exactly the one worth seeing.
</p>
<p>
  <strong>The panels</strong>, all but Endpoints from the command palette:
  <strong>Spring beans</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd>) lists every bean
  with its stereotype, scope and profile; <strong>Spring configuration</strong> lists every property
  key with its value and source file; <strong>Spring bound properties</strong> lists what each
  <code>@ConfigurationProperties</code> field binds; <strong>Spring property reference</strong>
  lists everything the project's dependencies accept, set or not. Each row opens its declaration.
</p>
<p>
  <strong>Spring beans from libraries</strong> lists the beans declared inside your
  <strong>dependencies</strong> — read from their bytecode, so it needs no sources — one row per
  artifact with its beans nested underneath. Nothing is read until you name the dependencies under
  <strong>Settings → Spring → Beans</strong>, by group id, artifact id, or a prefix of either. The
  entries worth adding are your own shared modules and starters.
</p>
<p>
  These are <strong>declarations, not facts</strong>. Spring Boot's auto-configuration is gated —
  <code>@ConditionalOnMissingBean</code>, <code>@ConditionalOnClass</code>,
  <code>@ConditionalOnProperty</code> — so a bean in a jar is what Spring <em>may</em> register, and
  knowing what it actually registers means running Spring's own evaluator. So every gated bean is
  shown with the conditions gating it, and none of them take part in autowiring candidates,
  completion or any diagnostic. Your project's own beans stay the only answer to what this
  application has.
</p>
<p>
  Each of them is offered only where it has something to say. Having Spring on the classpath turns
  the tooling on; what the model actually found decides which panels exist — a batch job or an
  XML-wired service layer with no request mappings gets no Endpoints button in the activity bar, no
  <kbd>Alt</kbd> + <kbd>4</kbd> and no palette entry, because the alternative is a permanent
  invitation to open an empty list. They appear as the project index finishes, and again after a
  rebuild that finds the first route.
</p>

<h2>JPA</h2>
<p>
  On a project with JPA or Spring Data on its classpath — and only there, like every other
  framework tool here. Entities, repositories and the queries between them.
</p>
<p>
  <strong>Generated sources are part of the project.</strong> The static metamodel
  (<code>Order_</code>, <code>Customer_</code>) that Criteria queries are written against is
  written by an annotation processor into <code>target/generated-sources</code>, and so are
  MapStruct's <code>*MapperImpl</code>, QueryDSL's <code>QOrder</code> and jOOQ's output. None of
  it exists under <code>src/</code> and all of it is referenced from there, so Bennu indexes those
  two roots — but nothing else under <code>target/</code>, which holds build output and sometimes
  an unpacked copy of somebody else's sources. Build the project once and they resolve.
</p>
<p>
  <strong>Derived query names are checked.</strong>
  <code>findByCustomerNameAndTotalGreaterThan</code> is not a name, it is a query that Spring Data
  compiles at <em>application start</em>. A typo in one is invisible to the compiler and to every
  test that doesn't touch that repository, and then it takes the context down on deploy. So every
  segment is resolved against the entity — following relations, so <code>CustomerName</code> is
  <code>customer.name</code> — and a segment that addresses nothing is flagged where you wrote it.
  The number of arguments the name asks for is checked too: <code>Between</code> wants two,
  <code>IsNull</code> wants none, and a <code>Pageable</code> is Spring's, not yours.
</p>
<p>
  <strong>The check goes quiet rather than guess.</strong> An entity whose
  <code>@MappedSuperclass</code> chain leaves the project, a relation whose target was never
  scanned, a repository over a type Bennu doesn't have — each turns the check off for that method.
  Nothing about the database is checked at all: whether the column exists needs a connection, which
  is Picus's business, not this one's.
</p>
<p>
  <strong>A <code>@Query</code> stops being a string.</strong> Keywords, parameters, literals and
  numbers are coloured inside it, and JPQL and native SQL are tinted apart because they are
  different risks — JPQL is resolved against the entity model, native SQL is sent to the database
  as written. A <code>:name</code> that no parameter binds is an error on the placeholder itself,
  with the fix named. <kbd>Ctrl</kbd> + <kbd>B</kbd> inside a query opens the entity it selects from.
</p>
<p>
  <strong>The gutter links the two ends.</strong> <code>▤</code> beside an entity opens the
  repositories that manage it; <code>◇</code> beside a repository opens its entity. Hovering a
  repository method says what it actually asks for — a derived name is rendered as the sentence it
  compiles to.
</p>
<p>
  <strong>The toolbar follows the file.</strong> Standing on an entity, the editor toolbar carries
  <strong>Add attribute</strong>, <strong>Add lifecycle callback</strong>,
  <strong>Add named query</strong>, <strong>Repository</strong> and <strong>Projection</strong>.
  Standing on a repository it carries <strong>Add query method</strong> and
  <strong>Add modify method</strong> instead. On a class that is neither there is nothing — the
  buttons present <em>are</em> the answer to what kind of file this is, so there is no greyed-out
  row to interpret. A <code>@MappedSuperclass</code> gets the attribute and callback buttons but
  not the repository ones: it has no table, so those could not work.
</p>
<p>
  <strong>Adding an attribute</strong> writes the field, how it is stored — a plain column, an
  <code>@Enumerated(STRING)</code>, an <code>@Embedded</code>, an <code>@Lob</code> — its
  constraints, the Bean Validation you ask for, and optionally its accessors. The second preview
  tab shows the <code>alter table</code> the column implies, because the field and the column are
  one decision usually made in two places and the second one is written later from memory. It is a
  starting point and not a migration: no dialect, and no back-fill for a <code>not null</code>
  added to a table that already has rows.
</p>
<p>
  Choose a relation instead and it writes the pair people get backwards by hand: the owning side
  gets the <code>@JoinColumn</code>, and filling in <em>mapped by</em> makes it the inverse side,
  which owns no column at all. A to-many is held in a <code>Set</code> unless you say otherwise —
  a <code>List</code> of children makes Hibernate delete and re-insert the whole collection on any
  change — and it is always initialized, which is the omission that turns into a
  <code>NullPointerException</code> the first time anything adds to a new entity. Cascade and
  orphan removal are there too; the helper methods that keep both sides of a bidirectional relation
  in step are still yours to write.
</p>
<p>
  <strong>Query methods</strong> are built from the entity's own properties, and that is the point:
  a name assembled from properties that exist cannot be misspelled, and the parameter list follows
  from the keywords instead of being counted by eye. Leave <em>method name</em> empty and the
  derived name is used; write one and the method arrives with its <code>@Query</code> spelled out,
  because a name Spring Data cannot parse is no longer a derived query.
</p>
<p>
  <strong>What a finder hands back</strong> is a row of its own: <code>Optional</code>,
  the bare entity, <code>List</code>, <code>Page</code>, <code>Slice</code> or
  <code>Stream</code>. <code>Page</code> and <code>Slice</code> both take a <code>Pageable</code>
  and differ in what they cost — a <code>Page</code> also runs a <code>count(*)</code> to know the
  total, which a <code>Slice</code> skips because it only reports whether more rows follow. That is
  the one you want behind infinite scrolling. A finder can also take a <code>Sort</code> so the
  caller decides the ordering, except on a paged method, where the <code>Pageable</code> already
  carries one and taking both would not compile — the dialog says so rather than offering it.
</p>
<p>
  The button you pressed decides where the form <em>opens</em>, not what it can produce: the verb,
  the return shape, the ordering, a limit and <code>distinct</code> are all editable from inside.
  Adding several methods in a row is what actually happens, so <strong>Add and continue</strong>
  writes one and clears the form for the next without losing the repository you chose.
</p>
<p>
  <strong>Modify methods</strong> are always <code>@Modifying</code> with the JPQL written out.
  Spring Data has no naming scheme for an update at all, and a bulk write goes straight to the
  database — the rows are not loaded, so <code>@PreUpdate</code> and <code>@PreRemove</code> do not
  fire and the persistence context does not see it. The dialog says so, and warns when there are no
  conditions at all.
</p>
<p>
  <strong>Repositories</strong> land in the package the project already keeps repositories in, read
  off the ones that exist rather than assumed. A <strong>projection</strong> can be its own file
  <em>or</em> an interface nested inside the repository that returns it — both are idiomatic, and
  the dialog offers both. Every generator previews live, <kbd>Ctrl</kbd> + <kbd>Enter</kbd>
  commits, and nothing is written before that. Each is also in the command palette by name.
</p>

<h2>Message bundles</h2>
<p>
  Half of what a web application puts on screen is not in its source. It is in a
  <code>.properties</code> file, reached by a string, and normally that string is checked by
  nothing — not the compiler, not the tests, and, because Struts renders an unresolved key as the
  key itself, often not by anyone looking at the page either. Bennu treats bundles as a model.
</p>
<p>
  <strong>What counts as a key.</strong> By shape rather than by a list of tags, because every
  framework in a legacy page spells it differently: an attribute called <code>key</code>, an
  attribute whose name ends in <code>Key</code> (<code>titleKey</code>, <code>messageKey</code>),
  the <code>name</code> of a <code>&lt;s:text&gt;</code> — the one tag where <code>name</code> is
  a key rather than a field — and the first string argument of <code>getText</code>,
  <code>getMessage</code> or <code>getString</code> in Java. A <strong>computed</strong> value
  (<code>%&#123;keyName&#125;</code>, <code>$&#123;row.label&#125;</code>, a scriptlet) is not
  treated as a key at all: it usually is one at runtime, but nothing can say which, and guessing
  would flag every dynamic label in the project.
</p>
<p>
  <strong>What is deliberately left alone</strong>: a key that is answered from somewhere other
  than a file. Entando's <code>&lt;wp:i18n key="…"&gt;</code> reads the platform's label table in
  the <strong>database</strong>, edited from its admin console — no <code>.properties</code>
  declares it, and treating it as one would put a warning under every label on every page. That
  one tag, by name, and no attempt at the rest of Entando's vocabulary.
</p>
<p>
  <strong>On a key</strong>, <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line that declares it — one
  entry per translation, each showing what that language says, so choosing is reading rather than
  guessing. Hovering shows the same thing without leaving the page, and names the locales that do
  not have it yet. Typing inside a key attribute completes from the bundles, with each key's text
  beside it. A key <strong>no bundle declares</strong> is underlined where it is written.
</p>
<p>
  <strong>The Messages panel</strong> (command palette) lists every key with its default text, the
  bundle it belongs to, and two things you cannot see any other way: how many places read it —
  <code>unused</code> when the answer is none — and which locales are <code>missing</code> it.
  Expanding a key shows every translation; each row opens its own file at its own line. Group by
  bundle or by key prefix. Untranslated is counted per bundle, not project-wide: two bundles
  having different locale sets is normal, and comparing across them would invent a debt nobody
  has.
</p>

<h2>XML with a schema behind it</h2>
<p>
  An XML file in a Java project is a configuration language whose vocabulary is written down
  precisely — in the DTD or XSD the document names — and normally nothing reads it. Bennu does.
  Open a <code>struts.xml</code>, a <code>web.xml</code>, a <code>pom.xml</code> or a
  <code>beans.xml</code> and typing <code>&lt;</code> lists the elements that may go there, with
  the schema's own description of each.
</p>
<p>
  <strong>Where the schema comes from.</strong> A document names it by URL, and Bennu never fetches
  one. It does not have to: frameworks ship their grammar inside their own jar —
  <code>struts2-core.jar</code> carries <code>struts-2.5.dtd</code>, <code>spring-beans.jar</code>
  carries every <code>spring-beans.xsd</code> ever published — so the file the URL names is already
  on the machine. Schemas kept in the project itself are found too, and win over a jar copy of the
  same name. The Maven POM is the one exception nobody ships, so its vocabulary is built in.
</p>
<p>
  <strong>What you get.</strong> Element names filtered by what the parent may contain; attribute
  names, with the ones already written removed; attribute <em>values</em> where the schema closes
  the set. Ghost text where exactly one thing can follow — and never where the rest of the name is
  already written, which is most carets in a document whose closing tags the editor typed for you.
  Hover with the schema's documentation,
  the required attributes, and which grammar answered. <kbd>Ctrl</kbd> + <kbd>B</kbd> on a tag or
  an attribute jumps to its declaration in the schema — which turns
  <code>&lt;result type="…"&gt;</code> from a word into something you can read.
</p>
<p>
  <strong>Following the schema itself.</strong> <kbd>Ctrl</kbd> + <kbd>B</kbd> on the
  <code>DOCTYPE</code> or the <code>xsi:schemaLocation</code> opens the grammar the file is
  actually checked against — the copy out of the jar, not the address it is written as. When
  nobody ships one, Bennu downloads it once and caches it, and that is worth more than the
  reading: the cached copy joins the catalog, so a <code>pom.xml</code> stops being answered by
  the built-in table and starts being answered by the real Maven schema. Nothing is ever fetched
  during a scan — only when you follow the link.
</p>
<p>
  <strong>What it will not do.</strong> Say anything at all without a schema. No grammar resolved
  means no completion, no ghost text and no warnings — a vocabulary guessed from the tags already
  in the file would confidently propose whatever typo is already there. And where a schema says
  content is unconstrained (<code>ANY</code>, <code>xs:any</code>, a POM
  <code>&lt;configuration&gt;</code>) nothing inside is checked. Prefixed names are never reported
  either: a document mixing four namespaces usually has schemas for one of them, and the rest must
  be invisible rather than wrong.
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
