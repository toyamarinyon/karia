import * as vscode from 'vscode';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import { LanguageClient, TransportKind, } from 'vscode-languageclient/node.js';
const SERVER_NAME = 'karia';
const PACKAGE_NAME = 'karia-lsp';
let client;
function findServerModule() {
    const configured = vscode.workspace.getConfiguration(SERVER_NAME).get('serverPath');
    if (configured) {
        return configured;
    }
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
        try {
            const require = createRequire(folder.uri.fsPath + '/');
            const packageJson = require.resolve(`${PACKAGE_NAME}/package.json`);
            const serverPath = new URL('../server.js', `file://${packageJson}`).pathname;
            if (fs.existsSync(serverPath)) {
                return serverPath;
            }
        }
        catch {
            continue;
        }
    }
    throw new Error(`karia: ${PACKAGE_NAME} not found. Install it in your project (pnpm add -D ${PACKAGE_NAME}) or set "karia.serverPath" in settings.`);
}
export async function activate(context) {
    const serverModule = findServerModule();
    const serverOptions = {
        run: { module: serverModule, transport: TransportKind.stdio, args: ['--stdio'] },
        debug: { module: serverModule, transport: TransportKind.stdio, args: ['--stdio'] },
    };
    const clientOptions = {
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
export async function deactivate() {
    await client?.stop();
}
//# sourceMappingURL=extension.js.map