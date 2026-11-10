<script lang="ts">
  /**
   * The code generators as a guide: what each one writes, how it adapts to the project, where it is
   * opened from and how its dialog works. The generators are `bennu-generate`'s registry; this page
   * follows it.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Code generators</h1>

<p class="doc-lead">
  Generators write the code that is the same in every project and different in the details: a Spring
  Security configuration, a JPA converter, a Jackson serializer, a client for your own endpoints. Each has
  its own dialog with its own options, shows the result <strong>before</strong> anything is written, and
  writes it as one step — new files are created, and what goes into the open file is a single undo.
</p>

<h2>Opening one</h2>
<ul>
  <li>The <strong>command palette</strong> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>): every generator the open file can take is listed as «Generate …».</li>
  <li>The editor's <strong>context menu › Generate</strong>: one submenu with <em>Members…</em> (the dialog <kbd>Alt</kbd>+<kbd>Insert</kbd> opens), <em>From a template…</em>, the configuration-properties flows where they apply, and then the same list.</li>
  <li>A folder's <strong>context menu › Generate</strong> in the project tree: the classes are written into that folder's package.</li>
</ul>

<h2>What the dialog does</h2>
<p>
  The options are on the left, the preview on the right — one tab per file, one for what is inserted into
  the open class, one for the snippets meant for other files (an <code>application.yml</code> key, a
  TypeScript wrapper). The preview follows every change. Below it Bennu lists the dependencies the code
  needs and the classpath lacks, the annotations it adds to your existing code, and anything worth knowing
  before you accept.
</p>
<p>
  The first field has the focus when it opens; <kbd>Tab</kbd> walks the options, <kbd>Ctrl</kbd>+<kbd>Enter</kbd>
  generates, <kbd>Esc</kbd> cancels. A file that already exists is never overwritten: the dialog says so and
  waits for another name or package.
</p>

<h2>What it reads from the project</h2>
<p>
  Defaults come from the project: the Java level (records and pattern matching only where they compile),
  <code>jakarta.*</code> or <code>javax.*</code> from the Spring Boot, servlet and persistence versions,
  whether Lombok, MapStruct, AssertJ or Mockito are on the classpath, and the package of the application
  class. The detected facts are shown as chips at the top of the dialog.
</p>

<h2>Spring Security configuration</h2>
<p>
  A <code>SecurityFilterChain</code> for an API with JWT, an API with several issuers (a fixed list, each
  with its audience and roles claim — a dotted path such as <code>realm_access.roles</code> is walked), a
  web app with form login, a stateless API with HTTP Basic, or an API and a web app together as two ordered
  chains. Rules are a list of method, path and access; CORS, CSRF (optionally with the token in a cookie),
  method security, a user store (in memory, JDBC, your own) and the password encoder are options.
</p>
<p>
  <strong>Excluded from security</strong> lists Ant patterns (<code>/css/**</code>, <code>/favicon.ico</code>…)
  that skip the filter chain altogether through a <code>WebSecurityCustomizer</code> bean
  (<code>configure(WebSecurity)</code> before 5.7). Web scenarios start with the usual static resources; for a
  path that still needs security headers add a rule with <code>permitAll()</code> instead.
</p>
<p>
  The style follows the Spring Security version: the lambda DSL, the fluent <code>.and()</code> chain where
  it still exists, <code>requestMatchers</code> or <code>antMatchers</code>, <code>@EnableMethodSecurity</code>
  or <code>@EnableGlobalMethodSecurity</code>, and a <code>WebSecurityConfigurerAdapter</code> before 5.7.
</p>

<h2>JPA attribute converter</h2>
<p>
  Dates as ISO text, durations as milliseconds, enums by name or by a code accessor, booleans as
  <code>Y</code>/<code>N</code>, lists as separated text, JSON columns through Jackson, encrypted strings
  (AES-GCM, key from an environment variable) or any pair of expressions. It is registered with
  <code>autoApply</code> or wired onto a field of the open class with <code>@Convert</code>.
</p>

<h2>Jackson serializer / deserializer</h2>
<p>
  For the class at the caret or any type you name: written as a JSON object field by field (with a naming
  strategy and optional nulls) or as a single string. It is registered through Spring Boot's
  <code>@JsonComponent</code>, a <code>Module</code> bean, or <code>@JsonSerialize</code> /
  <code>@JsonDeserialize</code> on the type or a field.
</p>

<h2>HTTP client from Spring endpoints</h2>
<p>
  Pick endpoints from the application's request mappings, grouped by controller, and choose the library:
  <code>RestClient</code>, <code>WebClient</code>, <code>RestTemplate</code>, an <code>@HttpExchange</code>
  interface, OpenFeign, <code>java.net.http.HttpClient</code>, OkHttp or Retrofit. Path, query and header
  parameters and the body become typed method parameters; the return type loses its
  <code>ResponseEntity</code> / <code>Mono</code> wrapper. The base URL is read from a property, authentication
  (bearer, basic, API key) from further ones, and a configuration class builds the client with timeouts and
  an error handler that throws a generated exception carrying the status and the body. The DTOs are either
  imported from the server's module or generated as records or classes next to the client.
</p>

<h2>Web services: SOAP and CXF</h2>
<p>
  <strong>SOAP call</strong> — paste an envelope (and the WSDL, if you have it) and get the class that makes the
  call. The Body gives the operation and its namespace, and every value in the request becomes an argument; the
  WSDL gives the address, the <code>SOAPAction</code> and the service and port names. Variants: the JDK
  <code>HttpClient</code> with the envelope in a text block, Spring-WS <code>WebServiceTemplate</code>,
  <code>WebClient</code>, CXF <code>Dispatch</code> or a generated CXF port, and Apache HttpClient. SOAP 1.1 and
  1.2 are told apart by the envelope namespace (content type, action header), a WS-Security
  <code>UsernameToken</code> is optional, and the response is read by XPath, unmarshalled with JAXB, or returned
  as XML; a SOAP fault is thrown as an exception.
</p>
<p>
  <strong>CXF REST client → modern client</strong> — paste a JAX-RS interface or resource class
  (<code>@Path</code>, <code>@GET</code>, <code>@PathParam</code>, <code>@QueryParam</code>, <code>@HeaderParam</code>,
  <code>@Consumes</code>, <code>@Produces</code>) and get the same API as any of the HTTP client generator's Java
  libraries, or as Rust <code>reqwest</code> / <code>ureq</code>. XML bodies stay XML: the Spring converters,
  Jackson's <code>XmlMapper</code> (JAXB annotations honoured), a JAXB converter for Retrofit, or
  <code>quick-xml</code> in Rust. Form, matrix and cookie parameters are reported, not converted.
</p>

<h2>Class generators</h2>
<ul>
  <li><strong>DTO and mapper</strong> — a record, Lombok or plain DTO from the entity's chosen fields, and a MapStruct interface or a hand-written mapper (static or a Spring component), both directions and lists.</li>
  <li><strong>Builder</strong> — a nested fluent builder written into the class, with <code>toBuilder()</code> and required-field checks; or Lombok's <code>@Builder</code> on it.</li>
  <li><strong>equals, hashCode and toString</strong> — over every field, the id alone (the default for an entity) or a chosen set; arrays and floating-point fields are compared correctly.</li>
  <li><strong>Test class</strong> — a MockMvc test with a call per known endpoint for a controller, a Mockito test with the dependencies mocked for a service, a <code>@DataJpaTest</code> for a repository, plain JUnit otherwise; JUnit 4 or 5.</li>
</ul>

<h2>Rust generators</h2>
<ul>
  <li><strong>Tauri command</strong> — the <code>#[tauri::command]</code> function at the caret (state, app handle, window, async, <code>Result</code>), its <code>generate_handler!</code> line and a typed <code>invoke()</code> wrapper with camelCase arguments.</li>
  <li><strong>HTTP client</strong> — from an endpoint description (method, path with <code>{'{id}'}</code> variables, query and header parameters, body and response types): <code>reqwest</code> async or blocking, or <code>ureq</code>; serde structs for the payloads, an error enum (<code>thiserror</code> or hand-written), bearer / basic / API-key authentication, a timeout, and the <code>Cargo.toml</code> lines it needs.</li>
  <li><strong>ratatui application</strong> — terminal init and restore, the event loop with an optional tick, an <code>App</code> with its state, <code>draw</code> and key handling; <code>ratatui::init()</code> from 0.28, the manual crossterm set-up before.</li>
  <li><strong>ratatui widget or component</strong> — a <code>Widget</code> (also for <code>&amp;T</code>), a <code>StatefulWidget</code> with its state, or a component with <code>handle_key</code> and <code>render</code>.</li>
  <li><strong>ratatui list or table screen</strong> — the row type, wrapping selection (next, previous, first, last), the highlighted render and the arrow, Home/End and vim keys.</li>
  <li><strong>ratatui key map</strong> — an <code>Action</code> enum and <code>from_key</code> written from specs such as <code>q, ctrl+c</code> or <code>shift+tab</code>, with the labels and descriptions of a help line; a key bound twice is refused.</li>
  <li><strong>ratatui popup, confirmation or input</strong> — a centred popup, a yes/no dialog that reports the answer, or a one-line text input with a cursor, drawn over <code>Clear</code>.</li>
  <li><strong>Bevy component, system or plugin</strong> — components, resources, events (messages from 0.17), states, systems with a filtered query and resources, and plugins that register them.</li>
</ul>

<Callout variant="tip">
  The generators write sensible, ordinary code and stop there. The templates under <em>Code templates</em>
  are the way to shape a generator's output to your own style when a project needs something different.
</Callout>
