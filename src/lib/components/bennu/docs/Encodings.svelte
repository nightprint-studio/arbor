<!-- Bennu docs — encodings: reading legacy files correctly, and spotting the ones already broken. -->
<h1>Encodings</h1>
<p class="doc-lead">
  Legacy Java sources are very often not UTF-8, and reading one as if it were quietly corrupts
  every accented character in it. Bennu reads each file as what it is, and can tell you which
  files were damaged before you ever opened them.
</p>

<h2>Encodings</h2>
<p>
  Legacy projects often declare <code>Cp1252</code> in their <code>pom.xml</code>
  (<code>project.build.sourceEncoding</code>). Bennu decodes each file with the pom-declared
  encoding and shows which one won in the footer, so a mojibake surprise never slips in silently.
</p>
<p>
  A project that declares nothing falls back to <strong>Settings → Java → Default source
  encoding</strong>. It is only ever a fallback: a declared <code>sourceEncoding</code> wins over
  it, and a per-file override wins over both.
</p>
<p>
  A <strong>Cargo</strong> project is always <code>UTF-8</code>: Rust source is UTF-8 by language
  definition, so the encoding default configured for a legacy Java tree never reaches it.
</p>
<h2>Mojibake check</h2>
<p>
  <strong>Check file for mojibake</strong> (Command Palette) scans the open file for text that was
  UTF-8 but got read as Windows-1252 — the classic <code>Ã©</code> for <code>é</code> or
  <code>â€™</code> for <code>'</code>. Each hit is squiggled with a one-click
  <strong>Replace with «…»</strong> quick-fix, and a summary tells you how many were found.
  Detection is exact (a table of real corruption sequences), so clean accented text is never flagged.
</p>
