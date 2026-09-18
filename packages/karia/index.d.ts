/** Whether the current system uses musl libc (e.g. Alpine Linux). */
export declare function isMusl(): boolean;

/** `karia-<os>-<arch>[.exe]` for the current platform, or null if unsupported. */
export declare function binaryName(): string | null;

/**
 * Absolute path to the native binary shipped for this platform.
 * The file may not exist until `npm run build` has copied it into bin/.
 */
export declare const binaryPath: string;
export default binaryPath;
