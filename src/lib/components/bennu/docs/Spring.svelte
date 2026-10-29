<script lang="ts">
  /**
   * Spring, overview: what switches the tooling on, the wiring in the gutter, bean XML, how an annotation is told from a
   * look-alike, and the panels. Configuration, the web side and the proxy diagnostics have their own pages.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Spring</span>
<h1>Spring</h1>

<p class="doc-lead">
  In a Spring application a large part of the behaviour is declared rather than written — in annotations, in XML, in
  <code>application.yml</code>. Bennu reads that as part of the program, and puts it where you are already looking.
</p>
<p>
  It switches on for a project that uses Spring, detected like every other capability from the pom and the sources. Nothing in this section appears on a
  project without Spring.
</p>

<table>
  <thead><tr><th>Page</th><th>Covers</th></tr></thead>
  <tbody>
    <tr><td><strong>Spring</strong> (this page)</td><td>Beans and injection in the gutter, bean XML, how annotations are recognised, the bean panels</td></tr>
    <tr><td><strong>Spring configuration</strong></td><td>Keys inside annotation strings, <code>@ConfigurationProperties</code>, conditional beans, the YAML side, writing a property file</td></tr>
    <tr><td><strong>Spring web</strong></td><td>The Endpoints panel, Struts actions beside Spring routes, endpoints checked against each other</td></tr>
    <tr><td><strong>Spring proxies and declarations</strong></td><td>Annotations that do nothing, bean declarations Spring refuses or ignores, a transaction held across a network call</td></tr>
  </tbody>
</table>
<p>
  Neighbouring subjects live with their own tool: Spring Data derived queries and repositories under <strong>JPA</strong>, <code>@Scheduled</code> under
  <strong>Scheduled jobs</strong>, <code>@SpringBootTest</code> next to the Mockito checks, Spring Boot launch configurations and active profiles under
  <strong>Running</strong>.
</p>

<h2>In the gutter</h2>
<table>
  <thead><tr><th>Icon</th><th>Beside</th><th>Click to</th></tr></thead>
  <tbody>
    <tr><td><code>I</code></td><td>A bean declaration (green badge)</td><td>See where it is injected</td></tr>
    <tr><td><code>B</code></td><td>An injection point (blue badge)</td><td>See the beans that can satisfy it</td></tr>
    <tr><td><code>B</code></td><td>An injection only a dependency can satisfy (dashed teal badge)</td><td>See what may provide it — an allowlisted artifact, or Spring Boot's auto-configuration, read from each jar's <code>AutoConfiguration.imports</code> or <code>spring.factories</code>. The tooltip says whose bean it is and on what condition: a library bean is what Spring <em>may</em> register, never a fact. A bean a method returns as a subtype (a <code>HikariDataSource</code> for a <code>DataSource</code>) is matched by the type it returns.</td></tr>
    <tr><td><code>»</code></td><td>A request handler</td><td>See its route</td></tr>
  </tbody>
</table>
<p>
  Injection points include the ones with no constructor in sight: a single constructor needs no <code>@Autowired</code>, and Lombok's generated one makes
  the <code>final</code> fields themselves the injection points.
</p>
<Callout variant="tip" title="A jump with several destinations asks">
  A bean injected in six places, a key read from three — Bennu asks instead of picking. The menu opens at the pointer for a gutter icon and at the caret
  for <kbd>Ctrl</kbd> + <kbd>B</kbd>, and each entry says what kind of site it is, which tells two injections of one bean apart.
</Callout>

<h2>A bean named as a string</h2>
<p>
  <code>@Qualifier("fast")</code>, <code>@DependsOn("audit")</code>, <code>@Resource(name = "ds")</code> and a SpEL <code>@beanName</code> name a bean the way
  an injection names a type: one colour, and <kbd>Ctrl</kbd> + <kbd>B</kbd> follows them to the declaration.
</p>
<Callout variant="info" title="A bean name is not a class name">
  <code>@ConditionalOnBean(name = "…")</code> is left alone: it names a bean, not a type, however much the two look alike.
</Callout>

<h2>Bean XML</h2>
<p>
  <code>&lt;bean class=&gt;</code>, <code>ref=</code> and <code>&lt;property name=&gt;</code> navigate and complete, and a <code>&lt;property&gt;</code> naming
  nothing writable on the bean's class is flagged. Every check here speaks only when it is sure:
</p>
<ul>
  <li>the property check stays quiet on a class extending something outside the project, or carrying a Lombok annotation whose accessors Bennu does not model;</li>
  <li>a missing <code>class=</code> is reported only when its package is one the project declares;</li>
  <li>an unknown <code>ref=</code> only when it looks like a typo of a bean that exists — a bean can legitimately come from a jar;</li>
  <li>an unresolved <code>$&#123;key&#125;</code> only when it has no default and the project already configures other keys in that namespace.</li>
</ul>

<h2>An annotation is checked by origin, not by name</h2>
<p>
  <code>@Service</code> is not a reserved word; a project can declare its own, and several do. Each annotation is resolved through the file's imports as the
  compiler would: a qualified use decides outright, then an explicit <code>import</code>, then a wildcard import of the expected package — and a bare name with
  no import can only be a class of the same package, so it is not Spring's. Your <code>com.acme.Service</code> declares no bean, gets no gutter icon and is
  counted in no panel.
</p>
<p>The same rule gates every check on the other Spring pages: an annotation of the same name that is not Spring's is never judged by Spring's rules.</p>
<Callout variant="warning" title="Meta-annotations are not recognised">
  Your own <code>@MyService</code>, itself annotated <code>@Service</code>, is a real stereotype and is missed — which loses a bean rather than inventing one.
</Callout>

<h2>The panels</h2>
<table>
  <thead><tr><th>Panel</th><th>Lists</th></tr></thead>
  <tbody>
    <tr><td><strong>Spring beans</strong> <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F12</kbd></td><td>Every bean, with its stereotype, scope and profile</td></tr>
    <tr><td><strong>Spring configuration</strong></td><td>Every property key, with its value and source file</td></tr>
    <tr><td><strong>Spring bound properties</strong></td><td>What each <code>@ConfigurationProperties</code> field binds</td></tr>
    <tr><td><strong>Spring property reference</strong></td><td>Everything the dependencies accept, set or not</td></tr>
    <tr><td><strong>Spring beans from libraries</strong></td><td>The beans declared inside the dependencies you name under <strong>Settings → Spring</strong> — by group id, artifact id or a prefix — read from bytecode, one row per artifact</td></tr>
  </tbody>
</table>
<p>All but Endpoints open from the command palette, and each row opens its declaration. Endpoints has its own button and shortcut — see <strong>Spring web</strong>.</p>
<Callout variant="warning" title="A library's beans are declarations, not facts">
  Boot's auto-configuration is gated — <code>@ConditionalOnMissingBean</code>, <code>@ConditionalOnClass</code>, <code>@ConditionalOnProperty</code> — so a bean in a
  jar is what Spring <em>may</em> register. Each is shown with its conditions, and none takes part in autowiring, completion or any diagnostic: your own beans stay the
  only answer to what this application has.
</Callout>
<p>
  Each panel is offered only where it has something to say: Spring on the classpath turns the tooling on, and what the model found decides which panels exist. A batch
  job with no request mappings gets no Endpoints button, no <kbd>Alt</kbd> + <kbd>4</kbd> and no palette entry. Panels appear as the index finishes, and again after a
  rebuild finds the first route.
</p>

<h2>Where the pieces are checked</h2>
<p>
  The bean-level diagnostics — a declaration Spring refuses, an injection into a static field, an annotation a proxy cannot honour — are marked as you type and
  carry a quick fix when the repair is mechanical. They are described, with the rules for what they leave alone, in <strong>Spring proxies and declarations</strong>.
</p>
<pre><code>{@html highlightCode(`@Service
public class Mailer {
    private final Sender sender;      // injection point: B in the gutter, resolves to the bean

    public Mailer(Sender sender) {    // a single constructor needs no @Autowired
        this.sender = sender;
    }
}`, 'java')}</code></pre>
