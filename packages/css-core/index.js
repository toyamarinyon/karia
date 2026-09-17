import { fileURLToPath } from "node:url";

/** Absolute path to the native karia executable after `npm run build`. */
export const binaryPath = fileURLToPath(
  new URL(
    process.platform === "win32"
      ? "./target/release/karia.exe"
      : "./target/release/karia",
    import.meta.url,
  ),
);
export default binaryPath;
