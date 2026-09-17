#!/usr/bin/env node

/**
 * Downloads the platform-specific native binary from GitHub Releases when the
 * installed package does not already contain it, and points npm's global bin
 * entries at the native binary (zero-overhead) where possible.
 */

import {
  chmodSync,
  createWriteStream,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { platform } from "node:os";
import { get } from "node:https";
import { execSync } from "node:child_process";
import { binaryName, binaryPath } from "../index.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, "..");

const packageJson = JSON.parse(
  readFileSync(join(packageRoot, "package.json"), "utf8"),
);

const GITHUB_REPO = "toyamarinyon/karia";
const downloadUrl = `https://github.com/${GITHUB_REPO}/releases/download/v${packageJson.version}/${binaryName()}`;

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    const request = (target) => {
      get(target, (response) => {
        if (response.statusCode === 301 || response.statusCode === 302) {
          request(response.headers.location);
          return;
        }
        if (response.statusCode !== 200) {
          reject(new Error(`HTTP ${response.statusCode}`));
          return;
        }
        response.pipe(file);
        file.on("finish", () => file.close(resolve));
      }).on("error", (error) => {
        unlinkSync(dest);
        reject(error);
      });
    };
    request(url);
  });
}

/** Point npm's global bin entry at the native binary directly. */
async function fixGlobalInstallBin() {
  let npmBinDir;
  try {
    const prefix = execSync("npm prefix -g", { encoding: "utf8" }).trim();
    npmBinDir = join(prefix, "bin");
  } catch {
    return;
  }

  if (platform() === "win32") {
    const cmdShim = join(npmBinDir, "karia.cmd");
    const ps1Shim = join(npmBinDir, "karia.ps1");
    if (!existsSync(cmdShim)) return;
    const relativePath = `node_modules\\karia\\bin\\${binaryName()}`;
    if (!existsSync(join(npmBinDir, relativePath))) return;
    try {
      writeFileSync(
        cmdShim,
        `@ECHO off\r\n"%~dp0${relativePath}" %*\r\n`,
      );
      writeFileSync(
        ps1Shim,
        `#!/usr/bin/env pwsh\r\n$basedir = Split-Path $MyInvocation.MyCommand.Definition -Parent\r\n& "$basedir\\${relativePath}" $args\r\nexit $LASTEXITCODE\r\n`,
      );
    } catch {
      // Non-critical: the JS wrapper still works.
    }
    return;
  }

  const symlinkPath = join(npmBinDir, "karia");
  try {
    if (!lstatSync(symlinkPath).isSymbolicLink()) return;
  } catch {
    return;
  }
  try {
    unlinkSync(symlinkPath);
    symlinkSync(binaryPath, symlinkPath);
    console.log("karia: global bin now points at the native binary");
  } catch (error) {
    console.log(`karia: could not optimize global bin: ${error.message}`);
  }
}

async function main() {
  // Development installs (the package is private) never download.
  if (packageJson.private === true) return;

  const name = binaryName();
  if (!name) {
    console.error(
      `karia: unsupported platform ${platform()}-${process.arch}`,
    );
    return;
  }

  if (existsSync(binaryPath)) {
    if (platform() !== "win32") chmodSync(binaryPath, 0o755);
    await fixGlobalInstallBin();
    return;
  }

  mkdirSync(dirname(binaryPath), { recursive: true });
  console.log(`karia: downloading ${name} from ${downloadUrl}`);
  try {
    await downloadFile(downloadUrl, binaryPath);
    if (platform() !== "win32") chmodSync(binaryPath, 0o755);
    console.log(`karia: installed ${name}`);
  } catch (error) {
    console.log(`karia: could not download native binary: ${error.message}`);
    console.log("");
    console.log("To build it locally instead:");
    console.log("  1. Install Rust: https://rustup.rs");
    console.log("  2. Run: npm run build --workspace=karia");
  }

  await fixGlobalInstallBin();
}

main().catch(console.error);
