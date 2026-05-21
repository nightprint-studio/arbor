-- panel-demo — minimal proof-of-concept for the right ActivityBar + plugin
-- panel API introduced with `arbor.ui.add_sidebar` + `arbor.ui.set_panel_content`.
--
-- Exercised surface:
--   * add_sidebar(side="right", position="top")    → right sidebar icon + panel
--   * add_sidebar(side="right", position="bottom") → right-bottom icon opens
--                                                   the unique bottom panel
--   * panel:open:<id> hook → lazily produces body content via set_panel_content
--   * form-DSL nodes: heading, label, divider, list with click actions, button
--
-- Everything here runs in-process; no filesystem or git permissions.

local M = {}

-- Session counter so you can verify the panel actually re-fires the hook
-- every time you re-open it.
local open_count = 0

local function push_main_panel()
  open_count = open_count + 1

  arbor.ui.set_panel_content("overview", {
    title = "Panel Demo",
    nodes = {
      { type = "heading", text = "Right-side panels" },
      { type = "label",
        text = "This panel is pushed live by the plugin in response to the "
            .. "panel:open hook. Reopens re-fire the hook — current count: "
            .. tostring(open_count) },
      { type = "divider" },

      -- Icons can be either single-char emojis ("✓") or Lucide names ("Info").
      -- Lucide icons inherit the surrounding text color and size like any
      -- built-in Arbor icon — so they stay visually consistent when the
      -- theme changes.
      { type = "list", items = {
        { id = "toast-info",    icon = "Info",          label = "Show info toast",
          action = "panel-demo:toast" },
        { id = "toast-success", icon = "CheckCircle2",  label = "Show success toast",
          action = "panel-demo:toast", detail = "ok" },
        { id = "notify",        icon = "Bell",          label = "Push a notification",
          action = "panel-demo:notify" },
        { id = "refresh",       icon = "RefreshCw",     label = "Refresh this panel",
          action = "panel-demo:refresh" },
        { id = "emoji-demo",    icon = "🧪",            label = "Emoji icon still works",
          action = "panel-demo:toast" },
      }},

      { type = "divider" },
      { type = "label", text = "Try moving this icon with Customize Activity Bar " ..
                               "→ the Right tab." },
    },
    actions = {
      { label = "Open bottom demo", icon = "PanelBottom", action = "panel-demo:open-bottom" },
    },
  })
end

local function push_bottom_panel()
  arbor.ui.set_panel_content("runtime", {
    title = "Panel Demo — runtime log",
    nodes = {
      { type = "heading", text = "Bottom panels are unique" },
      { type = "label",
        text = "Opening this from the right bar ALSO closes any other bottom "
            .. "panel (stage / detail / terminal / jobs / pipelines). Only one "
            .. "bottom panel is ever visible — regardless of which side fired it." },
      { type = "divider" },
      { type = "button", label = "Send a success toast", action = "panel-demo:toast" },
    },
  })
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Lifecycle
-- ─────────────────────────────────────────────────────────────────────────────

arbor.events.on("on_plugin_load", function(_ctx)
  -- `icon` accepts either an emoji glyph or a Lucide icon name — both
  -- render at the same size/color as built-in Arbor icons.
  arbor.ui.add_sidebar({
    id       = "overview",
    icon     = "Puzzle",              -- Lucide name
    label    = "Panel Demo",
    tooltip  = "Right-side demo panel (plugin PoC)",
    side     = "right",
    position = "top",
  })

  arbor.ui.add_sidebar({
    id       = "runtime",
    icon     = "📋",                  -- emoji still works
    label    = "Panel Demo — bottom",
    tooltip  = "Demo of plugin-registered BOTTOM panels (unique slot)",
    side     = "right",
    position = "bottom",
  })
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- Panel content producers — fired every time the user opens the panel.
-- ─────────────────────────────────────────────────────────────────────────────

arbor.events.on("panel:open:overview", function(_ctx)
  push_main_panel()
end)

arbor.events.on("panel:open:runtime", function(_ctx)
  push_bottom_panel()
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- Click handlers
-- ─────────────────────────────────────────────────────────────────────────────

arbor.events.on("panel-demo:toast", function(ctx)
  local label = (ctx and ctx.label) or "Panel demo action fired"
  local level = ((ctx and ctx.detail) == "ok")
    and "success" or "info"
  arbor.notify{ message = label, level = level }
end)

arbor.events.on("panel-demo:notify", function(_ctx)
  arbor.notify{ title = "Panel demo", message = "Notifications surface in the bell icon — try it.", level = "info" }
end)

arbor.events.on("panel-demo:refresh", function(_ctx)
  push_main_panel()
end)

arbor.events.on("panel-demo:open-bottom", function(_ctx)
  -- Fire the panel:open hook manually so the content is ready when the UI
  -- opens it. Since the panel is a plugin-registered bottom entry, the user
  -- activates it by clicking the "📋" icon in the right ActivityBar.
  push_bottom_panel()
  arbor.notify{ title = "Panel demo", message = "Click the 📋 icon on the RIGHT bar to open the runtime panel.", level = "info" }
end)

return M
