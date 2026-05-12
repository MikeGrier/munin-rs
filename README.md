<!-- Copyright (c) Michael Grier -->
# munin-rs

[![CI](https://github.com/MikeGrier/munin-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/MikeGrier/munin-rs/actions/workflows/ci.yml)
[![release-please](https://github.com/MikeGrier/munin-rs/actions/workflows/release-please.yml/badge.svg)](https://github.com/MikeGrier/munin-rs/actions/workflows/release-please.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Munin is a Rust toolkit for working with MSBuild binary log (`.binlog`)
files.** It ships two crates and a VS Code extension:

| Crate | What it is |
|---|---|
| [`munin`](crates/munin) | Reader and seekable indexed data model for `.binlog` files. |
| [`munin-binlog-mcp`](crates/munin-binlog-mcp) | MCP (Model Context Protocol) server that exposes binlog queries to AI agents like GitHub Copilot. |
| [`Munin Binlog MCP` VS Code extension](crates/munin-binlog-mcp/extension) | Bundles the MCP server and registers it with VS Code automatically. |

The format itself is documented in the MSBuild repository:
<https://github.com/dotnet/msbuild/blob/main/documentation/wiki/Binary-Log.md>

## Repository layout

```
crates/
  munin/                       # core .binlog reader + index (library)
  munin-binlog-mcp/            # MCP server binary + library
    extension/                 # VS Code extension that bundles the binary
.github/
  workflows/                   # ci, build-extension, release-please, publish-extension
  actions/workspace-version/   # composite action that reads workspace version
tools/
  check-encoding.ps1           # CI guard against encoding corruption
release-please-config.json
.release-please-manifest.json
Cargo.toml                     # workspace root
```

## Building from source

Requires a recent Rust toolchain (MSRV: see `[workspace.package].rust-version`
in [Cargo.toml](Cargo.toml)).

```powershell
# Build everything
cargo build --workspace --release

# Run the test suite
cargo test --workspace

# Run the MCP server directly (stdio JSON-RPC)
cargo run --release -p munin-binlog-mcp
```

The MCP server binary is produced at `target/release/munin-binlog-mcp.exe`.
See [`crates/munin-binlog-mcp/README.md`](crates/munin-binlog-mcp/README.md)
for the tool reference and an `mcp.json` snippet that points VS Code at a
local build.

## VS Code extension

The simplest way to use the MCP server with VS Code Copilot is the
**Munin Binlog MCP** extension, which bundles the platform-appropriate
binary and registers it with VS Code automatically -- no `mcp.json` editing
required.

- Marketplace listing: [`MikeGrierTools.munin-binlog-mcp`](https://marketplace.visualstudio.com/items?itemName=MikeGrierTools.munin-binlog-mcp)
- Extension docs: [`crates/munin-binlog-mcp/extension/README.md`](crates/munin-binlog-mcp/extension/README.md)

Pre-built binaries ship for **Windows x64 and arm64**. Linux/macOS users
should build from source per the steps above.

## Release pipeline

Versioning, tagging, and Marketplace publishing are fully automated:

1. **Conventional Commits** on `main` (`fix:`, `feat:`, `feat!:`)
2. [`release-please`](.github/workflows/release-please.yml) opens or updates
   a Release PR that bumps the workspace version, the extension's
   `package.json`, and the changelog.
3. Merging the Release PR creates a `v<version>` tag.
4. [`publish-extension`](.github/workflows/publish-extension.yml) builds
   per-platform VSIXes, then -- gated behind a required-reviewer
   environment -- publishes them to the VS Code Marketplace and attaches
   them to a GitHub Release.

Crates.io publishing for `munin` and `munin-binlog-mcp` is currently manual.

## Contributing

Issues and pull requests welcome. The CI suite (`cargo build`, `cargo test`,
MSRV check, encoding sanity check) must pass; commit messages should follow
[Conventional Commits](https://www.conventionalcommits.org/) so
release-please can categorize them correctly.

## License

MIT -- see [LICENSE](LICENSE).
