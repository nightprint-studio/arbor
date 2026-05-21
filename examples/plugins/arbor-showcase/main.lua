-- arbor-showcase/main.lua
-- Demonstrates every Arbor plugin API surface.
-- Thin wiring file — all logic is inline here for clarity.

-- ── Helpers ───────────────────────────────────────────────────────────────────

local function gsetting(key, default)
  local v = arbor.settings.global.get(key)
  return (v ~= nil) and v or default
end

-- ── Lifecycle ─────────────────────────────────────────────────────────────────

arbor.events.on("on_plugin_load", function(ctx)
  -- Increment a persistent load counter.
  local count = (gsetting("run_count", 0) or 0) + 1
  arbor.settings.global.set("run_count", count)
  arbor.log.info(string.format(
    "loaded %d time(s) — api_version=%s app=%s",
    count, ctx.api_version, arbor.meta.app_version
  ))
end)

-- ── UI registrations ──────────────────────────────────────────────────────────

arbor.ui.add_sidebar({
  id      = "showcase-panel",
  label   = "Showcase",
  icon    = "Zap",
  side    = "right",
  tooltip = "Showcase",
})

arbor.ui.add_context_menu_item({
  target = "commit",
  label  = "✨ Inspect Commit (Showcase)",
  action = "showcase:inspect_commit",
  icon   = "Zap",
})

arbor.ui.add_context_menu_item({
  target = "branch",
  label  = "✨ Branch Info (Showcase)",
  action = "showcase:inspect_branch",
  icon   = "Zap",
})

arbor.ui.add_menu_item({
  label  = "Open Showcase Panel",
  action = "showcase:open_panel",
  icon   = "Zap",
})

-- ── Hook: panel opened ───────────────────────────────────────────────────────
-- The new `add_sidebar` API fires `panel:open:<id>` instead of the legacy
-- `sidebar_open:<action>` event. Push a minimal panel body that explains
-- what the plugin does and offers a button to open the preferences form.

arbor.events.on("panel:open:showcase-panel", function(_ctx)
  arbor.ui.set_panel_content("showcase-panel", {
    title = "Arbor Showcase",
    nodes = {
      { type = "paragraph",
        text = "Demonstrates every Arbor plugin API surface. See plugins/arbor-showcase/main.lua for the source." },
      { type = "button", label = "Open Preferences",   icon = "Settings",
        variant = "primary", action = "showcase:open_form" },
      { type = "button", label = "Open Dashboard Demo", icon = "BarChart3",
        action = "showcase:open_dashboard" },
    },
  })
end)

arbor.events.on("showcase:open_form", function(_ctx)
  arbor.ui.form({
    title         = "⚡ Arbor Showcase",
    submit_label  = "Save preferences",
    submit_action = "showcase:save_prefs",
    cancel_action = "showcase:cancel",
    nodes = {
      { type = "section", title = "Identity", children = {
        { type = "text", name = "username", label = "Your name",
          placeholder = "e.g. Alice", default = gsetting("username", "") },
      }},
      { type = "section", title = "Appearance", children = {
        { type = "select", name = "theme", label = "Preferred theme",
          default = gsetting("theme", "dark"),
          options = {
            { value = "dark",   label = "Dark"   },
            { value = "light",  label = "Light"  },
            { value = "system", label = "System" },
          }},
      }},
      { type = "checkbox", name = "notifications",
        label   = "Enable scheduled notifications",
        default = gsetting("notifications", true) },
    },
  })
end)

arbor.events.on("showcase:save_prefs", function(ctx)
  arbor.settings.global.set("username",      ctx.username)
  arbor.settings.global.set("theme",         ctx.theme)
  arbor.settings.global.set("notifications", ctx.notifications)
  arbor.notify{ message = "Preferences saved!", level = "success" }
  arbor.log.info("prefs saved — username=" .. (ctx.username or ""))
end)

arbor.events.on("showcase:cancel", function(_ctx)
  arbor.log.debug("showcase form cancelled")
end)

-- ── Dashboard demo (counter_grid + score_gauge + time_series_chart + filter_bar + data_table) ──
-- Pure display form. Uses fake data so it works in any repo without
-- network calls.

local ALL_FINDINGS = {
  { id = "f-1", severity = "Critical", repo = "api",   _severity_color = "var(--severity-critical)",
    title = "SQL injection in /api/users", file = "api/users.go:142", age = 8  },
  { id = "f-2", severity = "High",     repo = "infra", _severity_color = "var(--severity-high)",
    title = "Outdated TLS version",     file = "infra/tls.tf:7",     age = 31 },
  { id = "f-3", severity = "High",     repo = "web",   _severity_color = "var(--severity-high)",
    title = "Path traversal in upload", file = "web/upload.ts:88",   age = 4  },
  { id = "f-4", severity = "Medium",   repo = "auth",  _severity_color = "var(--severity-medium)",
    title = "Weak hashing algorithm",   file = "auth/hash.rs:23",    age = 92 },
  { id = "f-5", severity = "Low",      repo = "api",   _severity_color = "var(--severity-low)",
    title = "Verbose error in 500",     file = "api/error.go:55",    age = 14 },
}

-- Live filter state — updated by the filter_bar's change_action and consumed
-- on the next form.replace call.
local dash_filter = { search = "", filters = {} }

local function dash_filter_findings()
  local q   = (dash_filter.search or ""):lower()
  local sev = dash_filter.filters and dash_filter.filters.severity or {}
  local rep = dash_filter.filters and dash_filter.filters.repo     or {}
  local sev_set, rep_set = {}, {}
  for _, v in ipairs(sev) do sev_set[v] = true end
  for _, v in ipairs(rep) do rep_set[v] = true end

  local out = {}
  for _, f in ipairs(ALL_FINDINGS) do
    local pass = true
    if q ~= "" then
      local hay = (f.title .. " " .. f.file):lower()
      pass = hay:find(q, 1, true) ~= nil
    end
    if pass and next(sev_set) ~= nil then pass = sev_set[f.severity] == true end
    if pass and next(rep_set) ~= nil then pass = rep_set[f.repo]     == true end
    if pass then table.insert(out, f) end
  end
  return out
end

local function dashboard_nodes()
  return {
    { type = "paragraph", variant = "muted",
      text = "Generic dashboard primitives — any plugin can compose its own dashboard with these five nodes." },

    { type = "section", title = "Counters", children = {
      { type = "counter_grid",
        actions = { select = "showcase:dash_card_clicked" },
        items = {
          { key = "open",     label = "Open issues",  value = 42, hint = "+3 this week",
            color = "var(--severity-high)"     },
          { key = "blocked",  label = "Blocked",      value = 7,  hint = "owner: build",
            color = "var(--severity-critical)" },
          { key = "wip",      label = "In progress",  value = 12, hint = "median 3.2d",
            color = "var(--accent)"            },
          { key = "review",   label = "In review",    value = 5,  hint = "median 1.1d",
            color = "var(--severity-medium)"   },
          { key = "ready",    label = "Ready",        value = 3,  hint = "—",
            color = "var(--severity-low)"      },
          { key = "done",     label = "Closed today", value = 0,  empty = true },
        },
      },
    }},

    { type = "container", columns = 2, gap = 12, children = {

      { type = "section", title = "Risk score", children = {
        { type = "score_gauge",
          value    = 73.5,
          label    = "High risk",
          size     = "md",
          segments = {
            { from = 0,  to = 25,  color = "var(--severity-info)"     },
            { from = 25, to = 50,  color = "var(--severity-medium)"   },
            { from = 50, to = 75,  color = "var(--severity-high)"     },
            { from = 75, to = 100, color = "var(--severity-critical)" },
          },
        },
      }},

      { type = "section", title = "Trend", children = {
        { type = "time_series_chart",
          height      = 200,
          show_legend = true,
          series = {
            { id = "critical", label = "Critical", color = "var(--severity-critical)",
              points = {
                { x = "2026-04-29", y = 5 }, { x = "2026-04-30", y = 4 },
                { x = "2026-05-01", y = 6 }, { x = "2026-05-02", y = 5 },
                { x = "2026-05-03", y = 7 }, { x = "2026-05-04", y = 6 },
                { x = "2026-05-05", y = 8 }, { x = "2026-05-06", y = 9 },
              } },
            { id = "high", label = "High", color = "var(--severity-high)",
              points = {
                { x = "2026-04-29", y = 12 }, { x = "2026-04-30", y = 10 },
                { x = "2026-05-01", y = 11 }, { x = "2026-05-02", y = 13 },
                { x = "2026-05-03", y = 12 }, { x = "2026-05-04", y = 14 },
                { x = "2026-05-05", y = 13 }, { x = "2026-05-06", y = 12 },
              } },
          },
        },
      }},
    }},

    { type = "section", id = "dash-findings", title = "Findings (filter_bar + data_table)", children = {
      { type    = "filter_bar",
        id      = "dash-filter",
        name    = "dash_filter",
        default = dash_filter,
        search  = { placeholder = "Search title or file…" },
        actions = { change = "showcase:dash_filter_changed" },
        filters = {
          { id = "severity", label = "Severity", icon = "ShieldAlert",
            options = {
              { value = "Critical", label = "Critical", color = "var(--severity-critical)" },
              { value = "High",     label = "High",     color = "var(--severity-high)"     },
              { value = "Medium",   label = "Medium",   color = "var(--severity-medium)"   },
              { value = "Low",      label = "Low",      color = "var(--severity-low)"      },
            }},
          { id = "repo", label = "Repo", icon = "GitBranch", searchable = true,
            options = {
              { value = "api",   label = "api"   },
              { value = "web",   label = "web"   },
              { value = "auth",  label = "auth"  },
              { value = "infra", label = "infra" },
            }},
        },
      },
      { type         = "data_table",
        id           = "dash-table",
        row_key      = "id",
        height       = 240,
        initial_sort = { key = "age", dir = "desc" },
        empty        = "No findings match the current filters.",
        actions      = { row_click = "showcase:dash_row_clicked" },
        columns = {
          { key = "severity", label = "Severity", width = "100px", kind = "pill", sortable = true },
          { key = "title",    label = "Title",    width = "1fr",   sortable = true },
          { key = "file",     label = "File",     width = "240px", kind = "code" },
          { key = "age",      label = "Age",      width = "70px",  kind = "age", align = "right", sortable = true },
        },
        rows = dash_filter_findings(),
      },
    }},
  }
end

arbor.events.on("showcase:open_dashboard", function(_ctx)
  arbor.ui.form({
    title         = "Dashboard demo",
    width         = "920px",
    height        = "80vh",
    hide_submit   = true,
    cancel_label  = "Close",
    cancel_action = "showcase:cancel",
    nodes         = dashboard_nodes(),
  })
end)

arbor.events.on("showcase:dash_card_clicked", function(ctx)
  arbor.notify{ message = "Counter clicked: " .. (ctx.key or "?"), level = "info" }
end)

arbor.events.on("showcase:dash_row_clicked", function(ctx)
  arbor.notify{ message = "Row clicked: " .. (ctx.row_id or "?"), level = "info" }
end)

-- Filter changes — persist to closure state and rebuild the dashboard so the
-- data_table picks up the filtered rows. form.replace preserves field values
-- by `name`, so the filter_bar keeps its UI state across rebuilds.
arbor.events.on("showcase:dash_filter_changed", function(ctx)
  if ctx and ctx.value then dash_filter = ctx.value end
  arbor.ui.form.replace({ nodes = dashboard_nodes() })
end)

-- ── Hook: context-menu actions ────────────────────────────────────────────────

arbor.events.on("showcase:inspect_commit", function(ctx)
  arbor.log.info("inspect_commit oid=" .. (ctx.oid or "?"))
  arbor.notify{ message = "Commit inspected — see log for details.", level = "info" }
end)

arbor.events.on("showcase:inspect_branch", function(ctx)
  arbor.log.info("inspect_branch branch=" .. (ctx.branch or "?"))
  arbor.notify{ message = "Branch info logged.", level = "info" }
end)

arbor.events.on("showcase:open_panel", function(_ctx)
  arbor.notify{ message = "Use the sidebar Showcase section to open the panel.", level = "info" }
end)

-- ── Hooks: git events ────────────────────────────────────────────────────────

arbor.events.on("on_commit", function(ctx)
  arbor.log.info("on_commit oid=" .. (ctx.oid or "?"))
  arbor.notify{ message = "[showcase] Commit detected!", level = "success" }
end)

arbor.events.on("on_push", function(ctx)
  arbor.log.info("on_push remote=" .. (ctx.remote or "?") .. " branch=" .. (ctx.branch or "?"))
  arbor.notify{ message = "[showcase] Push detected!", level = "info" }
end)

arbor.events.on("on_checkout", function(ctx)
  arbor.log.info("on_checkout branch=" .. (ctx.branch or "?"))
  arbor.notify{ message = "[showcase] Branch checkout detected!", level = "info" }
end)

arbor.events.on("on_fetch", function(ctx)
  arbor.log.info("on_fetch remote=" .. (ctx.remote or "?"))
  arbor.notify{ message = "[showcase] Fetch detected!", level = "info" }
end)

arbor.events.on("on_repo_open", function(ctx)
  local count = gsetting("run_count", 1)
  arbor.log.info("on_repo_open repo=" .. (ctx.repo or "?"))
  arbor.notify{ message = string.format("[showcase] Repository opened (load #%d)", count), level = "success" }
end)

-- ── Scheduler: fires every 60 s ──────────────────────────────────────────────

arbor.scheduler.register({
  action     = "showcase:tick",
  fixed_rate = "60s",
})

local tick_count = 0

arbor.events.on("showcase:tick", function(_ctx)
  tick_count = tick_count + 1
  arbor.settings.global.set("last_tick", tick_count)
  arbor.log.info("scheduler tick #" .. tick_count)

  if gsetting("notifications", true) then
    arbor.notify{ title = "Arbor Showcase", message = string.format("Background tick #%d — everything is working!", tick_count) }
  end
end)

-- ── Settings form ─────────────────────────────────────────────────────────────

arbor.events.on("showcase:configure", function(_ctx)
  arbor.ui.form({
    title         = "Showcase Settings",
    submit_label  = "Save",
    submit_action = "showcase:save_prefs",
    nodes = {
      { type = "text",     name = "username",      label = "Your name",
        default = gsetting("username", "") },
      { type = "checkbox", name = "notifications", label = "Enable scheduled notifications",
        default = gsetting("notifications", true) },
      -- demonstrate kv_list
      { type = "kv_list",  name = "custom_labels", label = "Custom Labels",
        key_placeholder = "Key", value_placeholder = "Value",
        default = gsetting("custom_labels", {}) },
    },
  })
end)

arbor.log.info("all hooks registered.")
