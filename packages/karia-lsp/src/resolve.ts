import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';

const PACKAGE_NAME = 'karia-lsp';
const SERVER_RELATIVE = 'dist/server.js';

/**
 * Locate this package's built server entry point as installed for `fromDir`
 * (e.g. a workspace root). Resolves through node_modules, so a project-local
 * install wins over a copy bundled inside an editor extension.
 */
export function resolveServerPath(fromDir: string): string {
  const require = createRequire(path.join(fromDir, 'noop.js'));
  const packageJson = require.resolve(`${PACKAGE_NAME}/package.json`);
  const serverPath = path.join(path.dirname(packageJson), SERVER_RELATIVE);
  if (!fs.existsSync(serverPath)) {
    throw new Error(
      `${PACKAGE_NAME} resolved at ${packageJson}, but ${SERVER_RELATIVE} is missing — build the package first`,
    );
  }
  return serverPath;
}
