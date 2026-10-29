<span class="eyebrow">Imber</span>
<h1>Imber: Skills</h1>

<p class="doc-lead">
  A skill is a folder with a <code>SKILL.md</code> that teaches Imber how you want one kind of task done; none is known to Imber until you enable it in the skills manager.
</p>

<h2>The format</h2>
<p>
  The format is the one Claude Code uses: a <code>SKILL.md</code> with a frontmatter holding <code>name</code> and <code>description</code>, then the instructions. Examples: a release checklist, how to write a migration, how your team names tests.
</p>
<pre>---
name: write-migration
description: How to write a database migration in this project
---
1. Put the file in db/migrations, named V&lt;next number&gt;__&lt;what&gt;.sql.
2. Never edit a migration that has been applied.</pre>

<h2>Where skills are looked for</h2>
<ul>
  <li>each project's <code>.bennu/skills</code> and <code>.claude/skills</code>;</li>
  <li>Bennu's own <code>skills</code> folder in its config folder;</li>
  <li><code>~/.claude/skills</code>.</li>
</ul>
<p>
  A project skill hides a user skill of the same name, and <code>.bennu</code> wins over <code>.claude</code>.
</p>

<h2>The skills manager</h2>
<p>
  Open it from <strong>Skills…</strong> in the panel's <strong>…</strong> menu, the composer's settings menu, Settings › Imber › Skills, or the palette entry <em>Imber: Manage skills…</em>. It lists each skill with where it came from, its size and a preview of its text; switches them one by one or all at once; and lists folders it could not use with the reason. The Settings page and the composer show "N of M enabled".
</p>
<p>
  Enabling is an allow-list stored in the config (<code>skills_on</code>, ids of the form <code>user/name</code> or <code>project/name</code>): a skill found on disk but not ticked is invisible to Imber.
</p>

<h2>How a skill is used</h2>
<ul>
  <li>Only a one-line index of the enabled skills (name and description) travels with each request.</li>
  <li>Imber loads a skill's text with <code>load_skill</code> when a task matches, or when you start your message with <code>/name</code>. Type <code>/</code> in the composer to pick from the enabled skills.</li>
  <li>Limits: a skill's text is cut at 32 KB, at most 50 skills are read, and a description is cut at 240 characters.</li>
  <li>Nothing in a skill is ever run. Other files in its folder are only named; Imber can read them with its ordinary tools only if the reach allows it.</li>
</ul>
<p>Skills are not available in a chat.</p>
