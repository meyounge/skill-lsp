# Local validation record

## October 24: VS Code and public-source preparation

Added a MIT-licensed VS Code client using vscode-languageclient 9.0.1, the SKILL
grammar, custom semantic types, the user's palette, local API settings, restart
and output commands, and workspace-trust gating. Five headless Node tests pass
for launch settings, lifecycle/restart, error handling, grammar, and palette.
The VSIX was packaged with production dependencies only, inspected for contents,
and accepted by VS Code's CLI in an isolated profile/extensions directory under
`target`. The normal installed editor profile was not accessed or modified.
Actual VS Code UI interaction and Marketplace publishing are not yet verified.

The server now defaults to UTF-16 and negotiates UTF-32 with the updated Lite-XL
plugin. Synthetic wire tests verify positions after emoji for hover, definitions,
references, diagnostics, and semantic token lengths/offsets in both modes.
Existing 22 Rust tests, Clippy, stdio checks, selector checks, 8,144 baseline
source comparisons and 8,324 larger-library comparisons still pass. The larger
synthetic metadata comparison covers 90 entries. No private Finder data was used.

The extension's locked dependencies report zero npm audit vulnerabilities after
applying the patched MSAL override for the development-only packager. The VSIX
does not include that authentication tooling. Package/release scripts do not
publish or authenticate automatically. The source export uses a 47-file explicit
allowlist; only the two named invented Finder fixtures may enter the archive.

## October 24: updated selector and lambda-list colors

Car/cdr selectors now emit `keyword2` (blue), overriding Finder categories as
before. The whole `@name` marker emits `keyword7`, including `@key`, `@rest`, and
`@optional`. Strings and line comments retain their previous treatment.
The extra `literal` legend entry is retained so token indices stay compatible.
The Lua syntax configurations also recognize `@name` for initial fallback
coloring. Synthetic LSP tests cover all three example markers, their full token
lengths, and car/cdr hover precedence; Lua regression tests verify both palette
colors and the existing edit/remapping behavior.

## October 24: computed car/cdr selectors

The release binary was rebuilt with embedding disabled. The new selector hover,
diagram, one-based path, and `literal` highlighting override are verified using
invented JSON API entries, including a conflicting Finder-style entry and a user
declaration. `car_cdr_smoke.py` tests actual stdio responses; the Lua harness
checks that selectors use the same current palette entry as `t`/`nil`.

Passed: all 22 Rust tests, formatting, Clippy with warnings denied, release build,
existing stdio smoke tests, 8,144 unchanged baseline Python response comparisons,
the new selector smoke suite, Lua render/edit regression tests, and Lua config
syntax checks. Composition depth of 10,000 operations is covered without
recursive rendering. The original 18 semantic token indices remain stable;
`literal` is appended as index 18. Baseline comparison fixtures contain no new
selector cases; their intentional behavior changes are tested separately.

Only workspace files and synthetic documentation were used. Test scratch files
are placed under the workspace's `target` directory. Real Finder documents and
installed editor configuration were not inspected.

Validated on October 21, 2026, on Linux with Rust/Cargo 1.99.1 and Python 3.14.
All documentation inputs were invented synthetic Finder records. No original
Cadence Finder documentation was read or incorporated.

## Passed

- `cargo test --all-targets --offline --locked`: 19 tests, including five
  regressions for the runtime's Python-compatible behavior. The earlier
  experimental analyzer's tests are retained and pass separately.
- `cargo build --release --offline --locked`: native release executable built
  at `target/release/skill-lsp`, approximately 3 MB.
- `cargo clippy --all-targets --offline --locked -- -D warnings`.
- `cargo fmt --all -- --check`.
- Bytecode compilation of the four Python test scripts.
- `python3 scripts/smoke.py`: all advertised Rust stdio handlers, shutdown/exit
  without closing stdin, full-text updates, Unicode code-point behavior, JSON
  loading, six-function fixture metadata, and an uncapped response containing
  more than 350 completions.
- `python3 scripts/parity.py --benchmark`: 8,144 original-Python/Rust response
  comparisons across hand-written and seeded generated source documents.
  Checks hover, completion, definitions, references, signatures at every cursor
  column, plus whole-document symbols, formatting, tokens, and diagnostics.
- `python3 scripts/parser_parity.py`: 90 invented entries across 11 temporary
  Finder files match the actual original Python parser metadata, followed by
  8,324 response comparisons using the larger library. Cases include type
  combinations, optional flags, constraints, underscore-containing keyword
  arguments, quote escaping, malformed trailing input, duplicate precedence,
  immediate versus nested core files, description-derived core prefixes,
  multiline descriptions, Unicode, long words, and hyphen wrapping.
- Build with `SKILL_EMBED_FND_DIR` set only to `examples/SKILL`, followed by
  running the stdio smoke scenarios without a runtime document argument.
  Rebuilt afterward with the variable unset: the final binary has no embedded
  documentation.
- Release microbenchmark exercises the actual compatibility document engine
  and cached semantic-token access.

The source adapter's completion lists are sorted for comparison because the
Python set of syntax keywords has process-dependent iteration order. No other
response values are normalized. Framework initialization metadata is checked
separately rather than claimed identical.

## Performance measurement

`python3 scripts/parity.py --benchmark`, final run: a 3,000-line invented
document, six API functions, 30 warmed requests per operation, median elapsed
time including framing, pipe I/O, and client JSON handling:

| Request | Original Python handlers | Rust release | Observed ratio |
| --- | ---: | ---: | ---: |
| Hover | 0.059 ms | 0.022 ms | 2.71x |
| Completion | 4.516 ms | 0.035 ms | 127.45x |
| Full semantic tokens | 10.764 ms | 0.710 ms | 15.16x |

These measurements compare the supplied Python source handlers through the
standard-library reference adapter against the compiled Rust server. They do
not compare Rust with Nuitka. Caching, indexing, and language/runtime differences
all contribute. They do not establish performance on a full real-world library,
on cold startup, or during editor interaction. Full-text changes recompute the
document cache, so read-request timings must not be treated as edit timings.

`cargo run --release --offline --locked --example perf` reported approximately
1.19 ms for analysis of 3,000 synthetic lines and 1.00 microsecond for cloning
their cached semantic tokens. This is an in-process Rust microbenchmark, not an
editor round-trip or a direct Python comparison.

## Outstanding validation and deliberate boundaries

- The supplied Nuitka executables printed a missing-`warnings` import notice,
  loaded the synthetic Finder library, and printed Ready, but did not answer an
  initialize request through pipes in this environment. `python_reference.py`
  therefore imports the unmodified supplied Python source, calls its handlers,
  and provides minimal protocol value objects and stdio framing. It does not
  emulate pygls initialization, scheduling, or validation. An operational binary
  can be tested with `parity.py --python-binary /path/to/skill_lsp`.
- TCP socket creation is denied by the workspace sandbox (`Operation not
  permitted`), so TCP initialization, reconnection, and exit are not runtime
  verified. Stdio is runtime verified.
- Lite-XL/Neovim interaction, Windows/macOS builds, and a real Finder library
  have not been tested. The existing CI configuration remains local and has
  not been published or run remotely.
- The Rust server deliberately retains the Python handlers' quirks documented
  in README.md, including Unicode code-point columns and line-based signature
  help. Finite synthetic coverage is evidence of compatibility, not proof for
  every possible input. Rust uses tower-lsp for protocol framework behavior.
- JSON/export/embedding features from the previous Rust draft remain available.
  Repeated in-process loads do not reproduce Python's global prefix-state leak.
