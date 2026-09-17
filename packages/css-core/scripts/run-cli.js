import { fileURLToPath } from 'node:url';

// Workspace convenience command: resolve paths from the monorepo root.
process.chdir(fileURLToPath(new URL('../../../', import.meta.url)));
await import('../bin/karia.js');
