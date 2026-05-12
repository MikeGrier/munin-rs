// Copyright (c) Michael Grier.
//
// munin-binlog-mcp VS Code extension entry point.
//
// Registers the bundled `munin-binlog-mcp` binary as an MCP server so that
// Copilot Chat (and any other VS Code MCP consumer) discovers it automatically
// with no `.vscode/mcp.json` editing required.
//
// The provider id declared in `package.json` (`munin-binlog-mcp`) MUST match
// the id passed to `vscode.lm.registerMcpServerDefinitionProvider`.

import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";

const PROVIDER_ID = "munin-binlog-mcp";
const SERVER_LABEL = "munin-binlog-mcp";

/**
 * Resolve the path to the `munin-binlog-mcp` binary that should be spawned.
 *
 * Resolution order:
 *   1. The `munin-binlog-mcp.binaryPath` user/workspace setting (if non-empty
 *      and the file exists). Intended for developers running against a
 *      locally-built `munin-binlog-mcp`.
 *   2. The platform-appropriate binary bundled inside the extension at
 *      `<extensionPath>/bin/munin-binlog-mcp[.exe]`.
 *
 * Returns `undefined` if no usable binary can be located.
 */
function resolveBinaryPath(context: vscode.ExtensionContext): string | undefined {
    const config = vscode.workspace.getConfiguration("munin-binlog-mcp");

    const override = (config.get<string>("binaryPath") ?? "").trim();
    if (override.length > 0) {
        if (fs.existsSync(override)) {
            return override;
        }
        console.warn(
            `[munin-binlog-mcp] munin-binlog-mcp.binaryPath = ${override} does not exist; ` +
                "falling back to bundled binary.",
        );
    }

    const binaryName = process.platform === "win32" ? "munin-binlog-mcp.exe" : "munin-binlog-mcp";
    const bundled = path.join(context.extensionPath, "bin", binaryName);
    if (fs.existsSync(bundled)) {
        return bundled;
    }
    return undefined;
}

/**
 * Build the argument vector for spawning `munin-binlog-mcp` based on current
 * settings.
 */
function buildArgs(): string[] {
    const config = vscode.workspace.getConfiguration("munin-binlog-mcp");
    const args: string[] = [];

    const extraArgs = config.get<string[]>("extraArgs", []) ?? [];
    for (const a of extraArgs) {
        if (typeof a === "string" && a.length > 0) {
            args.push(a);
        }
    }

    return args;
}

/**
 * Resolve the version string to advertise for `binary`.
 *
 * If `binary` is the bundled binary, reads `<extensionPath>/bin/VERSION`
 * (written by CI). Falls back to the package.json version for local dev.
 * If `binary` is a user override, looks for a sibling VERSION file.
 */
function readBinaryVersion(context: vscode.ExtensionContext, binary: string): string {
    const bundledDir = path.join(context.extensionPath, "bin");
    const isBundled =
        path.normalize(path.dirname(binary)).toLowerCase() ===
        path.normalize(bundledDir).toLowerCase();

    if (isBundled) {
        const v = readVersionFile(path.join(bundledDir, "VERSION"));
        if (v !== undefined) {
            return v;
        }
        return context.extension.packageJSON.version ?? "0.0.0";
    }

    const sibling = readVersionFile(path.join(path.dirname(binary), "VERSION"));
    if (sibling !== undefined) {
        return `${sibling} (override)`;
    }
    return "override";
}

function readVersionFile(versionFile: string): string | undefined {
    try {
        if (fs.existsSync(versionFile)) {
            const v = fs.readFileSync(versionFile, "utf8").trim();
            if (v.length > 0) {
                return v;
            }
        }
    } catch {
        // fall through
    }
    return undefined;
}

class MuninBinlogMcpServerProvider
    implements vscode.McpServerDefinitionProvider<vscode.McpStdioServerDefinition>
{
    private readonly _onDidChange = new vscode.EventEmitter<void>();
    public readonly onDidChangeMcpServerDefinitions = this._onDidChange.event;

    private missingBinaryWarned = false;

    constructor(private readonly context: vscode.ExtensionContext) {
        const sub = vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration("munin-binlog-mcp")) {
                this.missingBinaryWarned = false;
                this._onDidChange.fire();
            }
        });
        context.subscriptions.push(sub, this._onDidChange);
    }

    public provideMcpServerDefinitions(
        _token: vscode.CancellationToken,
    ): vscode.ProviderResult<vscode.McpStdioServerDefinition[]> {
        const binary = resolveBinaryPath(this.context);
        if (binary === undefined) {
            if (!this.missingBinaryWarned) {
                this.missingBinaryWarned = true;
                void vscode.window.showWarningMessage(
                    "munin-binlog-mcp: bundled server binary not found. " +
                        "Reinstall the extension or set 'munin-binlog-mcp.binaryPath'.",
                );
            }
            return [];
        }

        const version = readBinaryVersion(this.context, binary);

        return [
            new vscode.McpStdioServerDefinition(
                SERVER_LABEL,
                binary,
                buildArgs(),
                /* env */ {},
                version,
            ),
        ];
    }

    public resolveMcpServerDefinition(
        server: vscode.McpStdioServerDefinition,
        _token: vscode.CancellationToken,
    ): vscode.ProviderResult<vscode.McpStdioServerDefinition> {
        return server;
    }
}

export function activate(context: vscode.ExtensionContext): void {
    const provider = new MuninBinlogMcpServerProvider(context);
    context.subscriptions.push(
        vscode.lm.registerMcpServerDefinitionProvider(PROVIDER_ID, provider),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand("munin-binlog-mcp.copyServerPath", async () => {
            const binary = resolveBinaryPath(context);
            if (binary === undefined) {
                await vscode.window.showErrorMessage(
                    "munin-binlog-mcp: bundled server binary not found.",
                );
                return;
            }
            await vscode.env.clipboard.writeText(binary);
            await vscode.window.showInformationMessage(
                `munin-binlog-mcp: copied server path to clipboard: ${binary}`,
            );
        }),
    );

    context.subscriptions.push(
        vscode.commands.registerCommand("munin-binlog-mcp.showServerVersion", async () => {
            const binary = resolveBinaryPath(context);
            if (binary === undefined) {
                await vscode.window.showInformationMessage(
                    "munin-binlog-mcp server: binary not found",
                );
                return;
            }
            const version = readBinaryVersion(context, binary);
            await vscode.window.showInformationMessage(
                `munin-binlog-mcp server version ${version} \u2014 ${binary}`,
            );
        }),
    );
}

export function deactivate(): void {
    // All disposables are managed via context.subscriptions.
}
