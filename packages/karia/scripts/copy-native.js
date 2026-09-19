#!/usr/bin/env node

/**
 * Copies the compiled Rust binary into bin/ with its platform-specific name.
 */

import { copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { binaryName, binaryPath } from "../index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, "..");

const ext = process.platform === "win32" ? ".exe" : "";
const sourcePath = join(packageRoot, `target/release/karia${ext}`);

if (!existsSync(sourcePath)) {
  console.error(`Error: native binary not found at ${sourcePath}`);
  console.error(
    "Run cargo build --release --manifest-path packages/karia/Cargo.toml first",
  );
  process.exit(1);
}

const name = binaryName();
if (!name) {
  console.error(
    `Error: unsupported platform ${process.platform}-${process.arch}`,
  );
  process.exit(1);
}

mkdirSync(dirname(binaryPath), { recursive: true });
// Remove before copying: overwriting in place keeps the old inode's
// com.apple.provenance xattr, and macOS kills the mismatched binary.
rmSync(binaryPath, { force: true });
copyFileSync(sourcePath, binaryPath);
console.log(`Copied native binary to ${binaryPath}`);
