import { fileURLToPath } from "node:url";

/** Absolute path to the native karia executable after `npm run build`. */
export const binaryPath = fileURLToPath(new URL("./target/release/karia", import.meta.url));
export default binaryPath;
