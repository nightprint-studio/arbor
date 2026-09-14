<script lang="ts">
  /**
   * MapStruct mappers: what is checked in a mapper, the Alt+Enter fixes, completion and go-to inside mapping paths, the Mappers
   * panel, and what is deliberately left alone.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Java &amp; JSP</span>
<h1>MapStruct mappers</h1>

<p class="doc-lead">
  A mapper is an interface full of methods nobody writes. Bennu reads it the way the annotation processor will — and says what is wrong in the file where the mapping is
  written, instead of in a compiler message about generated code on the next build.
</p>

<h2>What is checked</h2>
<pre><code>{@html highlightCode(`@Mapper(componentModel = "spring")
public interface UserMapper {
    @Mapping(target = "nmae", source = "firstName")   // no property "nmae" on UserDto
    @Mapping(target = "email", constant = "n/a", source = "mail")
    UserDto toDto(User user);                         // "nickname" is never mapped
}`, 'java')}</code></pre>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Build fails</div>
    <div class="fc-title">A property that does not exist</div>
    <div class="fc-desc">A <code>target</code> or <code>source</code> path segment that names nothing on its type. Nested paths like <code>address.street</code> are followed
      property by property; with several source parameters, a path that starts with a parameter's name.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Build fails</div>
    <div class="fc-title">Mapped twice, or refused together</div>
    <div class="fc-desc">The same <code>target</code> on two <code>@Mapping</code>s of one method, and elements MapStruct will not combine — <code>source</code> with
      <code>constant</code> or <code>expression</code>, <code>constant</code> with <code>expression</code>, a default value with a default expression.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Build passes</div>
    <div class="fc-title">A target property nothing maps</div>
    <div class="fc-desc">Shown on the method name: every writable property that is neither targeted nor filled from a same-named source property or parameter. A
      warning, or an error when the mapper's <code>unmappedTargetPolicy</code> is <code>ERROR</code>.</div>
  </div>
</div>
<Callout variant="warning" title="The unmapped one is the one that reaches production">
  MapStruct only warns about it, in a build log. The generated method compiles, leaves the field unset, and the first person to notice sees a <code>null</code> three
  services away.
</Callout>

<h2>Alt+Enter</h2>
<table>
  <thead><tr><th>On</th><th>Offers</th></tr></thead>
  <tbody>
    <tr><td>An unmapped target</td><td><strong>Ignore N unmapped target properties</strong> — one <code>@Mapping(target = "…", ignore = true)</code> per property above the method, importing <code>Mapping</code> when the file does not.</td></tr>
    <tr><td>An unknown property</td><td><strong>Change to …</strong> — when exactly one property is a likely typo of what was written.</td></tr>
  </tbody>
</table>

<h2>Inside the strings</h2>
<p>
  A mapping path is code written as a string, and it behaves like code: completion offers the properties of whatever the path so far leads to (writable ones for a
  <code>target</code>, readable ones and parameter names for a <code>source</code>), replacing only the segment under the caret. <strong>Ctrl+B</strong> on a segment
  opens the property's field — or its getter or setter when there is no field — and hover shows its type and where it comes from: a field, an accessor, a constructor, a
  record component, or Lombok.
</p>
<p>A mapping method whose generated implementation exists under <code>generated-sources</code> gets a gutter mark that jumps into it.</p>

<h2>The Mappers panel</h2>
<p>
  <strong>Command Palette → MapStruct mappers</strong> lists every mapper with its component model, and under it each mapping method with its source and target types,
  whether it maps, updates a <code>@MappingTarget</code> or iterates a collection — and the properties it leaves unmapped, tagged, so the question "which mappers drop a
  field" has an answer without opening them one by one.
</p>

<h2>What it will not judge</h2>
<ul>
  <li><strong>Types it cannot see completely.</strong> A class from a dependency, a DTO whose superclass comes from a jar, one with accessors renamed by Lombok
    <code>@Accessors</code>, fluent setters, a hand-written builder, or several constructors: nothing is said about their properties either way.</li>
  <li><strong>A policy it cannot read.</strong> A <code>config</code> class it cannot find, an inheritance strategy, or a build without a <code>pom.xml</code> — processor
    options set in Gradle are not visible, so unmapped targets are reported there only on mappers that set their policy themselves.</li>
  <li><strong>Mappings that come from elsewhere</strong>: <code>@InheritConfiguration</code>, <code>@InheritInverseConfiguration</code>,
    <code>@BeanMapping(ignoreByDefault = true)</code>, a <code>"."</code> wildcard, a <code>target</code> written as a constant — and any annotation on the method it does
    not recognise, since MapStruct lets a project compose its own mapping annotations.</li>
  <li><strong>Collection, array, map, <code>Optional</code> and generic methods</strong>, which MapStruct maps element by element through other methods.</li>
  <li>A property Lombok's constructor or builder only <em>may</em> take is never reported as unmapped, and a name is unknown only when nothing on the type carries it.</li>
</ul>
