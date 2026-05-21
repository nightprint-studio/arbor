-- container-test — test plugin for the Phase 2 ContributableModal.
--
-- Registers one container (kind=modal, layout=tree_nav) plus 2 categories
-- and 3 sections that exercise the typical FormNode shapes (text, select,
-- toggle, switch).
--
-- Open the modal by firing `container_test:open` from a keybinding,
-- toolbar, or — easiest — from the Lua REPL once the plugin is loaded:
--
--   arbor.ui.container.open("container-test::demo")
--
-- Each section's on_save logs the slice it received so we can confirm the
-- parallel save dispatch is wired up. The host on_save logs the full
-- namespaced state.

local CONTAINER_KEY = "container-test::demo"

-- ─── Container definition ────────────────────────────────────────────────
arbor.ui.container.register({
  id            = "demo",
  kind          = "modal",
  layout        = "tree_nav",
  title         = "Container Test — Phase 2 Demo",
  width         = "880px",
  submit_label  = "Save All",
  cancel_label  = "Discard",
  on_save       = "container_test:host_save",
})

-- ─── Categories (nav rows) ───────────────────────────────────────────────
arbor.ui.contribute(CONTAINER_KEY .. ":category", {
  id       = "general",
  priority = 10,
  payload  = { label = "General",     icon = "Settings", description = "App-wide preferences" },
})
arbor.ui.contribute(CONTAINER_KEY .. ":category", {
  id       = "advanced",
  priority = 20,
  payload  = { label = "Advanced",    icon = "Cog",      description = "Power-user knobs" },
})

-- ─── Sections ────────────────────────────────────────────────────────────
-- Section 1 — General / Identity. Shows text + select.
arbor.ui.contribute(CONTAINER_KEY .. ":section", {
  id       = "identity",
  priority = 10,
  payload  = {
    category = "general",
    label    = "Identity",
    icon     = "User",
    on_save  = "container_test:save_identity",
    nodes    = {
      { type = "text",   name = "name",     label = "Display name", default = "Anonymous" },
      { type = "select", name = "language", label = "Language",
        options = { "en", "it", "fr" }, default = "en" },
    },
  },
})

-- Section 2 — General / Theme. Shows toggle + switch case.
arbor.ui.contribute(CONTAINER_KEY .. ":section", {
  id       = "theme",
  priority = 20,
  payload  = {
    category = "general",
    label    = "Theme",
    icon     = "Palette",
    on_save  = "container_test:save_theme",
    nodes    = {
      { type = "toggle", name = "dark_mode", label = "Dark mode", default = true },
      { type = "select", name = "accent",    label = "Accent",
        options = { "blue", "green", "purple", "orange" }, default = "blue" },
    },
  },
})

-- Section 3 — Advanced / Diagnostics. Single textarea.
arbor.ui.contribute(CONTAINER_KEY .. ":section", {
  id       = "diagnostics",
  priority = 10,
  payload  = {
    category = "advanced",
    label    = "Diagnostics",
    icon     = "Activity",
    on_save  = "container_test:save_diag",
    nodes    = {
      { type = "toggle", name = "verbose",     label = "Verbose logging", default = false },
      { type = "number", name = "max_threads", label = "Max threads",
        min = 1, max = 32, default = 4 },
    },
  },
})

-- ─── Action handlers ─────────────────────────────────────────────────────
-- Each section save just logs what it got. host_save sees the aggregated
-- state across all sections. None of them mutate any real state — this
-- plugin is purely for end-to-end manual testing.

arbor.events.on("container_test:open", function(_ctx)
  arbor.ui.container.open(CONTAINER_KEY)
end)

arbor.events.on("container_test:save_identity", function(ctx)
  arbor.log.info("[container-test] save_identity: " .. arbor.json.encode(ctx))
  arbor.notify{ title = "container-test", message = "identity saved", level = "success" }
end)

arbor.events.on("container_test:save_theme", function(ctx)
  arbor.log.info("[container-test] save_theme: " .. arbor.json.encode(ctx))
  arbor.notify{ title = "container-test", message = "theme saved", level = "success" }
end)

arbor.events.on("container_test:save_diag", function(ctx)
  arbor.log.info("[container-test] save_diag: " .. arbor.json.encode(ctx))
  arbor.notify{ title = "container-test", message = "diagnostics saved", level = "success" }
end)

arbor.events.on("container_test:host_save", function(ctx)
  arbor.log.info("[container-test] host_save (full state): " .. arbor.json.encode(ctx))
  arbor.notify{ title = "container-test", message = "all sections saved", level = "info" }
end)
