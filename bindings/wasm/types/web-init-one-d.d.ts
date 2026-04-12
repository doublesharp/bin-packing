// Additional type declarations appended to the one-d `web` target's `.d.ts`.

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
  readonly solve1d: typeof solve1d;
  readonly version: typeof version;
}
