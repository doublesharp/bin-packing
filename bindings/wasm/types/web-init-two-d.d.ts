// Web initialization returns raw WASM exports, not the ergonomic object API.
// Keep the ABI tied to the declarations generated for this exact WASM binary.
export type InitOutput = typeof import('./bin_packing_wasm_bg.wasm');
export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;
export type SyncInitInput = BufferSource | WebAssembly.Module;

/** Initialize once before calling the module-level solve and cut functions. */
export default function init(
  module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>,
): Promise<InitOutput>;

/** Synchronous initialization from bytes or an already compiled module. */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;
