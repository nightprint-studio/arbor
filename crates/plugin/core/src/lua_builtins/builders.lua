-- builders.lua — chainable sugar over arbor.pipeline.define and arbor.ui.form.
--
-- Both legacy table-config entry points keep working unchanged:
--     arbor.pipeline.define{ id = "...", stages = { ... } }
--     arbor.ui.form{ title = "...", nodes = { ... } }
--
-- This script installs metatables AFTER api::register has published the
-- arbor.* global, so the new call shapes coexist with the old ones:
--     arbor.pipeline("id"):name("X"):stage("build"):shell("make"):commit()
--     arbor.ui.form():title("X"):text("name"):on_submit("save"):open()
--
-- The builders compile down to the same table-config the legacy entry points
-- consume — they add no Rust-side surface, they only assemble Lua tables.

local function _slugify(s)
  s = tostring(s or ""):lower()
  s = s:gsub("[^a-z0-9_]+", "-")
  s = s:gsub("^%-+", ""):gsub("%-+$", "")
  if s == "" then return "stage" end
  return s
end

-- =========================================================================
-- Pipeline builder
-- =========================================================================

local PipelineBuilder = {}
PipelineBuilder.__index = PipelineBuilder

local function _current_stage(self)
  local s = self._stages[#self._stages]
  if not s then
    error("pipeline builder: call :stage(name) before :run/:shell/:step", 3)
  end
  return s
end

function PipelineBuilder:name(v)        self._cfg.name        = v; return self end
function PipelineBuilder:description(v) self._cfg.description = v; return self end
function PipelineBuilder:icon(v)        self._cfg.icon        = v; return self end
function PipelineBuilder:lock(v)        self._cfg.lock_key    = v; return self end
function PipelineBuilder:lock_key(v)    self._cfg.lock_key    = v; return self end
function PipelineBuilder:log_level(v)   self._cfg.log_level   = v; return self end
---Suppress the host's automatic start-toast / done-notification for runs of
---this pipeline (default false). Per-run override available via
---`arbor.pipeline.run{ silent = ... }`.
function PipelineBuilder:silent(v)      self._cfg.silent      = (v ~= false); return self end

---Begin a new stage. Subsequent :run / :shell / :step calls add steps to it.
---@param name_or_cfg string|table  Stage name (string) or {id, name, mode, max_parallel}
function PipelineBuilder:stage(name_or_cfg)
  local stage
  if type(name_or_cfg) == "table" then
    stage = {
      id           = name_or_cfg.id   or _slugify(name_or_cfg.name or ""),
      name         = name_or_cfg.name or name_or_cfg.id or "",
      mode         = name_or_cfg.mode,
      max_parallel = name_or_cfg.max_parallel,
      steps        = {},
    }
  else
    local nm = tostring(name_or_cfg or "")
    stage = { id = _slugify(nm), name = nm, steps = {} }
  end
  table.insert(self._stages, stage)
  return self
end

---Set the mode of the current stage. Valid: "sequential" | "parallel".
function PipelineBuilder:mode(m)
  _current_stage(self).mode = m
  return self
end

---Cap concurrency when the current stage is parallel.
function PipelineBuilder:max_parallel(n)
  _current_stage(self).max_parallel = n
  return self
end

local function _push_step(self, step)
  local stage = _current_stage(self)
  step.id   = step.id   or ("s" .. (#stage.steps + 1))
  step.name = step.name or step.id
  table.insert(stage.steps, step)
end

---Add a Lua-op step. Two call shapes:
---   :run("op_name", { params })
---   :run({ op = "op_name", params = {...}, plugin = "...", id = "...", name = "...", allow_failure = ... })
function PipelineBuilder:run(op, params)
  if type(op) == "table" then
    _push_step(self, {
      id            = op.id,
      name          = op.name,
      cwd           = op.cwd,
      allow_failure = op.allow_failure,
      lua_op        = { op = op.op, params = op.params or {}, plugin = op.plugin },
    })
  else
    _push_step(self, { lua_op = { op = op, params = params or {} } })
  end
  return self
end

---Add a shell step. Either:
---   :shell("make build")
---   :shell({ command = "make build", cwd = "...", id = "...", allow_failure = ... })
function PipelineBuilder:shell(cmd_or_cfg)
  if type(cmd_or_cfg) == "table" then
    _push_step(self, {
      id            = cmd_or_cfg.id,
      name          = cmd_or_cfg.name,
      command       = cmd_or_cfg.command,
      cwd           = cmd_or_cfg.cwd,
      allow_failure = cmd_or_cfg.allow_failure,
    })
  else
    _push_step(self, { command = tostring(cmd_or_cfg) })
  end
  return self
end

---Add a raw step (escape hatch for fields the helpers don't cover).
function PipelineBuilder:step(cfg)
  if type(cfg) ~= "table" then
    error("pipeline builder: :step expects a table", 2)
  end
  _push_step(self, cfg)
  return self
end

---Finalise the builder and call arbor.pipeline.define with the assembled config.
function PipelineBuilder:commit()
  local cfg = {}
  for k, v in pairs(self._cfg) do cfg[k] = v end
  cfg.stages = self._stages
  return arbor.pipeline.define(cfg)
end

-- Install __call on arbor.pipeline so `arbor.pipeline("id")` returns a builder.
-- arbor.pipeline.define / .run / .list_ops etc. are unaffected — those are
-- index lookups on the table, not __call invocations.
do
  local pmeta = getmetatable(arbor.pipeline) or {}
  pmeta.__call = function(_self, id_or_cfg)
    local cfg
    if type(id_or_cfg) == "table" then
      cfg = id_or_cfg
    else
      cfg = { id = id_or_cfg }
    end
    return setmetatable({
      _cfg    = cfg,
      _stages = (type(id_or_cfg) == "table" and id_or_cfg.stages) or {},
    }, PipelineBuilder)
  end
  setmetatable(arbor.pipeline, pmeta)
end

-- =========================================================================
-- Form builder
-- =========================================================================

local FormBuilder = {}
FormBuilder.__index = FormBuilder

local function _push_node(self, node)
  local top = self._stack[#self._stack]
  if top then
    top.children = top.children or {}
    table.insert(top.children, node)
  else
    table.insert(self._cfg.nodes, node)
  end
end

function FormBuilder:title(v)        self._cfg.title         = v; return self end
function FormBuilder:description(v)  self._cfg.description   = v; return self end
function FormBuilder:submit_label(v) self._cfg.submit_label  = v; return self end
function FormBuilder:cancel_label(v) self._cfg.cancel_label  = v; return self end

---Set the submit action (required for the form to fire `plugin:form-submit`).
---Two shapes:
---   :submit("save:action")           -- sets submit_action
---   :submit("Save", "save:action")   -- sets submit_label + submit_action
function FormBuilder:submit(label, action)
  if action == nil then
    self._cfg.submit_action = label
  else
    self._cfg.submit_label  = label
    self._cfg.submit_action = action
  end
  return self
end

function FormBuilder:on_submit(action)
  self._cfg.submit_action = action
  return self
end

---Set the cancel action. Either:
---   :cancel("cancel:action")
---   :cancel({ label = "Discard", action = "cancel:action" })
function FormBuilder:cancel(action_or_cfg)
  if type(action_or_cfg) == "table" then
    self._cfg.cancel_label  = action_or_cfg.label
    self._cfg.cancel_action = action_or_cfg.action
  else
    self._cfg.cancel_action = action_or_cfg
  end
  return self
end

function FormBuilder:on_cancel(action)
  self._cfg.cancel_action = action
  return self
end

---Echo state forwarded back to the plugin in the submit ctx.
function FormBuilder:state(t)
  self._cfg.state = t
  return self
end

---Open a new section. Subsequent fields attach to it. Calling :section() again
---auto-closes the previous section (Arbor sections are flat by convention).
---Use :end_section() to explicitly return to the top level.
---@param title_or_cfg string|table  Section title or { title, description, collapsed?, ... }
function FormBuilder:section(title_or_cfg)
  -- Auto-close any prior open section so flat layouts read naturally.
  while #self._stack > 0 do table.remove(self._stack) end
  local node
  if type(title_or_cfg) == "table" then
    node = {}
    for k, v in pairs(title_or_cfg) do node[k] = v end
    node.type     = "section"
    node.children = node.children or {}
  else
    node = { type = "section", title = title_or_cfg, children = {} }
  end
  _push_node(self, node)
  table.insert(self._stack, node)
  return self
end

---Explicitly close the current section so subsequent calls push at top level.
function FormBuilder:end_section()
  if #self._stack == 0 then
    error("form builder: end_section() called outside a section", 2)
  end
  table.remove(self._stack)
  return self
end

local function _make_field(kind)
  return function(self, name_or_cfg, opts)
    local node
    if type(name_or_cfg) == "table" then
      node = {}
      for k, v in pairs(name_or_cfg) do node[k] = v end
      node.type = kind
    else
      node = {}
      if type(opts) == "table" then
        for k, v in pairs(opts) do node[k] = v end
      end
      node.type = kind
      node.name = name_or_cfg
    end
    _push_node(self, node)
    return self
  end
end

FormBuilder.text     = _make_field("text")
FormBuilder.password = _make_field("password")
FormBuilder.textarea = _make_field("textarea")
FormBuilder.number   = _make_field("number")
FormBuilder.select   = _make_field("select")
FormBuilder.checkbox = _make_field("checkbox")
FormBuilder.toggle   = _make_field("toggle")
FormBuilder.radio    = _make_field("radio")
FormBuilder.kv_list  = _make_field("kv_list")

---Insert a horizontal divider.
function FormBuilder:divider()
  _push_node(self, { type = "divider" })
  return self
end

---Insert an inline label.
function FormBuilder:label(text_or_cfg)
  if type(text_or_cfg) == "table" then
    local n = {}
    for k, v in pairs(text_or_cfg) do n[k] = v end
    n.type = "label"
    _push_node(self, n)
  else
    _push_node(self, { type = "label", text = text_or_cfg })
  end
  return self
end

---Insert a paragraph block.
function FormBuilder:paragraph(text)
  _push_node(self, { type = "paragraph", text = text })
  return self
end

---Insert a heading.
function FormBuilder:heading(text)
  _push_node(self, { type = "heading", text = text })
  return self
end

---Insert a button node (typically `{ label, icon, action, variant }`).
function FormBuilder:button(cfg)
  local n = {}
  for k, v in pairs(cfg or {}) do n[k] = v end
  n.type = "button"
  _push_node(self, n)
  return self
end

---Insert a `form_field` node — a vertical labeled wrapper around children.
---Same look as the host's <FormField> widget (label on top, content below,
---optional description / hint / error / leading icon / right-aligned actions).
---Useful around non-field content (button, copy_link) or to enrich a single
---field with affordances the field types don't expose.
---
---Call shapes:
---  · `:form_field({ label = "Name", required = true, children = {...} })`
---  · `:form_field("Name", { children = {...}, hint = "..." })`
---@param cfg_or_label table|string  Config table or label string.
---@param maybe_cfg    table|nil     Optional config when first arg is a label.
function FormBuilder:form_field(cfg_or_label, maybe_cfg)
  local n = { type = "form_field" }
  if type(cfg_or_label) == "table" then
    for k, v in pairs(cfg_or_label) do n[k] = v end
  else
    n.label = tostring(cfg_or_label or "")
    if type(maybe_cfg) == "table" then
      for k, v in pairs(maybe_cfg) do n[k] = v end
    end
  end
  n.children = n.children or {}
  _push_node(self, n)
  return self
end

---Escape hatch — push an arbitrary node table (any `type`, any extra fields).
function FormBuilder:field(node)
  if type(node) ~= "table" then
    error("form builder: :field expects a table node", 2)
  end
  _push_node(self, node)
  return self
end

---Finalise the builder and emit the form via the legacy opener.
function FormBuilder:open()
  local fmeta = getmetatable(arbor.ui.form)
  local opener = fmeta and fmeta.__open_legacy
  if opener then
    return opener(arbor.ui.form, self._cfg)
  end
  -- Should never happen: the bridge below installs __open_legacy.
  error("form builder: legacy opener missing", 2)
end

-- Install __call dispatch on arbor.ui.form: string/nil → builder, table → legacy open.
-- arbor.ui.form.set_options / .set_disabled / .set_value / .replace are
-- table-index calls and therefore unaffected by the metatable change.
do
  local fmeta = getmetatable(arbor.ui.form)
  if fmeta and fmeta.__call then
    -- Preserve the original opener so :open() / legacy table calls still emit.
    fmeta.__open_legacy = fmeta.__call
    fmeta.__call = function(self, arg)
      local t = type(arg)
      if t == "table" then
        -- Legacy: open immediately with the supplied config.
        return fmeta.__open_legacy(self, arg)
      end
      -- Builder mode. `arg` may be a string id (for symmetry with pipelines) or nil.
      local cfg = { nodes = {} }
      if t == "string" then cfg.id = arg end
      return setmetatable({
        _cfg   = cfg,
        _stack = {},
      }, FormBuilder)
    end
  end
end
