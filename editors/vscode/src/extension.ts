import * as vscode from 'vscode';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveServerPath } from 'karia-lsp/resolve';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node.js';

const SERVER_NAME = 'karia';
const PACKAGE_NAME = 'karia-lsp';

let client: LanguageClient | undefined;

function findServerModule(): string {
  const configured = vscode.workspace.getConfiguration(SERVER_NAME).get<string>('serverPath');
  if (configured) {
    return configured;
  }

  // Project-local install wins; fall back to the copy bundled with this extension.
  const dirs = (vscode.workspace.workspaceFolders ?? []).map(folder => folder.uri.fsPath);
  dirs.push(path.dirname(fileURLToPath(import.meta.url)));
  for (const dir of dirs) {
    try {
      return resolveServerPath(dir);
    } catch {
      continue;
    }
  }

  throw new Error(
    `karia: ${PACKAGE_NAME} not found. Install it in your project (pnpm add -D ${PACKAGE_NAME}) or set "karia.serverPath" in settings.`,
  );
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const serverModule = findServerModule();
  const serverOptions: ServerOptions = {
    run: { module: serverModule, transport: TransportKind.stdio, args: ['--stdio'] },
    debug: { module: serverModule, transport: TransportKind.stdio, args: ['--stdio'] },
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'css' },
      { scheme: 'file', language: 'typescriptreact' },
      { scheme: 'file', language: 'typescript' },
      { scheme: 'file', language: 'javascript' },
    ],
  };

  client = new LanguageClient(SERVER_NAME, 'Karia', serverOptions, clientOptions);
  context.subscriptions.push(client);
  await client.start();
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
