<script lang="ts">
  /**
   * MyBatis mappers: the XML and the interface navigated as one thing.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>MyBatis mappers</h1>

<p class="doc-lead">
  A mapper is an interface with no implementation and an XML file with no compiler checking it. Bennu links the two, so each half is one jump from the other.
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

<h2>Following a piece</h2>
<p>
  In a mapper <code>.xml</code>, <kbd>Ctrl</kbd> + <kbd>B</kbd> — or <kbd>Ctrl</kbd> + click — follows the piece under the caret:
</p>
<table>
  <thead><tr><th>The caret on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>A statement's <code>id="findById"</code></td><td>The matching method on the mapper interface</td></tr>
    <tr><td>The mapper's <code>namespace="…"</code></td><td>That interface</td></tr>
    <tr><td><code>&lt;include refid="columns"&gt;</code></td><td>The <code>&lt;sql&gt;</code> fragment it pulls in</td></tr>
    <tr><td>A statement's <code>resultMap="orderMap"</code></td><td>The <code>&lt;resultMap&gt;</code> it uses</td></tr>
  </tbody>
</table>
<Callout variant="tip" title="Instant inside one file">
  A fragment or result map referenced from the same file resolves at once, with no index needed.
</Callout>
