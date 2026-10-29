<script lang="ts">
  /**
   * MyBatis mappers: the XML and the interface navigated as one thing.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">MyBatis</span>
<h1>MyBatis mappers</h1>

<p class="doc-lead">
  A mapper is an interface with no implementation and an XML file with no compiler checking it. Bennu links the two, so each half is one jump from the other.
</p>

<h2>What Bennu treats as a mapper</h2>
<p>
  An <code>.xml</code> file whose root is <code>&lt;mapper namespace="…"&gt;</code>, anywhere in the project. The <code>namespace</code> is the fully-qualified
  name of the mapper interface; every <code>&lt;select&gt;</code>, <code>&lt;insert&gt;</code>, <code>&lt;update&gt;</code> and <code>&lt;delete&gt;</code> whose
  <code>id</code> equals a method name is that method's SQL. A <code>mybatis-config.xml</code> also contains <code>&lt;mapper&gt;</code> elements, but its root is
  not one, so it is left alone.
</p>
<p>
  MyBatis support turns on when the project has mapper interfaces or mapper XML. The mappers are collected with the rest of the project index, so
  cross-file jumps and checks wait for it, while everything that stays inside one file works at once.
</p>

<h2>A mapper, both halves</h2>
<pre><code>{@html highlightCode(`<mapper namespace="com.acme.orders.OrderMapper">
    <sql id="columns">id, customer, total</sql>

    <resultMap id="orderMap" type="com.acme.orders.Order">
        <id property="id" column="id"/>
    </resultMap>

    <select id="findById" resultMap="orderMap">
        SELECT <include refid="columns"/> FROM orders WHERE id = #{id}
    </select>
</mapper>`, 'markup')}</code></pre>
<pre><code>{@html highlightCode(`public interface OrderMapper {
    Order findById(long id);
}`, 'java')}</code></pre>

<h2>Colouring</h2>
<p>
  A mapper <code>.xml</code> is coloured for what it is, not only as markup: the SQL of each statement (keywords, functions, strings,
  numbers, comments), <code>#&#123;name&#125;</code> bindings, and <code>$&#123;name&#125;</code> in the warning colour — it is spliced into the
  statement rather than bound, so it is the one that can be an injection. The dynamic tags (<code>if</code>, <code>where</code>,
  <code>foreach</code>, …) and their <code>test</code> conditions stand apart from the SQL they control; statement ids, references
  (<code>refid</code>, <code>resultMap</code>) and the Java types a mapper names each have their own colour. The details are on
  <strong>Statements, parameters and result maps</strong> and <strong>Dynamic SQL</strong>.
</p>

<h2>Following a piece</h2>
<p>
  In a mapper <code>.xml</code>, <kbd>Ctrl</kbd> + <kbd>B</kbd> — or <kbd>Ctrl</kbd> + click — follows the piece under the caret. The caret only has to be
  inside the attribute value:
</p>
<table>
  <thead><tr><th>The caret on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>A statement's <code>id="findById"</code></td><td>The matching method on the mapper interface</td></tr>
    <tr><td>The mapper's <code>namespace="…"</code> (also on <code>&lt;cache-ref&gt;</code>)</td><td>That interface</td></tr>
    <tr><td><code>&lt;include refid="columns"&gt;</code></td><td>The <code>&lt;sql&gt;</code> fragment it pulls in</td></tr>
    <tr><td><code>resultMap="orderMap"</code> on a statement, <code>&lt;association&gt;</code>, <code>&lt;collection&gt;</code> or <code>&lt;case&gt;</code>; <code>extends="…"</code> on a <code>&lt;resultMap&gt;</code></td><td>The <code>&lt;resultMap&gt;</code> it uses</td></tr>
    <tr><td>An <code>&lt;association select="…"&gt;</code> or <code>&lt;collection select="…"&gt;</code></td><td>The statement it runs</td></tr>
    <tr><td><code>type</code>, <code>resultType</code>, <code>parameterType</code>, <code>javaType</code>, <code>ofType</code>, <code>typeHandler</code></td><td>The Java class — by qualified name, or by simple name when the project has exactly one class so named</td></tr>
    <tr><td>A result <code>property="name"</code></td><td>The field on the mapped class, else its setter or getter; the class itself when neither is written out (a Lombok property)</td></tr>
  </tbody>
</table>
<p>
  A reference written <code>com.acme.other.OtherMapper.columns</code>, or a bare one that this file does not declare, is looked up in the other mapper files of
  that namespace. A built-in alias such as <code>long</code> or <code>map</code> names no project class and goes nowhere. A value written as an expression
  (<code>$&#123;…&#125;</code>) names nothing yet and is not followed.
</p>
<p>
  The other way, on the mapper <code>.java</code> interface: <kbd>Ctrl</kbd> + <kbd>B</kbd> on a method's name opens its
  <code>&lt;select|insert|update|delete&gt;</code> statement in the XML — on the declaration, and on a call made through a mapper too. If the method has no
  statement, the ordinary go-to applies. <strong>Go to implementation</strong>
  (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd>) lists that statement together with any Java override.
</p>
<Callout variant="tip" title="Instant inside one file">
  A fragment or result map referenced from the same file resolves at once, with no index needed.
</Callout>

<h2>Find usages</h2>
<p><kbd>Alt</kbd> + <kbd>F7</kbd> in a mapper <code>.xml</code>:</p>
<table>
  <thead><tr><th>The caret on</th><th>Lists</th></tr></thead>
  <tbody>
    <tr><td>The <code>id</code> of a <code>&lt;sql&gt;</code> or <code>&lt;resultMap&gt;</code>, or a reference to one</td><td>Every reference to it across the project's mappers, the unsaved text of the open file included</td></tr>
    <tr><td>A statement <code>id</code></td><td>The Java usages of the interface method — what calls the statement</td></tr>
  </tbody>
</table>

<h2>Where to go next</h2>
<ul>
  <li><strong>Statements, parameters and result maps</strong> — what each statement and mapping element is, and how a statement is checked against a database.</li>
  <li><strong>Dynamic SQL</strong> — <code>if</code>, <code>where</code>, <code>foreach</code> and the rest, and how they are read.</li>
  <li><strong>Mapper checks</strong> — every problem Bennu reports in a mapper or its interface.</li>
</ul>
