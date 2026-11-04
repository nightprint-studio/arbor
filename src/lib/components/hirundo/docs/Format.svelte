<h1>On disk</h1>
<p class="doc-lead">Everything is readable text, made to be committed.</p>
<pre>my-project/
└─ .hirundo/
   ├─ collections/
   │  └─ orders/                (a collection)
   │     ├─ collection.toml     (name, auth, headers, variables)
   │     ├─ list.toml           (a request)
   │     └─ admin/              (a folder)
   │        ├─ folder.toml
   │        └─ purge.toml
   ├─ environments/             (one file per environment)
   ├─ env-types.toml            (types: colour, protection, shared variables)
   └─ local.toml                (this machine only — ignored by git)</pre>
<ul>
  <li>A request file holds its name, method, URL, params, headers, auth, body, tests and notes; renaming a request changes the name inside, not the file.</li>
  <li>Secrets are never in these files: a variable marked <code>@secret</code> has its value in the keychain.</li>
  <li>History, runs, schedules and the cookie jar belong to you, not the repository: they live in your profile, per project.</li>
</ul>
