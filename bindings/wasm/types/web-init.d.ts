// Additional type declarations appended to the `web` target's `.d.ts`.
// The `web` target produced by wasm-bindgen requires an explicit `init()`
// call that loads and instantiates the `.wasm` file. This matches the
// original wasm-bindgen `web` template.

/**
 * Initialize the WebAssembly module. Required before calling any of the
 * exported functions on the `web` target.
 *
 * Pass a URL, `Response`, `BufferSource`, or pre-compiled `WebAssembly.Module`
 * pointing at the packaged `.wasm` file. Call with no arguments to let the
 * runtime pick a default path relative to the importing module's URL.
 */
export default function init(
  module_or_path?:
    | { module_or_path: RequestInfo | URL | Response | BufferSource | WebAssembly.Module }
    | RequestInfo
    | URL
    | Response
    | BufferSource
    | WebAssembly.Module,
): Promise<InitOutput>;

/** Object returned by `init()` — same shape as the module's top-level exports. */
export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly solve1d: typeof solve1d;
  readonly solve2d: typeof solve2d;
  readonly solve3d: typeof solve3d;
  readonly solve1dJson: typeof solve1dJson;
  readonly solve2dJson: typeof solve2dJson;
  readonly solve3dJson: typeof solve3dJson;
  readonly plan1dCuts: typeof plan1dCuts;
  readonly plan2dCuts: typeof plan2dCuts;
  readonly version: typeof version;
}
