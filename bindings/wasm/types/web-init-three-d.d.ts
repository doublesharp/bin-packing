// Additional type declarations appended to the three-d `web` target's `.d.ts`.

export default function init(
  module_or_path?:
    | { module_or_path: RequestInfo | URL | Response | BufferSource | WebAssembly.Module }
    | RequestInfo
    | URL
    | Response
    | BufferSource
    | WebAssembly.Module,
): Promise<InitOutput>;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly solve3d: typeof solve3d;
  readonly version: typeof version;
}
