-- Run from CodexLive: lua skill-lsp-rs/scripts/lua_color_smoke.lua [capture|reject|missing-json]
-- All Lite-XL services are stubbed. No installed configuration or Finder files
-- are accessed, and the server process is not started by this harness.
package.path, package.cpath = "", ""
local mode = arg[1] or "success"
local plugin_path = arg[2] or "skill_lsp/skill_colors.lua"
local config_paths = arg[3] and { arg[3] } or {
  "skill_lsp/skill_lsp_config.lua", "skill-lsp-rs/editors/skill_lsp_config.lua"
}
USERDIR, PLATFORM = "/synthetic-user", "Linux"
local logs, writes, drawn = {}, {}, {}
local incoming, thread, now, token_request = "", nil, 0, nil
local auto_response = true
local token_data = { 0, 0, 11, 3, 0 }
local doc = { filename = "synthetic.il", abs_filename = "/synthetic.il",
  lines = { "dbSynthetic(nil)\n" } }
local core = { docs = { doc }, project = { path = "/synthetic" } }
function core.log(message) logs[#logs + 1] = message end
function core.add_thread(fn) thread = coroutine.create(fn) end
local common = {}
function common.merge(defaults, overrides)
  for k, v in pairs(overrides) do defaults[k] = v end
  return defaults
end
function common.color(hex)
  return tonumber(hex:sub(2, 3), 16), tonumber(hex:sub(4, 5), 16),
    tonumber(hex:sub(6, 7), 16), 255
end
local style = { syntax = { normal = {} }, syntax_fonts = {} }
local DocView = { update = function() end, draw_line_text = function() end }
local function offset(self, line, col)
  local result = col - 1
  for i = 1, line - 1 do result = result + #self.lines[i] end
  return result
end
local function set_text(self, text)
  self.lines = {}
  if text:sub(-1) ~= "\n" then text = text .. "\n" end
  for line in text:gmatch("[^\n]*\n") do self.lines[#self.lines + 1] = line end
end
local Doc = { save = function() end }
function Doc:raw_insert(line, col, inserted)
  local text, at = table.concat(self.lines), offset(self, line, col)
  set_text(self, text:sub(1, at) .. inserted .. text:sub(at + 1))
end
function Doc:raw_remove(line1, col1, line2, col2)
  local text = table.concat(self.lines)
  local first, last = offset(self, line1, col1), offset(self, line2, col2)
  set_text(self, text:sub(1, first) .. text:sub(last + 1))
end
local function encode(value)
  if type(value) == "string" then
    return '"' .. value:gsub('[%z\1-\31\\"]', function(c)
      return string.format('\\u%04x', c:byte())
    end) .. '"'
  elseif type(value) == "number" or type(value) == "boolean" then
    return tostring(value)
  elseif type(value) == "table" then
    local parts = {}
    if value[1] ~= nil then
      for _, v in ipairs(value) do parts[#parts + 1] = encode(v) end
      return "[" .. table.concat(parts, ",") .. "]"
    end
    for k, v in pairs(value) do parts[#parts + 1] = encode(k) .. ":" .. encode(v) end
    return "{" .. table.concat(parts, ",") .. "}"
  end
  return "null"
end
local function frame(body)
  return string.format("Content-Length: %d\r\n\r\n%s", #body, body)
end
local json = { encode = encode }
function json.decode(body)
  if body == "initialize-result" then
    if mode == "reject" then
      return { id = 1, error = { code = -32602, message = "synthetic initialization error" } }
    end
    return { id = 1, result = { capabilities = { semanticTokensProvider = {
      legend = { tokenTypes = { "keyword", "keyword1", "keyword2", "keyword3",
        "keyword4", "keyword5", "keyword6", "keyword7", "keyword8", "keyword9",
        "keyword10", "function", "variable", "comment", "string", "number",
        "operator", "kwarg", "literal" } }
    } } } }
  elseif body == "tokens-result" then
    return { id = token_request, result = { data = token_data } }
  end
  error("Unexpected test response")
end
local modules = {
  ["core"] = core, ["core.style"] = style, ["core.common"] = common,
  ["core.config"] = { plugins = { skill_colors = { server_command = "synthetic-server", fnd_dir = "" } } },
  ["core.docview"] = DocView, ["core.doc"] = Doc,
  ["core.command"] = { add = function() end },
  ["core.keymap"] = { add = function() end },
}
for name, value in pairs(modules) do package.preload[name] = function() return value end end
package.preload["plugins.lsp.json"] = function()
  if mode == "missing-json" then error("synthetic missing JSON dependency") end
  return json
end
system = { get_time = function() return now end }
process = { REDIRECT_DISCARD = 0 }
function process.start()
  return {
    running = function() return true end,
    write = function(_, message)
      writes[#writes + 1] = message
      if message:find('"textDocument/semanticTokens/full"', 1, true) then
        token_request = tonumber(message:match('"id":(%d+)'))
        if auto_response then incoming = incoming .. frame("tokens-result") end
      end
      return #message
    end,
    read_stdout = function()
      local data = incoming
      incoming = ""
      return data
    end,
  }
end
renderer = { draw_text = function(_, text, x, _, color)
  drawn[#drawn + 1] = { text = text, color = color }
  return x + #text
end }
assert(loadfile(plugin_path))()
if mode == "missing-json" then
  assert(#writes == 0 and logs[1]:find("Cannot load plugins.lsp.json", 1, true))
  print("PASS: missing JSON dependency is reported without launching a server")
  return
end
if mode == "capture" then io.write(writes[1]); return end
local function step()
  local ok, err = coroutine.resume(thread)
  assert(ok, err)
end
incoming = frame("initialize-result")
step() -- initial coroutine yield
step() -- consume initialize response
if mode == "reject" then
  assert(#writes == 1)
  assert(logs[1]:find("Server initialization failed: synthetic initialization error", 1, true))
  print("PASS: initialization errors are visible without debug mode")
  return
end
assert(token_request, "Initial token request must not wait for the edit timer")
now = 1
step() -- send changed text and token request
step() -- receive tokens and request redraw
assert(core.redraw == true and token_request)
doc.highlighter = {
  get_line = function() return { tokens = { "symbol", doc.lines[1] } } end,
  each_token = function()
    local done = false
    return function()
      if not done then done = true; return 1, "symbol", doc.lines[1] end
    end
  end,
}
local view = setmetatable({ doc = doc, position = { x = 0 }, size = { x = 1000 },
  get_font = function() return {} end,
  get_line_text_y_offset = function() return 0 end,
  get_line_height = function() return 16 end,
}, { __index = DocView })
view:draw_line_text(1, 0, 0)
assert(drawn[1].text == "dbSynthetic")
assert(drawn[1].color[1] == 0xD4 and drawn[1].color[2] == 0xA0 and drawn[1].color[3] == 0x60)
print("PASS: handshake, token decoding, repaint, and keyword3 drawing")

local function count(method)
  local n = 0
  for _, message in ipairs(writes) do
    if message:find('"' .. method .. '"', 1, true) then n = n + 1 end
  end
  return n
end
local function replace_doc(text)
  Doc.raw_remove(doc, 1, 1, #doc.lines, #doc.lines[#doc.lines])
  Doc.raw_insert(doc, 1, 1, text:sub(1, -2))
end
-- Edits share the first 50 ms deadline rather than continually resetting it.
now = 2
replace_doc("dbSynthetic(nil)\n")
local before = count("textDocument/semanticTokens/full")
now = 2.03
replace_doc("dbSynthetic(nil)\n")
step()
assert(count("textDocument/semanticTokens/full") == before)
auto_response = false
now = 2.06
step()
assert(count("textDocument/semanticTokens/full") == before + 1)
now = 2.5
step()
assert(count("textDocument/semanticTokens/full") == before + 1,
  "Do not send duplicate requests while a response is pending")
-- A reply for a document version before the latest edit must be discarded.
replace_doc("abcdefghijklmno\n")
core.redraw = false
incoming = frame("tokens-result")
now = 2.51
step()
assert(not core.redraw, "Stale tokens must not repaint the document")
token_data = { 0, 0, 3, 3, 0, 0, 3, 2, 3, 0, 0, 4, 4, 1, 0 }
auto_response = true
now = 2.56
step()
step()
assert(core.redraw)
assert(count("textDocument/didChange") == 2, "Each update uses one full-text change")
local versions = {}
for _, message in ipairs(writes) do
  if message:find('"textDocument/didChange"', 1, true) then
    versions[#versions + 1] = tonumber(message:match('"version":(%d+)'))
  end
end
assert(versions[1] == 2 and versions[2] == 3)
print("PASS: immediate initial request, bounded edit delay, one in-flight request, stale response rejection")

-- Semantic spans may cross multiple syntax tokens, or leave uncolored gaps.
local syntax_tokens = { "symbol", "ab", "string", "cdefghi", "symbol", "jklmno\n" }
style.syntax.string = { 1, 2, 3 }
doc.highlighter.get_line = function() return { tokens = syntax_tokens } end
doc.highlighter.each_token = function()
  local i = -1
  return function()
    i = i + 2
    if syntax_tokens[i] then return i, syntax_tokens[i], syntax_tokens[i + 1] end
  end
end
drawn = {}
view:draw_line_text(1, 0, 0)
local rendered, colors = "", {}
for _, piece in ipairs(drawn) do
  rendered = rendered .. piece.text
  for _ = 1, #piece.text do colors[#colors + 1] = piece.color end
end
assert(rendered == "abcdefghijklmno")
for i = 1, #rendered do
  if i <= 5 then assert(colors[i][1] == 0xD4) -- merged adjacent keyword3 spans
  elseif i <= 7 then assert(colors[i] == style.lsp_semantic_tokens.string) -- syntax gap
  elseif i <= 11 then assert(colors[i][1] == 0x4F) -- keyword1 across syntax boundary
  else assert(colors[i] == style.syntax.normal) end
end
-- Empty semantic tokens still render all text with normal syntax colors.
replace_doc("abcdefghijklmno\n")
token_data = {}
now = 3
step()
step()
drawn = {}
view:draw_line_text(1, 0, 0)
assert(drawn[1].color == style.syntax.normal and drawn[2].color == style.lsp_semantic_tokens.string)
print("PASS: span intersections, adjacent spans, syntax gaps, line endings, and empty tokens")

-- Use one syntax token per line to inspect semantic colors independently.
doc.highlighter.get_line = function(_, line)
  return { tokens = { "symbol", doc.lines[line] } }
end
doc.highlighter.each_token = function(_, line)
  local done = false
  return function()
    if not done then done = true; return 1, "symbol", doc.lines[line] end
  end
end
local function hydrate(text, data)
  replace_doc(text)
  token_data = data
  now = now + 1
  step()
  step()
end
local function painted(line)
  drawn = {}
  view:draw_line_text(line, 0, 0)
  local text, colors = "", {}
  for _, piece in ipairs(drawn) do
    text = text .. piece.text
    for _ = 1, #piece.text do colors[#colors + 1] = piece.color[1] or 0 end
  end
  assert(text == doc.lines[line]:sub(1, -2))
  return colors
end
local function expect(line, expected)
  local colors = painted(line)
  assert(#colors == #expected)
  for i, value in ipairs(expected) do
    assert(colors[i] == value, string.format("line %d byte %d: %s ~= %s", line, i, colors[i], value))
  end
end
local green, blue, lightblue = 0xD4, 0x7A, 0x4F
hydrate("alpha\nbeta\ngamma\n", { 0, 0, 5, 3, 0, 1, 0, 4, 1, 0, 1, 0, 5, 2, 0 })
local requests_before_edit = count("textDocument/semanticTokens/full")
Doc.raw_remove(doc, 1, 6, 2, 1) -- delete newline; no server step
expect(1, { green, green, green, green, green, lightblue, lightblue, lightblue, lightblue })
expect(2, { blue, blue, blue, blue, blue })
Doc.raw_insert(doc, 1, 6, "\n") -- inverse edit / undo-style split
expect(1, { green, green, green, green, green })
expect(2, { lightblue, lightblue, lightblue, lightblue })
Doc.raw_insert(doc, 1, 1, "\n\n")
expect(1, {})
expect(2, {})
expect(3, { green, green, green, green, green })
expect(5, { blue, blue, blue, blue, blue })
Doc.raw_remove(doc, 1, 1, 3, 1) -- remove both blank lines
Doc.raw_insert(doc, 1, 3, "\n") -- split inside an existing colored token
expect(1, { green, green })
expect(2, { green, green, green })
expect(3, { lightblue, lightblue, lightblue, lightblue })
Doc.raw_remove(doc, 1, 3, 2, 1)
Doc.raw_remove(doc, 1, 3, 3, 3) -- remove a multi-line selection
expect(1, { green, green, blue, blue, blue })
assert(count("textDocument/semanticTokens/full") == requests_before_edit,
  "Cached colors must move before any server request or reply")

hydrate("alpha beta\n", { 0, 0, 5, 3, 0, 0, 6, 4, 1, 0 })
Doc.raw_remove(doc, 1, 3, 1, 5)
expect(1, { green, green, green, 0, lightblue, lightblue, lightblue, lightblue })
Doc.raw_insert(doc, 1, 2, "ZZ")
expect(1, { green, 0, 0, green, green, 0, lightblue, lightblue, lightblue, lightblue })
hydrate('"😀" alpha\nbeta\n', { 0, 4, 5, 3, 0, 1, 0, 4, 1, 0 })
expect(1, { 0, 0, 0, 0, 0, 0, 0, green, green, green, green, green })
Doc.raw_remove(doc, 1, #doc.lines[1], 2, 1)
expect(1, { 0, 0, 0, 0, 0, 0, 0, green, green, green, green, green,
  lightblue, lightblue, lightblue, lightblue })
print("PASS: immediate newline joins/splits, blank lines, selection deletion, inline edits, and Unicode columns")
style.syntax.literal = { 0xDE, 0xAD, 0xBE }
hydrate("caadar(lst)\n", { 0, 0, 6, 2, 0 })
expect(1, { 0x7A, 0x7A, 0x7A, 0x7A, 0x7A, 0x7A, 0, 0, 0, 0, 0 })
style.syntax.literal = { 0xFA, 0xCE, 0x00 }
hydrate("caadar(lst)\n", { 0, 0, 6, 2, 0 })
expect(1, { 0x7A, 0x7A, 0x7A, 0x7A, 0x7A, 0x7A, 0, 0, 0, 0, 0 })
hydrate("@key @rest @optional\n", { 0, 0, 4, 7, 0, 0, 5, 5, 7, 0, 0, 6, 9, 7, 0 })
expect(1, { 0xE0, 0xE0, 0xE0, 0xE0, 0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0,
  0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0 })
print("PASS: selectors use keyword2 blue and @ markers use keyword7 red")

-- Lock every user-specified palette entry, including syntax fallback before
-- an unseen SKILL document has received any semantic tokens.
local expected_palette = {
  keyword1 = "#4FC8C0", keyword2 = "#7AB8F0", keyword3 = "#D4A060",
  keyword4 = "#9B7FE8", keyword5 = "#C87FD0", keyword6 = "#E26F8F",
  keyword7 = "#E06060", keyword8 = "#60B8D0", keyword9 = "#A0C040",
  keyword10 = "#6080A8", keyword = "#4FC8C0", kwarg = "#A8D8FF",
  ["function"] = "#60D0E8", variable = "#A0C8A0", string = "#78C878",
  number = "#F1EE8E", comment = "#4A5568", operator = "#C8E050",
}
local palette_tokens, palette_names, palette_text = {}, {}, ""
for name, hex in pairs(expected_palette) do
  local expected = { common.color(hex) }
  local actual = style.lsp_semantic_tokens[name]
  for i = 1, 4 do assert(actual[i] == expected[i], name) end
  palette_tokens[#palette_tokens + 1] = name
  palette_tokens[#palette_tokens + 1] = name
  palette_names[#palette_names + 1] = name
  palette_text = palette_text .. name
end
palette_tokens[#palette_tokens] = palette_tokens[#palette_tokens] .. "\n"
local unseen = { filename = "palette.il", lines = { palette_text .. "\n" }, highlighter = {
  get_line = function() return { tokens = palette_tokens } end,
  each_token = function()
    local i = -1
    return function()
      i = i + 2
      if palette_tokens[i] then return i, palette_tokens[i], palette_tokens[i + 1] end
    end
  end,
} }
view.doc = unseen
drawn = {}
view:draw_line_text(1, 0, 0)
assert(#drawn == #palette_names)
for i, name in ipairs(palette_names) do
  assert(drawn[i].color == style.lsp_semantic_tokens[name], name)
end
unseen.filename = "palette.lua"
drawn = {}
view:draw_line_text(1, 0, 0)
assert(#drawn == 0, "Other languages must use their original renderer")
assert(style.syntax.string[1] == 1, "Do not overwrite the editor's global syntax theme")

local registration
package.preload["plugins.lsp"] = function()
  return { add_server = function(server) registration = server end }
end
package.preload["core.syntax"] = function() return { add = function() end } end
for _, path in ipairs(config_paths) do
  assert(loadfile(path))()
  for name in pairs(expected_palette) do
    assert(registration.get_color_for_token(name) == style.lsp_semantic_tokens[name], name)
  end
  assert(registration.get_color_for_token("literal") == style.syntax.literal)
end
print("PASS: all 18 palette colors, initial syntax rendering, both LSP configurations, and other-language isolation")
