#!/usr/bin/env node

import { accessSync, chmodSync, constants, existsSync } from "node:fs";
import { spawn } from "node:child_process";
import { binaryPath } from "../index.js";

if (!existsSync(binaryPath)) {
  console.error(`karia: no native binary found for this platform`);
  console.error(`expected: ${binaryPath}`);
  console.error("");
  console.error(
    "Run npm run build in the source workspace, or reinstall the package to trigger the postinstall download.",
  );
  process.exit(1);
}

if (process.platform !== "win32") {
  try {
    accessSync(binaryPath, constants.X_OK);
  } catch {
    try {
      chmodSync(binaryPath, 0o755);
    } catch (error) {
      console.error(`karia: cannot make binary executable: ${error.message}`);
      console.error(`try running: chmod +x ${binaryPath}`);
      process.exit(1);
    }
  }
}

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  windowsHide: false,
});

child.on("error", (error) => {
  console.error(`karia: ${error.message}`);
  process.exitCode = 1;
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal));
}

child.on("exit", (code, signal) => {
  if (signal) {
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
  } else {
    process.exitCode = code ?? 1;
  }
});
