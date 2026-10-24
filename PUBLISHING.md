# Public repository and release setup

The standalone public project is `skill-lsp-rs`, not the parent CodexLive folder.
The latter also contains the original Python binaries and local Finder library.
This project uses the MIT license selected by its author. No Cadence
documentation rights are granted by that license.

## Create a clean public source tree

From `skill-lsp-rs`:

```sh
python3 scripts/package_source.py --check
python3 scripts/package_source.py
```

Extract `target/skill-lsp-public-source.zip` into a new empty directory. Its
`skill-lsp` folder contains only the files in `PUBLIC_FILES.json`. The exporter
does not recursively inspect any Finder directory; it only includes the two
named invented test fixtures. Real `.fnd` files, generated API databases,
compiled binaries, node_modules, caches, VSIX files, credentials, and the sibling
Python project are not part of this source archive.

Review that clean directory and initialize Git there, rather than adding the
entire CodexLive workspace:

```sh
git init
git add .
git diff --cached --stat
git commit -m "Initial Rust SKILL language server and editor clients"
```

Create your GitHub repository and follow its instructions to add the remote and
push. No repository, remote, commit, or public upload is created automatically
by this project. If any Finder documentation was previously committed to a
different repository, ignoring it now does not remove it from that history.

## CI and release artifacts

The included workflow tests Rust on Linux, Windows, and macOS, runs synthetic
stdio/Unicode/selector checks, and tests/packages the VS Code extension. CI
clears `SKILL_EMBED_FND_DIR` so public binaries cannot accidentally embed a
private documentation library. Native server executables and the VSIX are
uploaded as workflow artifacts for manual download; no automatic release or
Marketplace publishing occurs.

The first VSIX uses an external server executable rather than bundling one
platform's binary. Download the server artifact for your OS, or run
`cargo build --release --locked` / `cargo install --path . --locked`.
Configure `skillLsp.serverPath` in VS Code. Users supply their own authorized
Finder library through `skillLsp.fndDir`, a local API database, or normal server
discovery. Do not publish binaries built with a private library embedded.

## VS Code identity and manual publishing

Update `editors/vscode/package.json` with:

- Your Marketplace publisher ID instead of `skill-lsp-community`.
- The real GitHub repository, issues, and homepage URLs.
- A version and changelog appropriate to the release.

Then run:

```sh
cd editors/vscode
npm ci --ignore-scripts
npm test
npm run package
```

Install the VSIX locally and test a real editor session before a public release.
The headless checks do not prove that every installed theme or competing SKILL
extension behaves correctly. Marketplace publishing is a separate manual action
using your account; the repository contains no publishing token or automatic
publish job.

## Maintainer notes

`editors/skill_colors.lua` and `editors/skill_lsp_config.lua` are the portable
Lite-XL files included in the public export. Local CodexLive copies in the
sibling Python folder are not exported. Keep the portable copy synchronized
when making future Lua improvements.

Python-source parity scripts optionally use the original implementation in a
sibling directory. The public project and CI do not require that source: unit
tests, the checked-in invented metadata snapshot, selector checks, Unicode
checks, VS Code tests, and the stdio smoke tests are self-contained.
