<script lang="ts">
  /**
   * Spring configuration: keys and class names inside annotation strings, the full key a @ConfigurationProperties field
   * binds, conditional beans, the YAML-side usage counts, completion and hover in property files, environment variables.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Spring</span>
<h1>Spring configuration</h1>

<p class="doc-lead">
  Configuration is a second program: keys written as strings in the Java, values written in YAML, and a binding between them that only Spring knows. Bennu follows
  it in both directions — from the code to the line that sets a key, and from a key to everything that reads it.
</p>

<h2>Keys and names inside annotation strings</h2>
<pre><code>{@html highlightCode(`@Value("\${app.timeout:30}")
private int timeout;

@ConditionalOnProperty(name = "app.routing.mode")
@ConditionalOnClass(name = "com.zaxxer.hikari.HikariDataSource")
@ConfigurationProperties(prefix = "app.http.client")`, 'java')}</code></pre>
<p>
  To Java each of those is an opaque string; to Spring they are keys, defaults, SpEL, beans and classes. Bennu reads them as what they are:
</p>
<dl class="meta-grid">
  <dt><code>@Value("$&#123;app.timeout:30&#125;")</code></dt>
  <dd>The key, the default and any embedded SpEL are coloured apart. <kbd>Ctrl</kbd> + <kbd>B</kbd> on the key opens the <code>application*.yml</code> line declaring it, hover shows the value it resolves to, and typing inside <code>$&#123;</code> completes from the project's own keys.</dd>
  <dt>A key without braces</dt>
  <dd><code>@ConditionalOnProperty(name = "app.routing.mode")</code> names a property as directly as <code>$&#123;…&#125;</code> does, so it gets the same colour, hover and go-to — two ways of writing one thing should not look like two. <code>havingValue</code> keeps the string colour: it is what the key is compared <em>to</em>.</dd>
  <dt><code>@ConfigurationProperties(prefix)</code></dt>
  <dd>Coloured as a key. Hover says which property files declare anything under it — a prefix matching nothing is a class binding its defaults for ever, and looks exactly like one that works — and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the block, one entry per file.</dd>
  <dt>A bean as a string</dt>
  <dd><code>@Qualifier("fast")</code>, <code>@DependsOn("audit")</code>, <code>@Resource(name = "ds")</code> and a SpEL <code>@beanName</code> are the same thing said differently: one colour, and they follow to the declaration.</dd>
  <dt>A class as a string</dt>
  <dd><code>@ConditionalOnClass(name = "…")</code> names a type the only way it can — it may not be on the compile classpath, which is the point of the condition — so a typo silently turns the condition off for ever. It is <strong>coloured</strong> as a type, <strong>completed</strong> from the classpath (the index behind "Import class", JDK and dependencies included), and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens it. <code>@ConditionalOnMissingClass</code> likewise.</dd>
</dl>
<Callout variant="info" title="A bean name is not a class name">
  <code>@ConditionalOnBean(name = "…")</code> is left alone: it names a bean, not a type, however much the two look alike.
</Callout>

<h2>Which application.yml answers</h2>
<p>
  A project has several — a base file, one per profile, one per module — and which runs is a launch argument, not something the sources say. Hover, go-to and
  the value shown for a key are all answered by one chosen file.
</p>
<Callout variant="tip" title="Pick the file once">
  Open <strong>Spring configuration</strong> from the palette and pick one under <em>Resolve against</em>; it is remembered per project. With none set, the
  profile-less files answer — what Spring always loads.
</Callout>
<p>
  The launch side of profiles — the <code>-Dspring.profiles.active</code> a Spring Boot run configuration passes, and the <code>application-«profile»</code> files it
  detects — is described under <strong>Running</strong>. <strong>Settings → Spring</strong> names the dependencies that contribute beans and shows the file pinned for
  the project.
</p>

<h2>Configuration properties</h2>
<p>
  Hovering a field of a <code>@ConfigurationProperties</code> class shows <em>the full key it binds</em> — the string that appears nowhere in the source, assembled
  from the prefix, the chain of field names above it, and Spring's relaxed binding:
</p>
<table>
  <thead><tr><th>Field</th><th>Binds</th></tr></thead>
  <tbody>
    <tr><td><code>readTimeout</code>, three levels down</td><td><code>app.http.client.read-timeout</code> — with its current value; <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line setting it</td></tr>
    <tr><td>A <code>Map</code></td><td><code>…«key»…</code></td></tr>
    <tr><td>A <code>List</code></td><td><code>…[0]…</code></td></tr>
    <tr><td>A field with <code>@Name</code></td><td>The name it gives</td></tr>
  </tbody>
</table>
<p>
  A class reached from two roots shows both keys. A class can also be written <em>from</em> the keys — see <strong>Code templates</strong>. The
  <strong>Spring bound properties</strong> panel lists the same bindings for the whole project.
</p>

<h2>Conditional beans</h2>
<p>
  A bean behind a <code>@ConditionalOn…</code> is a different thing from a bean, so it says which condition gates it — in the Beans panel and on hover.
  <code>@ConditionalOnProperty</code>, <code>OnBean</code>, <code>OnMissingBean</code>, <code>OnClass</code>, <code>OnExpression</code> and the rest are read, and the
  property one goes further: its key is real, so hover shows its value now and <kbd>Ctrl</kbd> + <kbd>B</kbd> opens the line setting it.
</p>
<p>
  The Beans panel shows the profile a bean is declared for beside its stereotype and scope. The same conditions are shown on the beans a library may register, which is
  why those never take part in autowiring — see <strong>Spring</strong>.
</p>

<h2>From the YAML side</h2>
<p>
  In an <code>application*.yml</code>, each key something reads carries a count in the gutter — <code>2</code> means two places read it, and clicking asks which.
  <strong>A key with no mark</strong> is the useful signal: nothing in this project reads it.
</p>
<ul>
  <li>The count includes <code>@Value</code>, <code>@ConditionalOnProperty</code>, <code>@ConfigurationProperties</code> fields and XML
    <code>value="$&#123;…&#125;"</code>, and both spellings of a relaxed key — <code>readTimeout</code> and <code>read-timeout</code> — count as one.</li>
  <li>A key whose value is a <strong>list</strong> counts like any other: <code>allowed-origins:</code> with entries under it is what a <code>List&lt;String&gt;</code>
    binds. The items are not keys — <code>servers[0].url</code> is not <code>servers.url</code>.</li>
  <li>The counts follow the code: a save that changes what reads a key refreshes them shortly after, and a rename touching hundreds of files refreshes them once.</li>
</ul>

<h2>Writing a property file</h2>
<dl class="meta-grid">
  <dt>Completion</dt>
  <dd>Every key Spring and the project's libraries document, <em>and</em> the project's own <code>@ConfigurationProperties</code> paths — the half that matters on a legacy tree, where nobody documented your own namespace. Under a nested mapping candidates are relative: <code>u</code> under <code>spring: datasource:</code> completes to <code>url</code>. Values complete where the set is closed: an enum, a boolean, a log level.</dd>
  <dt>Where the vocabulary comes from</dt>
  <dd>Every Spring starter packages a description of the properties it accepts inside its jar, so the list is version-exact, covers third-party and in-house starters, and needs no network. A curated table stands in until dependencies have resolved once. <strong>Spring property reference</strong> in the palette browses it all, marking the keys this project sets.</dd>
  <dt>Ghost text</dt>
  <dd>Only where the answer is single: a documented default for an empty key — <code>server.port:</code> proposes <code>8080</code> — or a prefix one known key continues. <kbd>Tab</kbd> accepts. Never in the middle of a finished key or before an existing value.</dd>
  <dt>Hover</dt>
  <dd>The type, the default, the library's description, and who reads it. The type comes from the declaration, or from the readers — <code>30</code> against a <code>Duration</code> means thirty seconds. Two readers disagreeing about the type are reported, since that is usually a bug. A <code>$&#123;…&#125;</code> in a value is coloured as inside a <code>@Value</code>.</dd>
</dl>

<h3>As an environment variable</h3>
<p>
  Right-click a property line → <em>Show as environment variable</em>. Nothing is written; you get the name and a ready-to-paste line for a <code>.env</code>, a shell,
  <code>docker run</code> and a compose file.
</p>
<table>
  <thead><tr><th>Property</th><th>Variable</th></tr></thead>
  <tbody>
    <tr><td><code>spring.jpa.show-sql</code></td><td><code>SPRING_JPA_SHOWSQL</code></td></tr>
  </tbody>
</table>
<Callout variant="info" title="Dashes are removed, not replaced">
  The one rule everybody forgets — which is why it is worth computing rather than typing.
</Callout>
<p>
  A key written with quotes in the yaml — <code>service."name".uri</code>, how a map key with a special character is spelled — gives
  <code>SERVICE_NAME_URI</code>: the quotes belong to the file, not to the name.
</p>

<h3>Writing a .env</h3>
<p>
  <code>.env</code>, <code>.env.local</code>, <code>.env.production</code> and <code>*.env</code> files complete the other way round: type the start of a
  variable and the names Spring binds are offered — the framework's own and your <code>@ConfigurationProperties</code> — each with the property it overrides.
  After the <code>=</code>, a closed set of values (a boolean, a documented list) completes too, and hovering a name shows the property, its type, default and
  description. A name Spring does not know is left alone: a <code>.env</code> carries plenty of variables that are not Spring's.
</p>
