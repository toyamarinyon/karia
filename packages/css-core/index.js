import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { arch, platform } from "node:os";

const binDir = dirname(fileURLToPath(new URL("./bin/karia.js", import.meta.url)));

/** Whether the current system uses musl libc (e.g. Alpine Linux). */
export function isMusl() {
  if (platform() !== "linux") return false;
  try {
    const output = execSync("ldd --version 2>&1 || true", { encoding: "utf8" });
    return output.toLowerCase().includes("musl");
  } catch {
    return (
      existsSync("/lib/ld-musl-x86_64.so.1") ||
      existsSync("/lib/ld-musl-aarch64.so.1")
    );
  }
}

/** `karia-<os>-<arch>[.exe]` for the current platform, or null if unsupported. */
export function binaryName() {
  const os = platform();
  const cpuArch = arch();

  let osKey;
  switch (os) {
    case "darwin":
      osKey = "darwin";
      break;
    case "linux":
      osKey = isMusl() ? "linux-musl" : "linux";
      break;
    case "win32":
      osKey = "win32";
      break;
    default:
      return null;
  }

  let archKey;
  switch (cpuArch) {
    case "x64":
    case "x86_64":
      archKey = "x64";
      break;
    case "arm64":
    case "aarch64":
      archKey = "arm64";
      break;
    default:
      return null;
  }

  // No native Windows ARM64 build: run the x64 binary under emulation.
  if (osKey === "win32" && archKey === "arm64") {
    const nativeName = `karia-${osKey}-${archKey}.exe`;
    if (!existsSync(join(binDir, nativeName))) {
      archKey = "x64";
    }
  }

  const ext = os === "win32" ? ".exe" : "";
  return `karia-${osKey}-${archKey}${ext}`;
}

/**
 * Absolute path to the native binary shipped for this platform.
 * The file may not exist until `npm run build` has copied it into bin/.
 */
export const binaryPath = (() => {
  const name = binaryName();
  if (!name) {
    throw new Error(`karia: unsupported platform ${platform()}-${arch()}`);
  }
  return join(binDir, name);
})();
export default binaryPath;
