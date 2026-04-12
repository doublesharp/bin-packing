// Additional type declarations appended to the two-d `web` target's `.d.ts`.

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
  readonly solve2d: typeof solve2d;
  readonly version: typeof version;
}
