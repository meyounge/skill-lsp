# SKILL Language Server for VS Code

Local SKILL support powered by the Rust `skill-lsp` server: completion, hover,
car/cdr traversal diagrams, definitions, references, signature help, diagnostics,
formatting, and the project's 10 API color categories. Syntax colors work before
the server starts. `.il`, `.ils`, and `.skill` files are registered as SKILL.

No Cadence Finder documentation is included, uploaded, or downloaded. Each user
supplies their own locally authorized library. Without API documentation, syntax
keywords, current-document symbols, and computed car/cdr selectors still work.

## Install locally

1. Build the server from the repository root:

   ```sh
   cargo build --release --locked
   ```

2. In `editors/vscode`, install and package the extension:

   ```sh
   npm ci --ignore-scripts
   npm test
   npm run package
   ```

3. In VS Code, run **Extensions: Install from VSIX...** and select the generated
   `skill-lsp-0.1.1.vsix`. Alternatively:

   ```sh
   code --install-extension skill-lsp-0.1.1.vsix
   ```

4. Configure the server executable in VS Code User Settings (JSON):

   ```json
   {
     "skillLsp.serverPath": "/absolute/path/to/skill-lsp",
     "skillLsp.fndDir": "/absolute/path/to/private/SKILL"
   }
   ```

On Windows use `skill-lsp.exe`; paths can use forward slashes. The server is not
bundled in this initial VSIX. Putting `skill-lsp` on PATH also works. For a demo
using only invented data, set `fndDir` to this repository's `examples/SKILL`.

Open a SKILL file. If another extension claims it, select **Change Language Mode**
and choose **SKILL**. Other SKILL extensions may need to be disabled if they also
start a language server or register competing syntax/semantic providers.

## Settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `skillLsp.serverPath` | `skill-lsp` | Executable path or PATH command. |
| `skillLsp.fndDir` | empty | Local Finder directory; empty uses server discovery. |
| `skillLsp.database` | empty | Local precompiled API JSON; overrides `fndDir`. |
| `skillLsp.maxCompletions` | `0` | Optional response cap; zero is unlimited. |

`${workspaceFolder}` in paths resolves to the first workspace folder. Relative
executable paths containing `/` or `\` are also resolved there. For multi-root
workspaces, use absolute User Settings paths to avoid ambiguity.
Paths refer to the extension host machine: with Remote SSH, WSL, or a dev
container, install the binary and supply the library on that remote host.

Settings changes restart the server automatically. Commands:

- **SKILL: Restart Language Server**
- **SKILL: Show Language Server Output**

The extension uses VS Code's standard language client and native semantic token
rendering; it does not start Lite-XL's separate coloring process. UTF-16 protocol
positions are supported, including text after emoji. The default palette is
scoped to SKILL and can be overridden by user color settings. Car/cdr selectors
are `keyword2` blue; `@key`, `@rest`, and `@optional` are `keyword7` red.

## Develop

Open this directory in VS Code, run `npm ci --ignore-scripts`, and press **F5**
using the supplied launch configuration. In the Extension Development Host,
configure `skillLsp.serverPath`, then open a `.il` file. `npm test` runs the
headless launch/lifecycle, settings, grammar, and palette tests. Actual VS Code
UI behavior should be checked in the Extension Development Host before release.

The upstream architecture is documented in Microsoft's
[Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
and [Semantic Highlight Guide](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide).

## Publishing

The publisher ID `skill-lsp-community` is a placeholder for local builds. Before
publishing to Marketplace, replace it with your registered publisher ID and add
the real repository/homepage/issue URLs to `package.json`. Then run `npm ci`,
tests, and `npm run package` again. Nothing in these scripts publishes an
extension or authenticates to an external account automatically.

The lockfile includes a patched MSAL override for the development-only VSIX
packager's dependency advisory. The extension's runtime dependencies do not
include Azure authentication packages. Review this override when updating vsce;
Marketplace authentication/publishing itself has not been tested here.

MIT licensed. Independent community software; not affiliated with Cadence.
Cadence documentation and trademarks are not licensed by this project's MIT
license. Keep local documentation and generated API databases out of public
repositories and extension packages.
