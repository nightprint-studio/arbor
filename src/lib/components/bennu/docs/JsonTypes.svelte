<script lang="ts">
  /**
   * JSON types: what a Jackson DTO will and will not serialise, read from the class — the checks, the creator whose arguments
   * have no names, and the gate that keeps the checks honest.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>JSON types</h1>

<p class="doc-lead">
  What a DTO will and will not put in its JSON, read from the class — instead of discovered from a payload that came out wrong, on the other side of an HTTP call, days later.
</p>

<h2>Jackson fails quietly</h2>
<p>Almost nothing here throws where you can see it:</p>
<ul>
  <li>A field with no accessor is simply <strong>absent</strong> from the payload.</li>
  <li>A property two members both claim throws only when that object is serialised — in practice, in front of a customer.</li>
  <li>A <code>@JsonIgnore</code> meant as "not on the way out" takes the property off the way <em>in</em> as well.</li>
</ul>
<p>
  None of it shows in the class: you see a field, spelled correctly, with an annotation on it, and the first person to notice has no reason to suspect the DTO.
</p>

<h2>What is checked</h2>
<table>
  <thead><tr><th>Check</th><th>Why</th></tr></thead>
  <tbody>
    <tr><td><strong>Two members claiming one JSON name</strong> — an error</td><td>Jackson refuses to serialise the type at all. A field and <em>its own</em> getter are one property; what conflicts is two fields, or two members both naming themselves with <code>@JsonProperty</code>.</td></tr>
    <tr><td><strong>A field nothing can reach</strong></td><td>Not public, no accessor: it will not appear in the JSON.</td></tr>
    <tr><td><strong><code>@JsonIgnore</code> and <code>@JsonProperty</code> on one element</strong></td><td>Jackson drops the property, which is unlikely to be what the <code>@JsonProperty</code> was for.</td></tr>
  </tbody>
</table>
<p>
  Hover on a field or an accessor says what it will be called in the JSON, and why — named by <code>@JsonProperty</code>, named after the member, or not serialised at all.
</p>

<h2>A creator whose arguments have no names</h2>
<pre><code>{@html highlightCode(`public class Money {
    @JsonCreator
    public Money(String currency, long amount) { … }
}`, 'java')}</code></pre>
<p>
  A <code>@JsonCreator</code> with two or more arguments is <em>property-based</em>: Jackson matches each argument to a JSON field <strong>by name</strong>. Java keeps
  parameter names in the class file only when the compiler is told to:
</p>
<pre><code>{@html highlightCode(`<properties>
    <maven.compiler.parameters>true</maven.compiler.parameters>
</properties>`, 'markup')}</code></pre>
<p>
  — or <code>-parameters</code>. Without it the names are <code>arg0</code> and <code>arg1</code>, and deserialising fails with <em>"Argument #0 of constructor has no property
  name annotation"</em>.
</p>
<Callout variant="warning" title="The defect lives in two files">
  Neither is wrong on its own: the constructor is ordinary, the pom is ordinary. It is the pair, in files nobody reads together — which is why it survives review and turns
  up the first time somebody POSTs to that endpoint.
</Callout>
<p>Deliberately not reported, because all are correct:</p>
<ul>
  <li>a <strong>single-argument</strong> creator — Jackson reads it as delegating;</li>
  <li>an explicit <code>mode = Mode.DELEGATING</code>;</li>
  <li>a <strong>record</strong>, whose component names are in the class file regardless;</li>
  <li><code>@ConstructorProperties</code>, carrying the names as data.</li>
</ul>
<p>
  Nor is anything said when no pom can be read — a Gradle build is not evidence of a missing flag. <code>spring-boot-starter-parent</code> sets the flag for you, and is
  recognised as such.
</p>

<h2>The gate that keeps it usable</h2>
<p>
  Jackson's visibility rules are configurable globally at run time, per class with <code>@JsonAutoDetect</code>, and through mix-ins registered somewhere else entirely. So
  the reachability check runs <strong>only</strong> where the class says plainly what it is: it carries Jackson annotations, sets no visibility of its own, and generates no
  accessors through Lombok.
</p>
<Callout variant="info" title="That gate is the whole design">
  Without it the check fires on every Lombok DTO in the project — all of them — and a check wrong about all of them is turned off before it is ever right about one.
</Callout>
