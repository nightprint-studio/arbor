<script lang="ts">
  /**
   * Spring proxies and declarations: annotations a proxy cannot honour, bean declarations Spring refuses or ignores, and a
   * transaction held open across a network call.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Spring</span>
<h1>Spring proxies and declarations</h1>

<p class="doc-lead">
  The mistakes that cost most in Spring compile, start and look right. A transaction that never opens, a bean that is never created, a field that stays
  <code>null</code>: each is one line, and the line gives no sign. Bennu marks them as you type.
</p>
<p>
  Every check on this page resolves an annotation through the file's imports, so only Spring's own are judged — see <strong>An annotation is checked by origin</strong>
  on the Spring page. <kbd>Alt</kbd> + <kbd>Enter</kbd> offers the fix wherever the repair is mechanical.
</p>

<h2>When an annotation on a method does nothing</h2>
<p>
  <code>@Transactional</code>, <code>@Async</code>, <code>@Cacheable</code> and the authorization annotations are not implemented by the method they are on, but by a
  <strong>proxy</strong> in front of the bean — and anything reaching the bean without the proxy gets the bare method:
</p>
<pre><code>{@html highlightCode(`@Service
public class OrderService {
    public void placeAll(List<Order> orders) {
        for (Order order : orders) {
            place(order);          // through "this", not the proxy: no transaction
        }
    }

    @Transactional
    public void place(Order order) { … }
}`, 'java')}</code></pre>
<p>The annotations judged this way, each named in the message by what it would have done:</p>
<table>
  <thead><tr><th>Annotation</th><th>What the proxy would have provided</th></tr></thead>
  <tbody>
    <tr><td><code>@Transactional</code></td><td>The transaction</td></tr>
    <tr><td><code>@Async</code></td><td>The asynchronous dispatch</td></tr>
    <tr><td><code>@Cacheable</code>, <code>@CachePut</code>, <code>@CacheEvict</code></td><td>The cache lookup, write and eviction</td></tr>
    <tr><td><code>@PreAuthorize</code>, <code>@PostAuthorize</code>, <code>@Secured</code>, <code>@RolesAllowed</code></td><td>The authorization check</td></tr>
    <tr><td><code>@Retryable</code></td><td>The retry</td></tr>
    <tr><td><code>@Validated</code></td><td>The argument validation</td></tr>
  </tbody>
</table>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Reported on the call</div>
    <div class="fc-title">Self-invocation</div>
    <div class="fc-desc">A call from another method of the same class. It is reported on the call — the line that is wrong, and the one carrying no annotation to draw the eye.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Ignored by proxy AOP</div>
    <div class="fc-title">A non-public method</div>
    <div class="fc-desc">Proxy-based AOP skips it outright.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Cannot be overridden</div>
    <div class="fc-title">A final method or class</div>
    <div class="fc-desc">The proxy cannot override it. <kbd>Alt</kbd> + <kbd>Enter</kbd> removes the <code>final</code>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Never through the bean</div>
    <div class="fc-title">A static method</div>
    <div class="fc-desc">A static call never reaches the proxy.</div>
  </div>
</div>
<p><kbd>Alt</kbd> + <kbd>Enter</kbd> on a non-public method makes it public.</p>
<Callout variant="info" title="Quiet under AspectJ">
  A project weaving with AspectJ — <code>mode = AdviceMode.ASPECTJ</code>, or <code>mode="aspectj"</code> in XML — gets none of this, since all of it works there. Nor is
  recursion reported (the first call went through the proxy), or a bare call inside a nested class, which resolves to the nested class's own member.
</Callout>

<h2>Bean declarations Spring refuses or ignores</h2>
<p>
  Some mistakes in a bean declaration stop the context at startup, a build and a deploy away from the line that caused them. Others do nothing at all: no error, and
  the field stays <code>null</code>. Both are marked as you type:
</p>
<pre><code>{@html highlightCode(`@Configuration
public class DataConfig {
    @Bean
    private DataSource dataSource() { … }   // Spring refuses to start: must not be private or final

    @Autowired
    private static Clock clock;              // never injected: stays null
}`, 'java')}</code></pre>
<table>
  <thead><tr><th>Declaration</th><th>What happens</th><th><kbd>Alt</kbd> + <kbd>Enter</kbd></th></tr></thead>
  <tbody>
    <tr><td>A <code>private</code> or <code>final</code> <code>@Bean</code> method in a <code>@Configuration</code></td><td>Refused at startup — Spring subclasses the class to intercept its <code>@Bean</code> methods</td><td>Remove the modifiers</td></tr>
    <tr><td>A <code>final</code> <code>@Configuration</code> class, or one whose constructors are all private</td><td>Refused at startup, for the same reason</td><td>Remove <code>final</code></td></tr>
    <tr><td>A <code>void</code> <code>@Bean</code> method</td><td>Refused at startup — the bean is what the method returns</td><td>—</td></tr>
    <tr><td>A non-static <code>@Bean</code> returning a <code>BeanFactoryPostProcessor</code>, such as <code>PropertySourcesPlaceholderConfigurer</code></td><td>The class is created too early, and its own <code>@Autowired</code>, <code>@Value</code> and <code>@PostConstruct</code> are skipped</td><td>Make it static</td></tr>
    <tr><td><code>@Autowired</code>, <code>@Inject</code> or <code>@Value</code> on a static field or method</td><td>Skipped: the field stays <code>null</code></td><td>—</td></tr>
    <tr><td><code>@Resource</code> on a static field or method</td><td>Refused at startup</td><td>—</td></tr>
    <tr><td><code>@Component</code>, <code>@Service</code>, <code>@Controller</code> or <code>@RestController</code> on an interface or an abstract class</td><td>No bean: scanning only registers concrete classes, and subclasses do not inherit the annotation</td><td>Remove the annotation</td></tr>
    <tr><td>The same annotations on a non-static inner class</td><td>No bean: it needs an instance of its enclosing class</td><td>Make it static</td></tr>
    <tr><td>An <code>@Async</code> method returning a value that is not a <code>Future</code></td><td>The caller gets <code>null</code> — the call returns before the method runs</td><td>—</td></tr>
  </tbody>
</table>
<Callout variant="info" title="What is left alone">
  <code>@Configuration(proxyBeanMethods = false)</code> subclasses nothing, so the modifier and constructor rules do not apply to it — and a
  <code>proxyBeanMethods</code> set from a constant is not judged either way. <code>@SpringBootApplication</code>, <code>@SpringBootConfiguration</code> and
  <code>@TestConfiguration</code> count as <code>@Configuration</code>; your own annotations meta-annotated with it do not. <code>@Repository</code> on an
  interface is never reported, since a Spring Data repository is a bean; neither is an interface that extends another or carries any other annotation, such as a
  Feign client, nor an abstract class with <code>@Lookup</code> methods. An annotation of the same name that is not Spring's — your own <code>@Bean</code>, or
  Lombok's <code>@Value</code> — is not judged by Spring's rules.
</Callout>

<h2>A transaction held open across a network call</h2>
<p>
  A <code>@Transactional</code> method borrows a connection from the pool for its whole duration. A <code>RestTemplate</code> call inside one keeps that connection for as
  long as the other server takes to answer:
</p>
<pre><code>{@html highlightCode(`@Transactional
public void confirm(Order order) {
    orders.save(order);
    rest.postForObject(paymentsUrl, order, Receipt.class);   // the connection waits on another server
}`, 'java')}</code></pre>
<Callout variant="warning" title="What you are shown is a slow database">
  On a quiet afternoon this is fine. Under load, ten concurrent calls to a four-second endpoint hold every connection while doing no database work: connection wait up,
  queries queueing, pool saturated, database idle — and the cause is one line that looks like an ordinary call to a collaborator.
</Callout>
<ul>
  <li>A remote call is recognised by the <strong>declared type of the receiver</strong>, never by the method name — <code>execute</code>, <code>send</code> and
    <code>get</code> are on everything — so your own repositories, mappers and services are never mistaken for one, and a Feign client or a generated stub is left alone
    rather than guessed at.</li>
  <li>The receiver types known to talk to another process are <code>RestTemplate</code>, <code>TestRestTemplate</code>, <code>RestClient</code>, <code>WebClient</code>,
    <code>OkHttpClient</code>, <code>CloseableHttpClient</code>, <code>HttpClient</code>, <code>HttpURLConnection</code>, <code>URLConnection</code>, <code>WebTarget</code>,
    <code>Jedis</code> and <code>MongoClient</code>. A name merely ending in <code>Client</code> is not enough: that would sweep in the project's own gateway classes.</li>
  <li><code>Thread.sleep</code> inside a transaction is the same defect without a server at the other end.</li>
  <li>A propagation that <em>suspends</em> the transaction — <code>NOT_SUPPORTED</code>, <code>NEVER</code> — holds nothing, and is left alone: it is the fix.</li>
</ul>
