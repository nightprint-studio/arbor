<script lang="ts">
  /**
   * The language code templates are written in, taught by example: every idea as a template and what it
   * writes, side by side. Jinja is a language people already know from elsewhere, so this is the part of
   * it a code template uses — not a copy of its manual.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Code templates</span>
<h1>The template language</h1>

<p class="doc-lead">
  Templates are written in <strong>Jinja</strong>, the template language many web frameworks use. You do not
  need to know it to change a template — most changes are to the code around the tags — but a few minutes
  with this page makes the tags read like the code they are.
</p>

<p>
  A template is ordinary text with <strong>tags</strong> in it. Everything outside a tag is written out as it
  is. There are three kinds of tag:
</p>

<table>
  <thead>
    <tr><th>Tag</th><th>Does</th><th>Example</th></tr>
  </thead>
  <tbody>
    <tr><td><code>{'{{'} … {'}}'}</code></td><td>Prints a value</td><td><code>{'{{'} class.name {'}}'}</code></td></tr>
    <tr><td><code>{'{%'} … {'%}'}</code></td><td>Decides what is written: conditions, loops, variables</td><td><code>{'{%'} if field.id {'%}'}</code></td></tr>
    <tr><td><code>{'{#'} … {'#}'}</code></td><td>A comment — writes nothing</td><td><code>{'{#'} TODO: getters {'#}'}</code></td></tr>
  </tbody>
</table>

<p>
  The examples below run on a class like this one, which is what a <em>From a class</em> template reads as
  <code>class</code>:
</p>
<pre><code>{@html highlightCode(`public class OrderLine {
    @Id private Long id;
    private String productName;
    private int quantity;
}`, 'java')}</code></pre>

<h2>Printing values</h2>
<p>
  <code>{'{{'} … {'}}'}</code> prints what is inside it. A dot goes into a value — <code>class.name</code> is the
  class's name — and square brackets pick an item of a list.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Template</div>
    <pre><code>{@html highlightCode(`// {{ class.name }} has {{ class.fields | length }} fields
// the first is {{ class.fields[0].name }}`, 'jinja-java')}</code></pre>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Writes</div>
    <pre><code>{@html highlightCode(`// OrderLine has 3 fields
// the first is id`, 'java')}</code></pre>
  </div>
</div>
<p>
  <code>~</code> joins text: <code>{'{{'} class.name ~ "Repository" {'}}'}</code> is <code>OrderLineRepository</code>.
  Every variable a kind of template can read is listed in <strong>Template reference</strong>, and the editor
  offers them as you type inside a tag.
</p>

<h2>Filters</h2>
<p>
  A <strong>filter</strong> changes a value on its way out. It follows the value after a <code>|</code>, and
  filters chain from left to right.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Template</div>
    <pre><code>{@html highlightCode(`{{ class.name | snake }}
{{ "product_name" | camel }}
{{ class.name | snake | upper }}`, 'jinja')}</code></pre>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Writes</div>
    <pre><code>{`order_line
productName
ORDER_LINE`}</code></pre>
  </div>
</div>

<p>Bennu adds four filters for writing Java:</p>
<ul class="prop-list">
  <li><code>snake</code><code>customerName</code> → <code>customer_name</code></li>
  <li><code>camel</code><code>customer_name</code> → <code>customerName</code></li>
  <li><code>pascal</code><code>customer_name</code> → <code>CustomerName</code></li>
  <li><code>java_string</code>any text as a Java string literal, quoted and escaped</li>
  <li><code>imported</code><code>"java.time.LocalDate" | imported</code> is <code>LocalDate</code> — and the file being written gets the import</li>
  <li><code>arguments</code>in validation tests, <code>valid | arguments</code> is every valid value as a constructor's arguments; <code>arguments(field.name, case.value_java)</code> changes that one field's</li>
</ul>

<p>And these from Jinja itself come up the most:</p>
<table>
  <thead>
    <tr><th>Filter</th><th>Example</th><th>Writes</th></tr>
  </thead>
  <tbody>
    <tr><td><code>upper</code>, <code>lower</code>, <code>capitalize</code></td><td><code>{'{{'} "id" | upper {'}}'}</code></td><td><code>ID</code></td></tr>
    <tr><td><code>replace</code></td><td><code>{'{{'} class.name | replace("Line", "Row") {'}}'}</code></td><td><code>OrderRow</code></td></tr>
    <tr><td><code>length</code></td><td><code>{'{{'} class.fields | length {'}}'}</code></td><td><code>3</code></td></tr>
    <tr><td><code>join</code></td><td><code>{'{{'} class.fields | map(attribute="name") | join(", ") {'}}'}</code></td><td><code>id, productName, quantity</code></td></tr>
    <tr><td><code>default</code></td><td><code>{'{{'} class.package | default("com.example") {'}}'}</code></td><td>the package, or <code>com.example</code> when there is none</td></tr>
    <tr><td><code>first</code>, <code>last</code></td><td><code>{'{{'} (class.fields | last).name {'}}'}</code></td><td><code>quantity</code></td></tr>
    <tr><td><code>selectattr</code>, <code>rejectattr</code></td><td><code>{'{%'} for f in class.fields | rejectattr("id") {'%}'}</code></td><td>every field but the <code>@Id</code></td></tr>
    <tr><td><code>indent</code></td><td><code>{'{{'} body | indent(4) {'}}'}</code></td><td>every line after the first moved in by four spaces</td></tr>
  </tbody>
</table>

<h2>Conditions</h2>
<p>
  <code>{'{%'} if … {'%}'}</code> writes what follows only when the condition holds, up to
  <code>{'{%'} endif {'%}'}</code>. <code>elif</code> and <code>else</code> give the other cases. Conditions compare
  with <code>==</code>, <code>!=</code>, <code>&lt;</code>, <code>&gt;</code>, combine with <code>and</code>,
  <code>or</code>, <code>not</code>, and ask whether something is in a list with <code>in</code>.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Template</div>
    <pre><code>{@html highlightCode(`{% for field in class.fields %}
{% if field.id %}
// {{ field.name }} is the key
{% elif field.type_simple == "int" %}
// {{ field.name }} is a number
{% else %}
// {{ field.name }}
{% endif %}
{% endfor %}`, 'jinja-java')}</code></pre>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Writes</div>
    <pre><code>{@html highlightCode(`// id is the key
// productName
// quantity is a number`, 'java')}</code></pre>
  </div>
</div>
<p>
  A <strong>test</strong> asks what kind of value something is, with <code>is</code>:
  <code>{'{%'} if class.superclass is none {'%}'}</code>, <code>{'{%'} if loop.index is even {'%}'}</code>,
  <code>{'{%'} if extra is defined {'%}'}</code>. The editor offers them after <code>is</code>.
</p>

<h2>Loops</h2>
<p>
  <code>{'{%'} for … in … {'%}'}</code> writes its body once for every item of a list, up to
  <code>{'{%'} endfor {'%}'}</code>. Inside it, <code>loop</code> says where you are: <code>loop.index</code>
  counts from 1, <code>loop.first</code> and <code>loop.last</code> are true on the first and last time round —
  which is how a list is written with commas between its items and none after the last.
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Template</div>
    <pre><code>{@html highlightCode(`public {{ class.name }}(
{% for field in class.fields %}
        {{ field.type_name }} {{ field.name }}{% if not loop.last %},{% endif %}

{% endfor %}
) {`, 'jinja-java')}</code></pre>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Writes</div>
    <pre><code>{@html highlightCode(`public OrderLine(
        Long id,
        String productName,
        int quantity
) {`, 'java')}</code></pre>
  </div>
</div>
<p>
  A loop can skip items as it goes — <code>{'{%'} for field in class.fields if not field.final {'%}'}</code> —
  and an <code>{'{%'} else {'%}'}</code> inside it is written when the list is empty.
</p>

<h3>Loops that go deeper</h3>
<p>
  Some data nests — a configuration class has types inside types. A loop marked <code>recursive</code> can call
  itself on the children with <code>loop(…)</code>, and <code>loop.depth0</code> says how deep it is, starting at 0:
</p>
<pre><code>{@html highlightCode(`{% for type in [root] recursive %}
{{ type.indent }}public record {{ type.name }}(…) {
{% if type.types %}
{{ loop(type.types) -}}
{% endif %}
{{ type.indent }}}
{% endfor %}`, 'jinja-java')}</code></pre>

<h2>Variables</h2>
<p>
  <code>{'{%'} set … {'%}'}</code> gives a value a name, so a long expression is written once:
</p>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Template</div>
    <pre><code>{@html highlightCode(`{% set builder = class.name ~ "Builder" %}
public static {{ builder }} builder() {
    return new {{ builder }}();
}`, 'jinja-java')}</code></pre>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Writes</div>
    <pre><code>{@html highlightCode(`public static OrderLineBuilder builder() {
    return new OrderLineBuilder();
}`, 'java')}</code></pre>
  </div>
</div>

<h2>Macros</h2>
<p>
  A <strong>macro</strong> is a piece of template with a name and parameters, written once and used as often as
  needed — the template's own function:
</p>
<pre><code>{@html highlightCode(`{% macro getter(field) %}
public {{ field.type_name }} get{{ field.name | pascal }}() {
    return {{ field.name }};
}
{% endmacro %}

{% for field in class.fields %}
{{ getter(field) }}
{% endfor %}`, 'jinja-java')}</code></pre>

<h2>Blank lines and indentation</h2>
<p>
  A line that holds nothing but a <code>{'{%'} … {'%}'}</code> tag leaves nothing behind — not even the line —
  so a template can put every <code>if</code> and <code>for</code> on a line of its own and still write code
  without gaps. That is why the examples above look the way they do.
</p>
<p>
  The flip side: a line that <em>ends</em> in a tag after some text loses its line break with the tag. In the
  constructor above, the blank line after <code>{'{%'} endif {'%}'}</code> is what puts each parameter on a line
  of its own. And a <code>-</code> inside a tag removes the spaces and line breaks on that side of it:
  <code>{'{{'} value -{'}}'}</code> swallows everything up to the next text, <code>{'{%'}- if {'%}'}</code> everything
  before it.
</p>

<Callout variant="warning" title="A name that does not exist prints nothing">
  <code>{'{{'} class.nmae {'}}'}</code> writes an empty string rather than failing, so a typo shows up as a gap in
  the output. The preview beside the template is the quickest way to catch it — and the editor offers the real
  names inside every tag.
</Callout>

<h2>Comments and directives</h2>
<p>
  <code>{'{#'} … {'#}'}</code> is a comment: it writes nothing, and can span lines. A comment line that starts
  with <code>bennu.</code> is a <strong>directive</strong> — something the template tells Bennu about itself,
  such as what it is for or which file it writes:
</p>
<pre><code>{@html highlightCode(`{# bennu.description: A Spring Data repository for the entity -#}
{# bennu.output: file -#}
{# bennu.file: {{ class.name }}Repository.java -#}`, 'jinja-java')}</code></pre>
<p>Every directive is in <strong>Template reference</strong>.</p>

<Callout variant="tip" title="Trying something out">
  The preview's <strong>Parameters</strong> lay values over what the template reads — <code>{'{'} "class": {'{'} "name": "Invoice" {'}'} {'}'}</code>
  — so a branch the class at hand does not take can be seen without looking for a class that would.
</Callout>
