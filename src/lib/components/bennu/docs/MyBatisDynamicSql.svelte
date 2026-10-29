<script lang="ts">
  /**
   * MyBatis dynamic SQL: how the tags are drawn and how they are read for the database check.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">MyBatis</span>
<h1>Dynamic SQL</h1>

<p class="doc-lead">
  The tags of a statement decide which pieces of the SQL exist at run time. Bennu draws them apart from the SQL they control, and — for the database check —
  reads them as one possible expansion.
</p>

<h2>A dynamic statement</h2>
<pre><code>{@html highlightCode(`<select id="search" resultMap="orderMap">
    SELECT <include refid="columns"/> FROM orders
    <where>
        <if test="customer != null">
            AND customer = #{customer}
        </if>
        <choose>
            <when test="onlyOpen">AND status = 'OPEN'</when>
            <otherwise>AND status <> 'DELETED'</otherwise>
        </choose>
        <if test="ids != null and ids.size() > 0">
            AND id IN
            <foreach collection="ids" item="id" open="(" separator="," close=")">
                #{id}
            </foreach>
        </if>
    </where>
</select>`, 'markup')}</code></pre>

<h2>Colouring</h2>
<table>
  <thead><tr><th>Piece</th><th>Drawn as</th></tr></thead>
  <tbody>
    <tr><td>The name of <code>if</code>, <code>choose</code>, <code>when</code>, <code>otherwise</code>, <code>where</code>, <code>set</code>, <code>trim</code>, <code>foreach</code>, <code>bind</code>, <code>include</code></td><td>A dynamic-tag colour, on the opening <em>and</em> the closing tag</td></tr>
    <tr><td><code>test="…"</code> on any of them</td><td>An expression (italic) — the OGNL condition</td></tr>
    <tr><td><code>&lt;bind value="…"&gt;</code> and <code>&lt;foreach collection="…"&gt;</code></td><td>An expression, like a <code>test</code></td></tr>
    <tr><td>The SQL <em>inside</em> the tags, however deeply nested, and inside <code>CDATA</code></td><td>SQL: keywords, functions, strings, numbers, comments</td></tr>
    <tr><td><code>#&#123;…&#125;</code> / <code>$&#123;…&#125;</code> inside them</td><td>A bound parameter / a splice, as anywhere else</td></tr>
  </tbody>
</table>
<p>
  The tags and their conditions therefore read as a layer over the statement, and the statement still reads as SQL. A <code>test</code> is only drawn:
  its expression is not evaluated, completed or checked.
</p>
<Callout variant="info" title="Keywords are a closed set">
  A word right after <code>FROM</code>, <code>JOIN</code>, <code>INTO</code>, <code>UPDATE</code> or <code>TABLE</code>, or after a <code>.</code>, is a name and
  never a keyword — <code>user</code>, <code>value</code> and <code>index</code> are columns somewhere, and a perfectly good query must not read as broken.
</Callout>

<h2>How the database check reads them</h2>
<p>
  To check a dynamic statement against the database (see <strong>Statements, parameters and result maps</strong>), Bennu flattens the tags into a single
  statement. The result is one expansion among many, which is why a rejected dynamic statement says <em>Checked with every dynamic branch included</em>.
</p>
<table>
  <thead><tr><th>Tag</th><th>Rendered as</th></tr></thead>
  <tbody>
    <tr><td><code>&lt;if&gt;</code></td><td>Its body, always kept</td></tr>
    <tr><td><code>&lt;choose&gt;</code></td><td>The body of the <strong>first</strong> <code>&lt;when&gt;</code>; the <code>&lt;otherwise&gt;</code> only if there is no <code>&lt;when&gt;</code> — the other branches would repeat the same clause</td></tr>
    <tr><td><code>&lt;where&gt;</code></td><td><code>WHERE</code> plus the body, with a leading <code>AND</code> or <code>OR</code> dropped (whole word only: <code>android_id</code> keeps its letters). Nothing at all if the body is empty</td></tr>
    <tr><td><code>&lt;set&gt;</code></td><td><code>SET</code> plus the body, with the trailing comma dropped. Nothing if empty</td></tr>
    <tr><td><code>&lt;trim&gt;</code></td><td>The body with its <code>prefixOverrides</code> and <code>suffixOverrides</code> (separated by <code>|</code>) removed, wrapped in its <code>prefix</code> and <code>suffix</code>. Nothing if empty</td></tr>
    <tr><td><code>&lt;foreach&gt;</code></td><td>Its body once, between <code>open</code> and <code>close</code></td></tr>
    <tr><td><code>&lt;bind&gt;</code>, <code>&lt;selectKey&gt;</code></td><td>Nothing</td></tr>
    <tr><td><code>&lt;include&gt;</code></td><td>The fragment of this mapper, with the include's <code>&lt;property&gt;</code> values substituted for <code>$&#123;name&#125;</code> in it; a fragment of another mapper, or a missing one, leaves the statement unchecked</td></tr>
  </tbody>
</table>
<p>
  An include that includes itself is cut off after a few levels rather than looping. A <code>$&#123;name&#125;</code> that an include property filled in is
  not a splice any more; one that stays is, and needs a value to be checked.
</p>

<h2>Not covered</h2>
<ul>
  <li>No <code>test</code> condition, <code>&lt;bind&gt;</code> expression or <code>foreach</code> collection is validated — a misspelled parameter name in OGNL is not reported.</li>
  <li><code>&lt;foreach&gt;</code> attributes (<code>item</code>, <code>index</code>) are not cross-checked against the <code>#&#123;&#125;</code> inside.</li>
  <li>Only the first <code>&lt;when&gt;</code> reaches the database, so a mistake confined to a later branch is not seen by the server check.</li>
</ul>
