<script lang="ts">
  /**
   * Everything a code template can say and read, kind by kind, each with a template that works — the page
   * to keep open while writing one. The guide is `Templates`, the language `TemplateLanguage`.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Code templates</span>
<h1>Template reference</h1>

<p class="doc-lead">
  What a template can tell Bennu about itself, what it can read — about the project, your style and the thing
  it is generated from — and a complete template for every kind to start from.
</p>

<h2>Directives</h2>
<p>
  A directive is a comment line starting with <code>bennu.</code>, usually at the top of the template. The
  <code>-</code> before <code>{'#}'}</code> keeps the line from leaving a blank one behind.
</p>
<pre><code>{@html highlightCode(`{# bennu.description: A Spring Data repository for the entity -#}
{# bennu.output: file -#}
{# bennu.file: {{ class.name }}Repository.java -#}
{# bennu.requires: spring-data-jpa -#}`, 'jinja-java')}</code></pre>

<table>
  <thead>
    <tr><th>Directive</th><th>Kinds</th><th>What it says</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><code>bennu.description</code></td>
      <td>All</td>
      <td>What the template is for, shown next to its name in every list and in completion.</td>
    </tr>
    <tr>
      <td><code>bennu.output</code></td>
      <td>From a class</td>
      <td><code>members</code> — inserted into the class, the default; <code>file</code> — a new file beside it; <code>text</code> — to copy.</td>
    </tr>
    <tr>
      <td><code>bennu.file</code></td>
      <td>From a class, New file, Configuration class</td>
      <td>The name of the file written, relative to the class's folder or the folder chosen. It is a template too: <code>{'{{'} class.name {'}}'}Repository.java</code>.</td>
    </tr>
    <tr>
      <td><code>bennu.requires</code></td>
      <td>All</td>
      <td>What a project needs for the template to be offered — see below.</td>
    </tr>
    <tr>
      <td><code>bennu.constants</code></td>
      <td>Validation tests</td>
      <td>Classes to look up the constants holding each expected message in — see <strong>DTO Lab</strong>.</td>
    </tr>
    <tr>
      <td><code>bennu.abbrev</code></td>
      <td>Abbreviations</td>
      <td>The word that expands it, when that is not its file name — so the file can be called <code>logger-for-this-class</code> and the word stay <code>logd</code>.</td>
    </tr>
    <tr>
      <td><code>bennu.stops</code></td>
      <td>From a class</td>
      <td><code>true</code> reads the output as a snippet: <code>$1</code>, <code>{'${'}1:name{'}'}</code> and <code>$0</code> become tab stops in the class. Off by default — generated code and snippets both use <code>$</code>.</td>
    </tr>
  </tbody>
</table>

<h2>When a template is offered</h2>
<p>
  <code>bennu.requires</code> lists what a project must have, separated by commas. A template whose requirements
  the open project does not meet is left out wherever templates are offered — the New file dialog, the template
  bars, Generate, the abbreviations in completion — and its row in <strong>Settings › Code Templates</strong>
  says why, as in <em>Needs Java 16 or later</em>.
</p>
<table>
  <thead>
    <tr><th>Requirement</th><th>Met when</th></tr>
  </thead>
  <tbody>
    <tr><td><code>java &gt;= 16</code></td><td>The project's Java level is 16 or later.</td></tr>
    <tr><td><code>lombok</code></td><td>A dependency whose artifact is <code>lombok</code> is on the classpath.</td></tr>
    <tr><td><code>org.projectlombok:lombok</code></td><td>The same, by group and artifact — for a name two groups share.</td></tr>
    <tr><td><code>spring-boot &gt;= 3</code></td><td>Spring Boot is there, in version 3 or later.</td></tr>
    <tr><td><code>!lombok</code></td><td>Lombok is <em>not</em> there.</td></tr>
  </tbody>
</table>
<p>
  A version can be bounded with <code>&gt;=</code>, <code>&gt;</code>, <code>&lt;=</code>, <code>&lt;</code>,
  <code>=</code> and <code>!=</code>, and versions compare number by number: <code>1.18.30</code> is later than
  <code>1.18.4</code>. Until the project's Java level and classpath are known, nothing is held back.
</p>
<Callout variant="info" title="How a project's template is chosen">
  Until a project picks a template with <strong>Use for this project</strong>, it generates with the first one it
  meets. The built-in configuration classes are listed record first (<code>java &gt;= 16</code>), then Lombok
  (<code>lombok</code>), then a plain class — so a project gets the one it can compile, and one of yours with
  its own <code>bennu.requires</code> joins the same choice.
</Callout>

<h2>What every template reads</h2>
<p>
  Besides what its kind reads, every template has <code>project</code>, <code>style</code> and <code>naming</code> —
  and every filter, among them <code>imported</code>, which adds what the code needs to the file's imports.
</p>

<h3><code>project</code> — where it is generated</h3>
<ul class="prop-list">
  <li><code>java</code>the Java level, <code>17</code>; <code>0</code> while it is not known</li>
  <li><code>dependencies</code>every dependency on the classpath, as <code>group:artifact</code></li>
  <li><code>artifacts</code>the same dependencies by artifact alone: <code>lombok</code>, <code>spring-boot</code></li>
  <li><code>versions</code>the version of each, by <code>group:artifact</code></li>
  <li><code>resolved</code>whether the classpath has been read yet</li>
</ul>

<h3><code>style</code> — how you write Java</h3>
<p>
  From <strong>Settings › Java › Code Style</strong>. Its methods write a declaration the way those settings say, so a
  template does not spell the same <code>{'{%'} if {'%}'}</code> chain every time it declares something:
</p>
<ul class="prop-list">
  <li><code>local(type)</code>what goes before a local's name. <code>val</code> when you use Lombok's and the project has Lombok; else <code>var</code> when you use it and the project is on Java 10 or later; else the type — with <code>final</code> in front of <code>var</code> or the type when locals are final. For a local with an initializer, which <code>val</code> and <code>var</code> both need</li>
  <li><code>param(type)</code>a parameter's type, <code>final</code> when parameters are</li>
  <li><code>braces(body)</code>a one-line body in its braces, with a space inside them or without</li>
</ul>
<pre><code>{@html highlightCode(`{% for field in class.fields %}
    {{ style.local(field.type_simple) }} {{ field.name }} = source.{{ field.getter }}();
{% endfor %}

public void rename({{ style.param("String") }} name) {{ style.braces("this.name = name;") }}`, 'jinja-java')}</code></pre>
<pre><code>{@html highlightCode(`    val email = source.getEmail();          // Lombok's val, in a project with Lombok
    final var email = source.getEmail();    // var, and final locals
    String email = source.getEmail();       // neither

public void rename(final String name) { this.name = name; }    // final parameters, spaces in braces`, 'java')}</code></pre>
<p>And the settings themselves, to test in an <code>{'{%'} if {'%}'}</code> of your own:</p>
<ul class="prop-list">
  <li><code>final_params</code>generated parameters and locals are <code>final</code></li>
  <li><code>lombok_val</code>locals are Lombok's <code>val</code> — true only when you prefer it <em>and</em> the project has Lombok</li>
  <li><code>local_var</code>locals are <code>var</code> — true only when you prefer it <em>and</em> the project is on Java 10 or later</li>
  <li><code>switch_with_return</code>a switch that yields a value is an arrow-style switch expression</li>
  <li><code>space_in_braces</code>a one-line body has a space inside its braces</li>
  <li><code>blank_line_between_members</code>members are separated by a blank line</li>
</ul>

<pre><code>{@html highlightCode(`{% if "lombok" in project.artifacts %}
@Builder
{% endif %}
public {% if project.java >= 16 %}record{% else %}final class{% endif %} {{ name }} {`, 'jinja-java')}</code></pre>

<h3><code>naming</code> — how the project names things</h3>
<p>
  The convention for each kind of declaration in the file being written. It comes from
  <strong>Project Configuration → Naming conventions</strong> when the project checks them — for that file's path,
  so a project whose tests are named in snake_case gets snake_case in a test — and from the language's standard
  when it does not. Read alone, a target is its convention; called, it makes a name out of words in it:
</p>
<pre><code>{@html highlightCode(`{% for case in field.cases %}
    @Test
    void {{ naming.method("should reject", field.name, "when", case.name) }}() {
{% endfor %}`, 'jinja-java')}</code></pre>
<pre><code>{@html highlightCode(`    @Test
    void shouldRejectEmailWhenBlank() {         // Java's standard: camelCase

    @Test
    void should_reject_email_when_blank() {     // a project whose tests are snake_case`, 'java')}</code></pre>
<ul class="prop-list">
  <li><code>type</code>, <code>method</code>, <code>field</code>, <code>constant</code>, <code>parameter</code>, <code>local</code>, <code>type_parameter</code>, <code>enum_constant</code>, <code>package</code>each one <code>camelCase</code>, <code>PascalCase</code>, <code>snake_case</code>, <code>UPPER_SNAKE_CASE</code> or <code>lowercase</code> — <code>{'{%'} if naming.method == "snake_case" {'%}'}</code></li>
  <li><code>naming.method(words…)</code>the words — any number, each split where its own case changes — joined into one name; the same for every other target</li>
</ul>

<h2>Imports</h2>
<p>
  A template never has to write an <code>import</code>, or wonder whether the file already has it. A type put
  through <code>imported</code> is written by its simple name, and the file gets the import:
</p>
<pre><code>{@html highlightCode(`private {{ "java.time.LocalDate" | imported }} {{ field.name }}Date;
{{ style.local("java.util.List<String>" | imported) }} names = new ArrayList<>();`, 'jinja-java')}</code></pre>
<pre><code>{@html highlightCode(`import java.time.LocalDate;
import java.util.List;
…
private LocalDate createdDate;
List<String> names = new ArrayList<>();`, 'java')}</code></pre>
<p>
  <code>style.local(…)</code> does the same for Lombok's <code>val</code>, which is a type like any other. Where the
  imports go follows where the output goes:
</p>
<dl class="meta-grid">
  <dt>A new file</dt>
  <dd>Among its own imports, in order.</dd>
  <dt>Members inserted into a class</dt>
  <dd>Into that class's file, in the same undo step as the members.</dd>
  <dt>An abbreviation</dt>
  <dd>Into the file it is typed in, as it expands.</dd>
  <dt>Text to copy</dt>
  <dd>Nowhere — the preview says which imports it needs.</dd>
</dl>
<p>
  Nothing is added for a type in <code>java.lang</code>, in the file's own package, covered by a wildcard import, or
  imported already.
</p>

<h2>New file</h2>
<p>Rendered for the name typed in the New file dialog, in the folder it was opened on.</p>
<ul class="prop-list">
  <li><code>name</code>what was typed, without the extension</li>
  <li><code>package</code>the package the folder is in — empty outside a source folder</li>
  <li><code>file_name</code>the file created: the name and the template's extension</li>
  <li><code>directory</code>the folder it is created in</li>
  <li><code>date</code>, <code>year</code>today, <code>2026-01-31</code>, and its year</li>
</ul>
<pre><code>{@html highlightCode(`{# bennu.description: A Spring service -#}
{% if package %}
package {{ package }};

{% endif %}
import org.springframework.stereotype.Service;

@Service
public class {{ name }} {
}`, 'jinja-java')}</code></pre>

<h2>From a class</h2>
<p>Rendered with the class the caret is in.</p>
<ul class="prop-list">
  <li><code>class.name</code>, <code>class.package</code>, <code>class.fqn</code>the class, its package, its qualified name</li>
  <li><code>class.record</code>whether it is a record</li>
  <li><code>class.annotations</code>each with <code>name</code> and <code>attributes</code>; <code>class.annotation_names</code> just the names</li>
  <li><code>class.superclass</code>, <code>class.interfaces</code>, <code>class.imports</code>what it extends, implements and imports</li>
  <li><code>class.id_type</code>the type of its <code>@Id</code> field, boxed — <code>Long</code></li>
  <li><code>class.fields</code>each with <code>name</code>, <code>json_name</code>, <code>type_name</code>, <code>type_simple</code>, <code>final</code>, <code>id</code>, <code>setter</code>, <code>getter</code> (Lombok's counted), <code>annotations</code> and <code>constraints</code></li>
  <li><code>field.setter_chains</code>the setter returns the object, so calls chain — <code>order.setName(n).setQuantity(1)</code>; Lombok's <code>@Accessors(chain = true)</code> counted</li>
  <li><code>field.wither</code>the method that returns a copy with the field changed — <code>withName</code>, declared or from Lombok's <code>@With</code></li>
  <li><code>file</code>, <code>directory</code>the class's file and folder</li>
  <li><code>date</code>, <code>year</code>today and its year</li>
</ul>
<p>Members for the class — the default output:</p>
<pre><code>{@html highlightCode(`{# bennu.description: A copy constructor -#}
public {{ class.name }}({{ class.name }} other) {
{% for field in class.fields if not field.final %}
    this.{{ field.name }} = other.{{ field.name }};
{% endfor %}
}`, 'jinja-java')}</code></pre>
<p>A file beside the class:</p>
<pre><code>{@html highlightCode(`{# bennu.description: A Spring Data repository for the entity -#}
{# bennu.output: file -#}
{# bennu.file: {{ class.name }}Repository.java -#}
package {{ class.package }};

import org.springframework.data.jpa.repository.JpaRepository;

public interface {{ class.name }}Repository extends JpaRepository<{{ class.name }}, {{ class.id_type }}> {
}`, 'jinja-java')}</code></pre>

<h2>Configuration properties</h2>
<p>Rendered with a <code>@ConfigurationProperties</code> class, from the keys the Spring model says it binds.</p>
<ul class="prop-list">
  <li><code>prefix</code>the class's prefix, <code>app.http</code></li>
  <li><code>class_name</code>, <code>class_fqcn</code>the class</li>
  <li><code>properties</code>each key: <code>key</code>, <code>relative_key</code>, <code>segments</code>, <code>field</code>, <code>owner</code>, <code>type_name</code>, <code>type_simple</code> and <code>sample</code> — a plausible value for its type</li>
  <li><code>lines</code>the same keys laid out as YAML: <code>indent</code>, <code>key</code>, <code>leaf</code>, <code>item</code> (the first key of a list element) and <code>sample</code></li>
</ul>
<pre><code>{@html highlightCode(`{# bennu.description: The keys the class binds, as application.yml -#}
{% for line in lines %}
{{ line.indent }}{{ line.key }}:{% if line.leaf %} {{ line.sample }}{% endif %}

{% endfor %}`, 'jinja-yaml')}</code></pre>

<h2>Configuration class</h2>
<p>Rendered with the keys chosen under a prefix of a configuration file, every profile file beside it read too.</p>
<ul class="prop-list">
  <li><code>prefix</code>, <code>class_name</code>, <code>package</code>the prefix bound, the class and its package</li>
  <li><code>imports</code>what the fields' types need, sorted</li>
  <li><code>root</code>the class: <code>name</code>, <code>path</code>, <code>depth</code>, <code>indent</code> (four spaces a level), <code>fields</code> and <code>types</code> — the types nested in it, each the same shape</li>
  <li><code>field</code><code>key</code>, <code>full_key</code>, <code>name</code>, <code>pascal</code>, <code>type_name</code>, <code>list</code>, <code>map</code>, <code>nested</code> and <code>sample</code></li>
  <li><code>sources</code>, <code>notes</code>the files the keys came from, and what is worth knowing</li>
</ul>
<table>
  <thead>
    <tr><th>Value</th><th>Type</th></tr>
  </thead>
  <tbody>
    <tr><td><code>true</code>, <code>false</code></td><td><code>Boolean</code></td></tr>
    <tr><td><code>587</code></td><td><code>Integer</code>, or <code>Long</code> past its range</td></tr>
    <tr><td><code>2.5</code></td><td><code>Double</code> — also for a key that is <code>1</code> in one list element and <code>2.5</code> in another</td></tr>
    <tr><td><code>30s</code>, <code>PT30S</code></td><td><code>Duration</code></td></tr>
    <tr><td><code>10MB</code></td><td><code>DataSize</code></td></tr>
    <tr><td><code>${'{'}PORT:8080{'}'}</code></td><td>the type of its default; text without one</td></tr>
    <tr><td><code>"587"</code>, <code>0587</code>, anything else</td><td><code>String</code></td></tr>
    <tr><td>a list</td><td><code>List</code> of what its elements are</td></tr>
    <tr><td>a group of keys</td><td>a nested type named after the key</td></tr>
    <tr><td>groups alike under different names</td><td><code>Map&lt;String, …&gt;</code> of one nested type</td></tr>
  </tbody>
</table>
<p>Types are boxed, so a key a profile does not set binds to <code>null</code>. The record template Bennu ships:</p>
<pre><code>{@html highlightCode(`{# bennu.description: A record, with nested records -#}
{# bennu.output: file -#}
{# bennu.file: {{ class_name }}.java -#}
{# bennu.requires: java >= 16 -#}
{% if package %}
package {{ package }};

{% endif %}
{% for import in imports %}
import {{ import }};
{% endfor %}
import org.springframework.boot.context.properties.ConfigurationProperties;

{% for type in [root] recursive %}
{% if loop.depth0 == 0 %}
@ConfigurationProperties(prefix = "{{ prefix }}")
{% endif %}
{{ type.indent }}public record {{ type.name }}(
{% for field in type.fields %}
{{ type.indent }}        {{ field.type_name }} {{ field.name }}{% if not loop.last %},{% endif %}

{% endfor %}
{{ type.indent }}) {
{% if type.types %}
{{ loop(type.types) -}}
{% endif %}
{{ type.indent }}}
{% endfor %}`, 'jinja-java')}</code></pre>

<h2>Validation tests</h2>
<p>
  Rendered with the DTO Lab's cases: the class, a valid value for every field, and one case for every way each
  constraint can fail, with the violations the project's validator reported. Every variable, with the built-in
  template, is on the <strong>DTO Lab</strong> page.
</p>

<h2>Abbreviations</h2>
<p>
  Rendered in the file the abbreviation is typed in, whatever language that is; the template's name is the word
  that triggers it — or its <code>bennu.abbrev</code> — and the language in its name says where it is offered.
  <code>class_name</code> is the file's own name outside Java, where a file does not declare a type.
</p>
<ul class="prop-list">
  <li><code>class_name</code>, <code>package</code>, <code>file_name</code>the file it is typed in</li>
  <li><code>date</code>, <code>year</code>today and its year</li>
</ul>
<p>
  The result is a snippet: <code>$1</code>, <code>$2</code> are the <kbd>Tab</kbd> stops, <code>{'${'}1:name{'}'}</code>
  is one with a default, and <code>$0</code> is where the caret ends.
</p>
<pre><code>{@html highlightCode(`{# bennu.description: A logger for this class -#}
{# bennu.requires: slf4j-api -#}
private static final org.slf4j.Logger LOG = org.slf4j.LoggerFactory.getLogger({{ class_name }}.class);$0`, 'jinja-java')}</code></pre>

<h2>Preview and parameters</h2>
<p>
  The eye button in a template's toolbar renders it beside the source as you type. It renders with the first
  class of the last Java file opened, or any class picked with <strong>Choose…</strong>; a New file template with
  a name typed there; a Configuration class template with a configuration file and a prefix.
</p>
<p>
  <strong>Parameters</strong> lay a JSON object over everything the template reads, to see a branch the data at
  hand does not take. Objects merge key by key; a list or a value replaces what was there. Parameters are for
  the preview only.
</p>
<table>
  <thead>
    <tr><th>Parameters</th><th>Shows the template as if</th></tr>
  </thead>
  <tbody>
    <tr><td><code>{'{'} "class": {'{'} "name": "Invoice" {'}'} {'}'}</code></td><td>the class were called <code>Invoice</code>, with the same fields</td></tr>
    <tr><td><code>{'{'} "style": {'{'} "lombok_val": true {'}'} {'}'}</code></td><td>you wrote locals with <code>val</code></td></tr>
    <tr><td><code>{'{'} "style": {'{'} "local_var": true {'}'} {'}'}</code></td><td>you wrote locals with <code>var</code> — what <code>style.local(…)</code> writes included</td></tr>
    <tr><td><code>{'{'} "project": {'{'} "java": 11 {'}'} {'}'}</code></td><td>the project were on Java 11</td></tr>
  </tbody>
</table>
