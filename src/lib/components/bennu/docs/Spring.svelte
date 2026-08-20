<!-- Bennu docs — Spring: beans, wiring, and navigating XML-defined behaviour. -->
<h1>Spring</h1>
<p class="doc-lead">
  In a Spring application a large part of the behaviour is declared in XML rather than written in
  Java. Bennu reads that XML as part of the program.
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
