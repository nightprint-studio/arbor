# Bennu assistant — cross-project mode (proposal)

Status: proposal, nothing implemented. Written after reading the index service and the assistant's access policy; the points marked **(to verify)** were not checked in code.

## What the user wants

Let the assistant work across several projects at once — "where is this DTO used in the other services?", "make the same change in the client and the server" — which means those projects have to be **indexed and kept in memory** by Bennu, not read as loose text.

## What already exists

- **The index holds many projects.** `index_service.rs` keeps `slots: HashMap<PathBuf, Arc<ProjectSlot>>`, one slot per project root, each with its own index and validation. Bennu is already multi-project in the backend; the editor window shows one at a time.
- **The access policy has the scope.** `Scope::Workspace` ("All open projects") exists, with `workspace_roots` passed in each send. It decides *where the assistant may read and write*.
- **Most catalogue tools take a `root`.** The host fills in the session's root when the model leaves it out; a model that names another root reaches that project's slot **(to verify: that every tool resolves the slot from `root` and not from a "current project")**.

So the pieces are two: *which projects are in play*, and *that they are loaded*.

## What is missing

1. **The assistant does not know the other projects exist.** The system prompt names one root. It needs a short list: each project's name, root, kind (Maven, Gradle, Cargo…) and whether its index is ready.
2. **Nothing keeps a project warm.** A slot exists while a project is open or has been opened this session; there is no "load this one for the assistant". An unloaded project answers "the index is still building" or not at all.
3. **No memory bound.** An index per project is real memory (the JDK index is shared, the project's own is not). Several large legacy projects together can be a lot. No eviction exists today.
4. **Results do not say which project they come from.** A usages list across projects needs the project on each line, or the model will edit the wrong `Order.java`.
5. **A change set that spans projects.** Today a proposal belongs to one root (`PendingEdit.root`); the review card lists files without grouping by project.
6. **The cost.** Each extra project adds to what the model reads. Without a limit, "look at all of them" is the most expensive sentence the assistant can be asked.

## Proposal

### Settings and the composer

- Scope **Workspace** gets a real list: *Projects the assistant may work in*, chosen from the projects Bennu knows (recent and open), each with a toggle. Stored with the other assistant settings; per conversation it can be narrowed, as the reach is today.
- Nothing is added to the composer except a project chip row when more than one is allowed, so it is visible which are in play.

### Loading

- When a conversation with several projects starts, the backend **asks the index service to load each listed project** (the same call the editor makes on open, headless: no window). Progress is shown as one line in the thread — "Indexing orders-api… ready" — and the run waits for the indexes it needs, not all of them: a tool call on a project that is not ready waits, up to a bound, then says so.
- **Memory bound:** a setting, *Projects kept in memory* (default 3). Past it the least recently used project that is not open in the editor is unloaded. The user's own open project is never evicted.

### What the model sees

- A *Projects* block in the system prompt: name, root, kind, ready or not. The session's own project is marked as the default.
- Every tool whose answer names files shows paths **prefixed by the project name** when more than one is in play (`orders-api:src/…/Order.java`), and every tool that takes a `root` takes the project name as an alias. The host maps the name back to the root; the model never types a long path to switch.
- Searches across projects are a loop on the host's side: `search_text`, `bennu_find_usages` and friends accept `project: "all"` and return grouped results, capped per project so one project cannot fill the answer.

### Changes

- A proposal is **grouped by project**: one card with a section per project, each section accepted or rejected alone. Applying one project's changes never depends on another's.
- A single `edit_files` call may span projects; it stays all-or-nothing **per project** (an all-or-nothing over several roots would need a transaction the filesystem does not give us — the reply says which projects were applied).
- The access policy applies per project: a project that is listed but read-only in the user's settings stays read-only.

### Cost and limits

- Tool results across projects are cut harder (a per-project cap), and the run's euro limit is unchanged — it already counts everything.
- The dry-run preview shows the *Projects* block like any other part of the request.

## Order of work

1. The *Projects* block, the per-project name alias and path prefixing — useful on its own, because a model working on one project with another listed as read-only already benefits. (small)
2. Loading and the memory bound in the index service. (medium; the risky part, because it touches a shared service)
3. `project: "all"` on search and usages. (small)
4. Proposals grouped by project, and the review card. (medium; touches the card and the apply path)

Steps 1 and 3 can ship without 2 if the allowed projects are only those already open in Bennu.

## Decisions needed

- **Memory:** is "3 projects kept" the right default, and may it unload a project the user opened earlier but is not looking at?
- **Writes across projects:** allow it from the first version, or read-only across projects first and writes later? Read-only first is the safer path and covers most of the questions ("who calls this in the other services?").
- **Which projects:** only the ones open in Bennu, or any folder the user picks (which means indexing a project that was never opened)?
