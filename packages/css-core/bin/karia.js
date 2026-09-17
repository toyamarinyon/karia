#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { binaryPath } from '../index.js';

const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
child.on('error', (error) => {
  console.error(`karia: ${error.message}`);
  if (error.code === 'ENOENT') console.error('Native binary is missing. In the source workspace, run npm run build first.');
  process.exitCode = 1;
});
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('exit', (code, signal) => {
  if (signal) {
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
  } else process.exitCode = code ?? 1;
});
