#!/usr/bin/env node

/**
 * Syncs the version from packages/karia/package.json to Cargo.toml,
 * Cargo.lock, and packages/karia-lsp/package.json.
 * package.json is the single source of truth.
 */

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const coreDir = join(__dirname, "..");
const rootDir = join(coreDir, "..", "..");

const version = JSON.parse(
  readFileSync(join(coreDir, "package.json"), "utf8"),
).version;

// Cargo.toml
const cargoTomlPath = join(coreDir, "Cargo.toml");
const cargoToml = readFileSync(cargoTomlPath, "utf8");
const versionRegex = /^version\s*=\s*"[^"]*"/m;
const next = `version = "${version}"`;
const previous = cargoToml.match(versionRegex)?.[0];

if (!versionRegex.test(cargoToml)) {
  console.error("Could not find version field in Cargo.toml");
  process.exit(1);
}
let cargoTomlUpdated = false;
if (previous !== next) {
  writeFileSync(cargoTomlPath, cargoToml.replace(versionRegex, next));
  cargoTomlUpdated = true;
}

// packages/karia-lsp/package.json
const lspPkgPath = join(rootDir, "packages", "karia-lsp", "package.json");
const lspPkg = JSON.parse(readFileSync(lspPkgPath, "utf8"));
if (lspPkg.version !== version) {
  lspPkg.version = version;
  writeFileSync(lspPkgPath, JSON.stringify(lspPkg, null, 2) + "\n");
}

// Cargo.lock
if (cargoTomlUpdated) {
  try {
    execSync("cargo update -p karia --offline", {
      cwd: coreDir,
      stdio: "pipe",
    });
  } catch {
    try {
      execSync("cargo update -p karia", { cwd: coreDir, stdio: "pipe" });
    } catch (error) {
      console.error(`Warning: could not update Cargo.lock: ${error.message}`);
    }
  }
}
