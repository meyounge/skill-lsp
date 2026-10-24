# SKILL LSP in Rust

A native Rust rewrite of the supplied Python SKILL language server, using
[`tower-lsp` 0.20.0](https://docs.rs/tower-lsp/0.20.0/tower_lsp/).
The default request behavior follows the supplied Python v3 source, including
its existing quirks, with computed car/cdr selectors and negotiated protocol
positions for VS Code and Lite-XL.
No Cadence documentation is included; the Finder fixtures
in `examples/SKILL` contain invented functions and descriptions.

## VS Code and public distribution

The VS Code client is in [`editors/vscode`](editors/vscode/README.md). Build the
Rust server, package/install the VSIX, and configure `skillLsp.serverPath`.
Optional `skillLsp.fndDir` or `skillLsp.database` settings point to each user's
local authorized library. Native semantic highlighting includes the project's
palette, car/cdr hover diagrams, and `@` marker colors. No Lua plugin is needed
in VS Code.

The project is MIT licensed. See [PUBLISHING.md](PUBLISHING.md) for a clean
allowlisted GitHub source export, CI artifacts, and manual VSIX publishing.
Publish this standalone project, not the parent workspace containing local
documentation and Python build outputs.

## Build and run

Use a current stable Rust toolchain. The local build was tested with Rust 1.99.1.
The executable needs no Python runtime. Python is only used by test scripts.

```sh
cargo test --all-targets --locked
cargo build --release --locked
./target/release/skill-lsp --fnd-dir /absolute/path/to/SKILL
```

On Windows the executable is `target/release/skill-lsp.exe`.
To try only the included invented documentation:

```sh
./target/release/skill-lsp --fnd-dir examples/SKILL
```

Stdio is the default transport. TCP uses the original flags and default address:

```sh
./target/release/skill-lsp --fnd-dir examples/SKILL --tcp --host 127.0.0.1 --port 2087
```

TCP accepts another connection after a client disconnects. An LSP `exit`
notification ends the process. TCP operation still needs validation outside
this workspace, whose sandbox denies loopback socket creation.

Documentation lookup order is `--database`, `--fnd-dir`/`SKILL_FND_DIR`, embedded
data, the Python server's `$HOME/.config/lite-xl/plugins/skill_lsp/SKILL`,
`./SKILL`, then `SKILL` beside the executable. Without documentation, syntax
keywords and current-document symbols still work. An explicit missing Finder
directory warns and starts with an empty API library, as Python does.

The Python executable and its source are left untouched. To use Rust in Lite-XL,
adjust `SKILL_LSP_BIN` and `FND_DIR` in `editors/skill_lsp_config.lua`. The existing
18 semantic token indices and keyword1–10 color mapping are retained. A new
`literal` token remains appended for compatibility with earlier builds;
computed car/cdr selectors now use the existing `keyword2` token.
`editors/neovim.lua` contains an additional configuration example.

The portable `editors/skill_colors.lua` standalone color plugin requests
initial tokens immediately, batches edits for 50 ms by default, and draws cached
color spans without rebuilding per-character color tables. It allows one token
request per document at a time and rejects responses made stale by edits.
Cached spans shift immediately with inserted/deleted text, including newline
splits and joins, so surviving text retains its colors before a server reply.
Unicode server columns are converted to UTF-8 bytes for Lite-XL drawing and edits.
Copy that Lua file to your Lite-XL configuration and restart; no binary rebuild is
required for these drawing changes. An existing `refresh_delay` configuration
override still takes precedence. Its synthetic regression harness can be run
from the project root with
`lua scripts/lua_color_smoke.lua success editors/skill_colors.lua editors/skill_lsp_config.lua`.

The `COLORS` table in that plugin is the authoritative user palette. It supplies
both semantic colors and syntax fallback colors for SKILL files, including
before the initial server response, and is shared with the SKILL LSP color
callbacks through `style.lsp_semantic_tokens`. The global syntax theme for other
languages is not modified. Computed selectors use `keyword2` (`#7AB8F0` blue);
`@name` lambda-list markers use `keyword7` (`#E06060`), including `@key`, `@rest`,
and `@optional`. The whole marker, including `@`, is colored. Markers inside
strings and line comments do not receive this semantic classification.

## Compatibility

Completion, hover, definitions, document symbols, references, diagnostics,
formatting, signature help, and full semantic tokens are implemented.
The previous Rust draft deliberately improved some of these features. The
runtime now preserves the original Python behavior instead:

- Completion includes documentation immediately, has no default 300-item limit,
  and retains separate API and user-symbol entries with the same name. It is
  available in strings and comments, just as in Python. Keyword order is stable
  in Rust; Python's set iteration makes its keyword order vary between launches.
- Hover uses the same aligned parameter columns, return text, and 72-character
  description wrapping, with the same precedence for APIs and syntax keywords.
- Definitions and document symbols use the Python regex matches and their
  locations, even where those locations identify the declaration keyword.
  Definitions in comments and strings are recognized, as in the source.
- References include strings, comments, and declarations, even when the client
  sends `includeDeclaration: false`. Punctuation follows Python word boundaries.
- Signature help searches backward on the current line for a parenthesized
  function name and counts commas. It does not add the draft's multiline or
  extra call-syntax support.
- Diagnostics check parentheses using the Python per-line string/comment rules.
  Formatting uses spaces, normalizes line endings, and appends a newline, even
  if `insertSpaces` is false. These are retained Python behaviors.
- Semantic tokens preserve the original type precedence, per-line strings,
  numeric scanning, keyword arguments, and `t`/`nil` handling. Comment tokens
  are not added.
- Protocol positions default to UTF-16 for VS Code and other standard clients.
  The updated Lite-XL plugin explicitly negotiates UTF-32 to retain the original
  code-point semantics, then converts its drawing positions to UTF-8 bytes.
- The last full-text change wins, including a decreasing version number;
  closing a document removes it without adding a diagnostics-clear notification.

Full document sync is explicitly advertised so editors send the complete text
that the original Python handlers expect. Framework initialization metadata and
invalid-request responses are supplied by tower-lsp rather than pygls. This is
tested handler compatibility, not a claim that every framework message is
byte-for-byte identical. Completion order is the only normalization applied in
the response comparisons.

## Computed car/cdr selectors

Lowercase names matching `c[ad]+r` receive computed hover text and `keyword2`
highlighting, even when Finder data or a user declaration has the same name.
The Lua plugin uses the user's `keyword2` blue rather than the literal color.
Install both the updated binary and `editors/skill_colors.lua` for this
feature. The updated `skill_lsp_config.lua` also maps this token for clients
that handle semantic coloring through the LSP configuration.

The operations apply from right to left. Examples:

| Name | Expansion | One-based path | Input shape |
| --- | --- | --- | --- |
| `car` | `car(lst)` | `lst[1]` | `(x -)` |
| `cadr` | `car(cdr(lst))` | `lst[2]` | `(- x -)` |
| `caar` | `car(car(lst))` | `lst[1][1]` | `((x -) -)` |
| `cadar` | `car(cdr(car(lst)))` | `lst[1][2]` | `((- x -) -)` |
| `caadar` | `car(car(cdr(car(lst))))` | `lst[1][2][1]` | `((- (x -) -) -)` |
| `cddr` | `cdr(cdr(lst))` | `lst[3..]` | `(- - . x)` |

`x` marks the selected value. A dash marks unknown data or an unconstrained
remaining tail, not a known element count. `. x` identifies a selected tail
rather than one element. These diagrams describe the required traversal shape;
they do not evaluate a variable or infer its actual runtime contents. Generated
selectors do not navigate to a Finder/user definition. Existing Finder
completion entries for these names also get the computed documentation; the
server does not enumerate an infinite set of composed names for completion.
Ordinary identifiers such as `carpet`, `cadrExtra`, and `Cadr` remain unchanged.
Recognition and diagram rendering support arbitrary composition depth using
linear, iterative processing.

Finder parsing follows the provided `fnd_parser.py`: quoted top-level records,
literal backslash escapes, type prefixes, optional flags, keyword/type merging,
return suppression, two-pass core-prefix discovery, categories, and duplicate
precedence. Incomplete records are ignored rather than becoming fatal errors.
Only immediate `Core_SKILL` children get core-file treatment. Core-prefix
discovery scans quoted descriptions too, matching the Python quick scan.
Each runtime loads one library; Python's accidental prefix leakage between
repeated in-process library loads is not emulated.

## Performance and validation

Finder files are indexed once at startup. Completion uses a case-folded prefix
index. Hover text is prepared once. Document analysis, reference locations, and
semantic tokens are cached when text changes and shared by subsequent requests.
This moves work from repeated read requests to each full-text update.

```sh
python3 scripts/smoke.py
python3 scripts/parity.py --benchmark
python3 scripts/parser_parity.py
python3 scripts/car_cdr_smoke.py
python3 scripts/encoding_smoke.py
cargo run --release --example perf
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

`smoke.py` tests the compiled Rust server over stdio, JSON database loading,
shutdown with the input pipe still open, and more than 350 completion items.
`parity.py` exercises every cursor column in hand-written and seeded generated
documents and compares all advertised request handlers against the original
Python source. `parser_parity.py` creates additional invented Finder files,
compares metadata to the actual Python parser, and checks generated hover and
completion documentation. They default to the original sources in `../skill_lsp`.

The original Nuitka binaries loaded the synthetic library but did not respond to
initialization over pipes here. Therefore the default comparator uses
`scripts/python_reference.py`, a standard-library adapter that imports the
unaltered Python source and calls its handlers. The adapter supplies protocol
value objects, decorators, and framing; it does not replace the Python parsing
or handler logic. It does not validate pygls initialization/transport behavior.
Use `parity.py --python-binary /path/to/skill_lsp` to compare an operational binary.

The final local measurement on 3,000 synthetic lines (30 warmed requests each)
was approximately 2.7x faster hover, 127x faster completion, and 15x faster semantic
tokens than the source-handler adapter. These are request round-trip measurements
on a six-function synthetic library, not comparisons with Nuitka or claims about
a real Cadence library. See `VALIDATION.md` for measured values and limitations.

## Optional data packaging

The previous draft's JSON and embedding options remain available:

```sh
./target/release/skill-lsp index /path/to/SKILL skill.api.json
./target/release/skill-lsp --database skill.api.json
SKILL_EMBED_FND_DIR=/path/to/SKILL cargo build --release
```

The JSON loader accepts both the Rust array export and Python's object export.
Embedding includes the supplied documentation in the executable; clearing the
build-time variable and rebuilding removes it. Synthetic data is sufficient
for development; actual-library verification remains outstanding.
`--max-completions N` is an optional Rust-only response cap and is disabled by
default.

`src/fnd.rs` handles Finder data; `src/compat.rs` is the runtime document engine;
`src/server.rs` provides the tower-lsp handlers; `src/main.rs` handles loading and
transport. The earlier `src/document.rs` analyzer is retained as library code and
supplies token constants, but its enhanced parsing rules are not used by the
runtime server. Existing unit tests for that analyzer remain alongside the
functional compatibility scripts.
