<script lang="ts">
  /**
   * Code templates, as a guide: what a template is, the six kinds, a first one made step by step, where
   * they live and how each is used. Read first; the language is `TemplateLanguage`, and every variable
   * and directive is in `TemplateReference`.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Editor</span>
<h1>Code templates</h1>

<p class="doc-lead">
  Much of the code Bennu writes for you — a new file, a builder, a repository beside an entity, a
  configuration class, a test for every constraint, the snippet behind an abbreviation — comes out of a
  <strong>template</strong>. Bennu ships one for each job, and every one of them can be copied and
  changed, so what it writes looks like what you would have written yourself.
</p>

<p>
  A template is a text file with gaps in it. The text is the code as it should come out; the gaps say
  where the details go — the class name, its fields, the value of a key. When Bennu generates, it fills
  the gaps with what it knows about the class, the file or the project you are working in. This is a
  small part of the builder Bennu ships:
</p>

<pre><code>{@html highlightCode(`public static final class Builder {
{% for field in class.fields %}
    private {{ field.type_name }} {{ field.name }};
{% endfor %}
}`, 'jinja-java')}</code></pre>

<p>
  Run on a class with a <code>name</code> and an <code>email</code>, it writes:
</p>

<pre><code>{@html highlightCode(`public static final class Builder {
    private String name;
    private String email;
}`, 'java')}</code></pre>

<p>
  What sits between <code>{'{{'} {'}}'}</code> is printed; what sits between <code>{'{%'} {'%}'}</code>
  decides what is printed, and how many times. That is most of it. <strong>The template language</strong>
  goes through the rest one example at a time, and <strong>Template reference</strong> lists everything a
  template can read.
</p>

<h2>Six kinds of template</h2>
<p>
  Every template has a <em>kind</em>. The kind decides where you use the template, what it can read, and
  where what it writes ends up.
</p>

<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">New file dialog</div>
    <div class="fc-title">New file</div>
    <div class="fc-desc">A whole file, named after what you type. Your templates are listed under the built-in shapes, as <em>Your templates</em>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Right-click on a Java class</div>
    <div class="fc-title">From a class</div>
    <div class="fc-desc">Code written from the class at the caret: members inserted into it — a builder — or a file beside it — a Spring Data repository for an entity.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Right-click on a <code>@ConfigurationProperties</code> class</div>
    <div class="fc-title">Configuration properties</div>
    <div class="fc-desc">The keys the class binds, written out as YAML or <code>.properties</code> with a sample value for each, to copy or to append to a file.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">The wand on <code>application.yml</code></div>
    <div class="fc-title">Configuration class</div>
    <div class="fc-desc">The other way round: a <code>@ConfigurationProperties</code> class written from the keys you select — a record, a Lombok class or a plain one.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">DTO Lab › Tests</div>
    <div class="fc-title">Validation tests</div>
    <div class="fc-desc">One test for every way a constraint can fail, expecting what the project's own validator reported.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Java completion</div>
    <div class="fc-title">Abbreviations</div>
    <div class="fc-desc">A word you type that expands into a snippet, next to <code>psf</code> and <code>sout</code>, with <kbd>Tab</kbd> stops to fill in. In any language: the name says which.</div>
  </div>
</div>

<h2>Your first template, step by step</h2>
<p>
  The quickest way to a template of your own is to start from one that already works and change it while
  looking at what it writes. Here is the builder again:
</p>

<ol class="step-list">
  <li>Open <strong>Settings › Code Templates</strong> — or run <em>Code templates…</em> from the command palette. The
    six kinds are listed, and each has a page of its own: the templates of that kind on the left, and the one you pick
    on the right — what it writes, what the project needs for it, and where its file is.</li>
  <li>Under <em>From a class</em>, press the copy button on <strong>builder</strong> and name the copy — <code>team-builder</code>. It opens in the editor.</li>
  <li>Open any Java class in another tab, go back to the template and press the <strong>eye button</strong> in its toolbar. The preview beside it renders the template against that class, and follows every key you type.</li>
  <li>Change something — the builder's methods, a comment on top, the order of the fields — and watch the preview change with it. A mistake shows as a message above the last output that worked.</li>
  <li>Put the caret on a line of the template: the lines it writes light up in the preview, scrolled into view. Select something in the preview and the template lines that wrote it light up in the editor.</li>
  <li>Save. In the class, right-click › <strong>Generate from a template…</strong> now offers <code>team-builder</code>, and <strong>Use for this project</strong> makes it the one this project generates with.</li>
</ol>

<Callout variant="tip" title="Reading a built-in">
  Its body is on that page beside the list, so most of the time there is nothing to open. When you want the whole
  thing, a built-in opens in the editor like one of yours — read-only, with its preview — and <strong>Copy</strong>
  makes it yours.
</Callout>
<p>
  A template of yours can be <strong>renamed</strong> from the same list — the file moves, a tab open on it
  follows, and a project generating with it keeps generating with it. For an abbreviation the name is the word
  you type, so renaming one is how you change what expands it.
</p>

<h2>Where templates live</h2>
<dl class="meta-grid">
  <dt>Yours</dt>
  <dd>
    In your profile, one folder per kind: <code>bennu/templates/&lt;kind&gt;/&lt;name&gt;.&lt;extension&gt;.jinja</code>
    — for instance <code>bennu/templates/class/team-builder.java.jinja</code>. They belong to you rather than to a
    project, so every project sees them.
  </dd>
  <dt>Built-in</dt>
  <dd>
    Inside Bennu, and always there, so every kind has a template that works. They cannot be changed or deleted —
    copy them instead. One badged <strong>Starter</strong> is there only to be copied: the New file dialog already
    makes a class, and an abbreviation nobody chose is a word that would expand in every Java file. Bennu never
    generates with a starter, and a starter abbreviation is not offered in the popup until you have made it yours.
  </dd>
  <dt>A project's choice</dt>
  <dd>
    Which template a project generates each kind with, kept in the project's <code>.arbor/bennu/config.toml</code>.
    A single generation can still pick another. Until the project chooses, it uses the first template it can —
    see <strong>Template reference › When a template is offered</strong>.
  </dd>
</dl>
<p>
  The extension before <code>.jinja</code> is the language the template writes. It is how the editor colours
  the template — <code>team-builder.java.jinja</code> is Java with template tags on top — and how the preview
  colours what it writes. <em>New template…</em> asks for it where the kind lets you choose, which is why the
  starting point is a copy: what you copy already writes something.
</p>
<Callout variant="info" title="Which kinds a project has">
  Four of the six read a Java class, the Spring model or Bean Validation — <em>From a class</em>, the two
  configuration ones and the validation tests — so on a Cargo workspace they are not listed: there is nothing for
  them to run on. <strong>New file</strong> and <strong>abbreviations</strong> are about the file rather than the
  language, and are there on every project.
</Callout>

<h2>Generating</h2>
<table>
  <thead>
    <tr><th>Kind</th><th>Where</th><th>What happens</th></tr>
  </thead>
  <tbody>
    <tr>
      <td>New file</td>
      <td>The New file dialog, under <em>Your templates</em></td>
      <td>The file is created in the chosen folder and opened.</td>
    </tr>
    <tr>
      <td>From a class</td>
      <td>Right-click on a Java class › <em>Generate from a template…</em>; the command palette; <em>From a template…</em> in the Generate dialog (<kbd>Alt</kbd>+<kbd>Insert</kbd>)</td>
      <td>Members go into the class as one undo step; a file is created beside it; or text is copied.</td>
    </tr>
    <tr>
      <td>Configuration properties</td>
      <td>Right-click on a <code>@ConfigurationProperties</code> class › <em>Generate configuration properties…</em>; the command palette</td>
      <td>YAML or <code>.properties</code> is copied, or appended to a file you pick.</td>
    </tr>
    <tr>
      <td>Configuration class</td>
      <td>The wand in the toolbar of <code>application.yml</code> and its profiles; the right-click menu; the command palette</td>
      <td>The class is created in the chosen folder and opened.</td>
    </tr>
    <tr>
      <td>Validation tests</td>
      <td>DTO Lab › Tests</td>
      <td>A test class is written, or tests are added to an existing one.</td>
    </tr>
    <tr>
      <td>Abbreviations</td>
      <td>Typing the abbreviation in a file of the language it writes</td>
      <td>The snippet replaces the word, and <kbd>Tab</kbd> walks its stops.</td>
    </tr>
  </tbody>
</table>
<p>
  Nothing is written before you have seen it. Members are inserted only while the class is still the text
  they were computed against — a class edited in the meantime is generated again. Text appended to a file
  goes into its buffer, so the file's own unsaved changes stay where they are.
</p>

<h2>A configuration class from YAML</h2>
<p>
  In <code>application.yml</code> — or <code>.properties</code>, or a profile file — <strong>select the lines</strong>
  whose keys the class should bind and press the wand in the editor toolbar. The class binds the group of keys
  your selection shares, and gets exactly those keys. With nothing selected, it starts from the group the caret
  is in, and offers the groups around it instead.
</p>
<pre><code>{@html highlightCode(`app:
  mail:
    host: smtp.example.com
    port: 587
    read-timeout: 30s
    servers:
      - url: https://one.example.com`, 'yaml')}</code></pre>
<p>Selecting the <code>mail</code> group and generating with the record template writes:</p>
<pre><code>{@html highlightCode(`@ConfigurationProperties(prefix = "app.mail")
public record MailProperties(
        String host,
        Integer port,
        Duration readTimeout,
        List<Server> servers
) {
    public record Server(
            String url
    ) {
    }
}`, 'java')}</code></pre>
<p>
  Each key becomes a field typed by its value — <code>587</code> is an <code>Integer</code>, <code>30s</code> a
  <code>Duration</code> — and each group of keys a nested type. In the dialog every key has a box, so a key can
  be left out; a group's box stands for everything below it. Groups that are the same thing under different
  names — <code>postgres1</code> and <code>postgres2</code> with the same keys — become one type in a
  <code>Map&lt;String, …&gt;</code>, and the <strong>Map</strong> switch on a group decides otherwise. The profile
  files beside the one open (<code>application-dev.yml</code>) are read too, so a key only one profile sets still
  gets a field.
</p>

<h2>Abbreviations</h2>
<Callout variant="info" title="The name is the word you type — until you say otherwise">
  An abbreviation template called <code>logd.java.jinja</code> is offered when you type <code>logd</code> in a
  Java file, and <code>dbg.rs.jinja</code> when you type <code>dbg</code> in a Rust one: the language in the name
  says where it belongs, and one written without a language — <code>licence.jinja</code> — is offered in every
  file, which is what a header or a banner wants. That is why <em>New template…</em> asks abbreviations for an <em>Abbreviation</em> rather than a name.
  One called like a built-in abbreviation — <code>psf</code> — replaces it.
  <br />
  The two can be told apart when the file name makes a poor trigger: <strong>Settings › Code Templates ›
  Abbreviations</strong> shows the word beside each template and lets you change it, which writes a
  <code>bennu.abbrev</code> line into the template. So the file can be <code>logger-for-this-class</code> and the
  word stay <code>logd</code>.
  <br />
  What you type is matched <strong>whatever case it is in</strong>, as it is for <code>psf</code> and
  <code>sout</code>: an abbreviation named <code>Hget</code> is offered on <code>hget</code>.
</Callout>
<p>
  Its text is a snippet: <code>$1</code>, <code>$2</code> are the places <kbd>Tab</kbd> walks through,
  <code>{'${'}1:name{'}'}</code> is one with a default, and <code>$0</code> is where the caret ends. The template
  runs first, so it can use the file the abbreviation is typed in:
</p>
<pre><code>{@html highlightCode(`{# bennu.description: A logger for this class -#}
private static final Logger LOG = LoggerFactory.getLogger({{ class_name }}.class);$0`, 'jinja-java')}</code></pre>
<p>
  Write the body at the left margin: what it writes is <strong>re-indented to the line it lands on</strong>, so a
  method written flat in the template arrives lined up with the class it is typed into. A template with no stops at
  all is indented the same way.
</p>
<Callout variant="tip" title="Two stops with one number are one thing typed twice">
  <code>{'${'}1:name{'}'}</code> written in two places is filled in both as you type, and <kbd>Tab</kbd> visits the
  pair once — a field and the parameter it is assigned from stay the same word without you writing it twice. The
  copies are drawn fainter than the one you are typing in.
</Callout>

<h2>Tab stops in generated members</h2>
<p>
  A <em>From a class</em> template can walk you through what it wrote too — the name of the method it generated, the
  message of the exception it throws — by declaring it:
</p>
<pre><code>{@html highlightCode(`{# bennu.description: A guard clause -#}
{# bennu.stops: true -#}
private void require{{ class.name }}(\${1:Order} \${2:order}) {
    if (\${2:order} == null) throw new IllegalArgumentException("$0");
}`, 'jinja-java')}</code></pre>
<p>
  The members land in the class as usual, with the caret on the first stop and <kbd>Tab</kbd> walking the rest.
</p>
<Callout variant="warning" title="Off unless asked for">
  Generated code and snippets both use <code>$</code>: reading every template as a snippet would quietly eat the
  <code>&#36;&#123;spring.datasource.url&#125;</code> out of one that writes a property file. So a template that wants
  stops says <code>bennu.stops: true</code>, and every other template writes its dollars as they are.
</Callout>

<h2>Following the project and your style</h2>
<p>
  Every template can read the <strong>project</strong> it is generating for — its Java level and its
  dependencies — and your <strong>style</strong> from <em>Settings › Java › Code Style</em>. And <code>style</code> writes
  a declaration the way you do — <code>val</code> where you use Lombok's, <code>var</code> where you use that, the type
  everywhere else — so one template fits every project:
</p>
<pre><code>{@html highlightCode(`{{ style.local(field.type_simple) }} {{ field.name }} = source.{{ field.getter }}();`, 'jinja-java')}</code></pre>
<p>
  And a template that only makes sense in some projects can say so in one line —
  <code>{'{#'} bennu.requires: java &gt;= 16 {'#}'}</code> — to be offered only where it applies. Both are in
  <strong>Template reference</strong>.
</p>

<h2>For an AI client</h2>
<p>
  An AI client connected to Arbor can list your templates with <code>bennu_list_templates</code> and render one
  with <code>bennu_render_template</code> — on a class, on the keys of a configuration file, or for a new file's
  name — so the code it writes is shaped like yours. Both only read: what a template renders comes back with
  where it would go, and nothing is written.
</p>
