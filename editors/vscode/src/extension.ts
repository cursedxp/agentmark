import * as vscode from "vscode";
import { execFile } from "child_process";
import * as path from "path";
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind } from "vscode-languageclient/node";
import { CheckReport, parseCheckJson } from "./checkjson";
import { renderPanel } from "./panel";
import { checkInvocation, projectTarget } from "./server";

let client: LanguageClient | undefined;
let panel: vscode.WebviewPanel | undefined;
let lastRoot = "";
let generation = 0;

function startServer(context: vscode.ExtensionContext): void {
  const cfg = vscode.workspace.getConfiguration("agentmark");
  const command = cfg.get<string>("server.path", "agentmark-lsp");
  const serverOptions: ServerOptions = {
    run: { command, transport: TransportKind.stdio },
    debug: { command, transport: TransportKind.stdio },
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "agentmark" }],
    synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/{agentmark,tools}.toml") },
  };
  client = new LanguageClient("agentmark", "AgentMark", serverOptions, clientOptions);
  client.start().catch((e) => {
    vscode.window.showWarningMessage(
      `AgentMark language server did not start (${command}): ${e?.message ?? e}. Highlighting and the touch panel still work. Set "agentmark.server.path".`,
    );
  });
  context.subscriptions.push({ dispose: () => void client?.stop() });
}

function runCheck(cli: string, target: string): Promise<CheckReport> {
  const inv = checkInvocation(cli, target);
  return new Promise((resolve, reject) => {
    // Exit code 1 means "the project has problems" and still prints the JSON document.
    execFile(inv.command, inv.args, { cwd: inv.cwd, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (stdout.trim().startsWith("{")) {
        try {
          return resolve(parseCheckJson(stdout));
        } catch (e) {
          return reject(e);
        }
      }
      reject(new Error(err ? `${err.message}${stderr ? `\n${stderr}` : ""}` : stderr || "no output"));
    });
  });
}

async function showTouchPanel(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  const file = editor?.document.uri.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!file) {
    vscode.window.showInformationMessage("Open an AgentMark file or folder first.");
    return;
  }
  const mine = ++generation;
  const target = projectTarget(file);
  lastRoot = checkInvocation("agentmark", target).cwd;
  if (!panel) {
    panel = vscode.window.createWebviewPanel("agentmarkTouch", "What can this program touch", vscode.ViewColumn.Beside, {
      enableScripts: true,
    });
    panel.onDidDispose(() => (panel = undefined));
    panel.webview.onDidReceiveMessage(async (m: { file: string; line: number }) => {
      const candidates = [path.resolve(lastRoot, m.file)];
      for (const c of candidates) {
        try {
          const doc = await vscode.workspace.openTextDocument(c);
          const pos = new vscode.Position(Math.max(0, m.line - 1), 0);
          await vscode.window.showTextDocument(doc, { selection: new vscode.Range(pos, pos) });
          return;
        } catch {
          /* try the next candidate */
        }
      }
    });
  }
  panel.webview.html = renderPanel(null, "Running agentmark check ...");
  const cli = vscode.workspace.getConfiguration("agentmark").get<string>("cli.path", "agentmark");
  try {
    const report = await runCheck(cli, target);
    if (mine === generation && panel) panel.webview.html = renderPanel(report);
  } catch (e) {
    if (mine === generation && panel) panel.webview.html = renderPanel(null, `Could not run "${cli} check --json": ${(e as Error).message}`);
  }
}

export function activate(context: vscode.ExtensionContext): void {
  startServer(context);
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (panel && (doc.languageId === "agentmark" || /(agentmark|tools)\.toml$/.test(doc.fileName))) {
        void showTouchPanel();
      }
    }),
    vscode.commands.registerCommand("agentmark.showTouchPanel", showTouchPanel),
    vscode.commands.registerCommand("agentmark.restartServer", async () => {
      await client?.stop();
      startServer(context);
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
