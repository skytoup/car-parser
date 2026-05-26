declare module "@car-wasm/client" {
  export interface CarWasmClientDiagnosticEvent {
    requestId?: number;
    requestType: string;
    source: "client" | "worker";
    stage: string;
    timestamp: number;
    detail?: unknown;
  }

  export class CarWasmClientError extends Error {
    code: string;
    requestId?: number;
    requestType?: string;
    phase?: string;
    trace?: CarWasmClientDiagnosticEvent[];
    details?: unknown;
    cause?: unknown;
    constructor(
      code: string,
      message: string,
      options?: {
        requestId?: number;
        requestType?: string;
        phase?: string;
        trace?: CarWasmClientDiagnosticEvent[];
        details?: unknown;
        cause?: unknown;
      },
    );
  }

  export interface CarWasmArchiveClientOptions {
    worker?: Worker;
    timeoutMs?: number;
    consoleDiagnostics?: boolean;
    onDiagnostic?: (event: CarWasmClientDiagnosticEvent) => void;
  }

  export class CarWasmArchiveClient {
    constructor(worker: Worker, options?: Omit<CarWasmArchiveClientOptions, "worker">);
    static load(
      bytes: Uint8Array | ArrayBuffer | ArrayBufferView,
      options?: CarWasmArchiveClientOptions,
    ): Promise<CarWasmArchiveClient>;

    setDiagnosticListener(
      listener: ((event: CarWasmClientDiagnosticEvent) => void) | null,
    ): void;
    documentInfo(): Promise<Record<string, unknown>>;
    listEntries(): Promise<unknown[]>;
    listImages(): Promise<unknown[]>;
    listEntrySummaries(): Promise<unknown[]>;
    listImageSummaries(): Promise<unknown[]>;
    getEntryInfo(id: string): Promise<unknown>;
    getImageInfo(id: string): Promise<unknown>;
    getDisplayPayload(id: string): Promise<unknown>;
    getDownloadPayload(id: string): Promise<unknown>;
    getThumbnailPayload(
      id: string,
      options?: { maxDimension?: number },
    ): Promise<unknown>;
    terminate(): void;
  }
}
