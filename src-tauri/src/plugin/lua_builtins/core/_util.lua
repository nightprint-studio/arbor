-- arbor.core._util — internal helpers shared by the arbor.core.* op modules.
--
-- Not part of the public API: the leading underscore signals "don't require
-- this directly from plugin code". Kept stable for the sibling modules.

local M = {}

local IS_WIN = arbor.meta.os() == "windows"

-- ---------------------------------------------------------------------------
-- Path resolution
-- ---------------------------------------------------------------------------

function M.is_abs(p)
  if not p or p == "" then return false end
  if IS_WIN then
    return p:match("^[A-Za-z]:[/\\]") ~= nil
           or p:sub(1,2) == "\\\\"
           or p:sub(1,1) == "/"
  end
  return p:sub(1,1) == "/"
end

-- Resolve a (possibly relative) path against the op ctx's cwd.
function M.abs_path(p, ctx)
  if M.is_abs(p) then return p end
  local cwd = (ctx and ctx.cwd) or ""
  if cwd == "" then return p end
  local sep = IS_WIN and "\\" or "/"
  if cwd:sub(-1) == "/" or cwd:sub(-1) == "\\" then
    return cwd .. p
  end
  return cwd .. sep .. p
end

-- ---------------------------------------------------------------------------
-- Structured log accumulator — every handler builds its step output this way
-- so the pipeline panel renders a consistent `[op_name] key = value` trail.
-- ---------------------------------------------------------------------------

function M.new_log(kind)
  local lines = {}
  local function add(msg)
    lines[#lines+1] = "[" .. kind .. "] " .. msg
  end
  return lines, add
end

function M.log_entry(add, key, value)
  add(key .. " = " .. tostring(value))
end

function M.finish(lines, exit_code)
  return { exit_code = exit_code or 0, stdout = table.concat(lines, "\n") }
end

function M.fail(lines, msg)
  lines[#lines+1] = "[FAIL] " .. msg
  return { exit_code = 1, stdout = table.concat(lines, "\n"), stderr = msg }
end

return M
