-- arbor.core.edit — structured-format editors (JSON / YAML / TOML / XML).
--
-- Every op here is a thin wrapper around arbor.fs.{json,yaml,toml,xml}_set:
-- the Rust side owns the parsers and path walkers; this module adds logging,
-- value promotion and the arbor.pipeline op contract.
--
-- Ops
--   · json_edit { path, jpath, value }
--   · yaml_edit { path, ypath, value }
--   · toml_edit { path, tpath, value }
--   · xml_edit  { path, xpath, value }    -- value is always stringified

local U = require("arbor.core._util")

local M = {}

-- Auto-promote: if `value` parses as JSON treat it as the structured type
-- (number, bool, array, object). Otherwise keep the literal string. Makes
-- it easy to set `42` or `[1,2]` from a form field without a type selector.
local function resolved_value(v)
  if type(v) ~= "string" then return v end
  local ok, decoded = pcall(arbor.json.decode, v)
  if ok and decoded ~= nil then return decoded end
  return v
end

function M.json_edit(params, ctx)
  local path = U.abs_path(params.path or "", ctx)
  local jpath = params.jpath or ""
  if jpath == "" then
    return { exit_code = 1, stderr = "[json_edit] jpath is required" }
  end
  local lines, add = U.new_log("json_edit")
  U.log_entry(add, "file ", path)
  U.log_entry(add, "jpath", jpath)
  local ok, err = arbor.fs.json_set{ path = path, jpath = jpath, value = resolved_value(params.value) }
  if not ok then return U.fail(lines, tostring(err)) end
  add("wrote")
  return U.finish(lines)
end

function M.yaml_edit(params, ctx)
  local path = U.abs_path(params.path or "", ctx)
  local ypath = params.ypath or ""
  if ypath == "" then
    return { exit_code = 1, stderr = "[yaml_edit] ypath is required" }
  end
  local lines, add = U.new_log("yaml_edit")
  U.log_entry(add, "file ", path)
  U.log_entry(add, "ypath", ypath)
  local ok, err = arbor.fs.yaml_set{ path = path, ypath = ypath, value = resolved_value(params.value) }
  if not ok then return U.fail(lines, tostring(err)) end
  add("wrote")
  return U.finish(lines)
end

function M.toml_edit(params, ctx)
  local path = U.abs_path(params.path or "", ctx)
  local tpath = params.tpath or ""
  if tpath == "" then
    return { exit_code = 1, stderr = "[toml_edit] tpath is required" }
  end
  local lines, add = U.new_log("toml_edit")
  U.log_entry(add, "file ", path)
  U.log_entry(add, "tpath", tpath)
  local ok, err = arbor.fs.toml_set{ path = path, tpath = tpath, value = resolved_value(params.value) }
  if not ok then return U.fail(lines, tostring(err)) end
  add("wrote")
  return U.finish(lines)
end

function M.xml_edit(params, ctx)
  local path = U.abs_path(params.path or "", ctx)
  local xpath = params.xpath or ""
  if xpath == "" then
    return { exit_code = 1, stderr = "[xml_edit] xpath is required" }
  end
  local lines, add = U.new_log("xml_edit")
  U.log_entry(add, "file ", path)
  U.log_entry(add, "xpath", xpath)
  local ok, err = arbor.fs.xml_set{ path = path, xpath = xpath, value = tostring(params.value or "") }
  if not ok then return U.fail(lines, tostring(err)) end
  add("wrote")
  return U.finish(lines)
end

function M.register()
  arbor.pipeline.register_op("json_edit", M.json_edit)
  arbor.pipeline.register_op("yaml_edit", M.yaml_edit)
  arbor.pipeline.register_op("toml_edit", M.toml_edit)
  arbor.pipeline.register_op("xml_edit",  M.xml_edit)
end

return M
