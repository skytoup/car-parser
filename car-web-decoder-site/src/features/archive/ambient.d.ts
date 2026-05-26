declare module "fflate" {
  export interface ZipOptions {
    level?: number;
  }

  export function zipSync(
    data: Record<string, Uint8Array>,
    opts?: ZipOptions,
  ): Uint8Array;
}
