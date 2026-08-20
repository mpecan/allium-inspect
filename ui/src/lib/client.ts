// The typed HTTP client.
//
// Every type it returns is generated from the Rust structs by ts-rs into
// `src/lib/api/`, and `just types-check` fails on any drift. Nothing in this
// file restates a shape the server already defines — the only thing here is
// which URL carries which one, and what to do when a request fails.

import type { Diagnostic } from "./api/Diagnostic";
import type { Finding } from "./api/Finding";
import type { NodeId } from "./api/NodeId";
import type { Obligation } from "./api/Obligation";
import type { SpecGraph } from "./api/SpecGraph";

export type { Diagnostic, Finding, NodeId, Obligation, SpecGraph };

/** A request that did not produce the document it was supposed to. */
export class ApiError extends Error {
  constructor(
    readonly url: string,
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** The source text of one module, with the module it belongs to. */
export interface ModuleSource {
  module: string;
  path: string;
  text: string;
}

/** Which projection of the graph to draw. */
export type ViewKind = "domain" | "flow" | "lifecycle" | "journey";

/**
 * `fetch`, narrowed to the shape this client uses.
 *
 * Injected rather than reached for globally so the tests drive the client with
 * a stub and no network, and so a caller can supply a signal.
 */
export type Fetcher = (
  input: string,
  init?: { signal?: AbortSignal },
) => Promise<Response>;

/**
 * Reads the spec graph and drives the simulator.
 *
 * The base URL is empty by default because the UI is served by the same binary
 * that answers these routes. It exists for the dev server, which runs on its
 * own port and proxies across.
 */
export class InspectClient {
  constructor(
    private readonly fetcher: Fetcher = (input, init) => fetch(input, init),
    private readonly base = "",
  ) {}

  /** The whole graph: modules, nodes, edges, diagnostics, findings. */
  async spec(signal?: AbortSignal): Promise<SpecGraph> {
    return this.json<SpecGraph>("/api/spec", signal);
  }

  /** One module's source text, for the span-anchored source panel. */
  async source(module: string, signal?: AbortSignal): Promise<ModuleSource> {
    return this.json<ModuleSource>(
      `/api/spec/source/${encodeURIComponent(module)}`,
      signal,
    );
  }

  private async json<T>(path: string, signal?: AbortSignal): Promise<T> {
    const url = `${this.base}${path}`;
    let response: Response;
    try {
      response = await this.fetcher(url, { signal });
    } catch (cause) {
      // A rejected fetch is the server having gone away — the user stopped the
      // process, or the machine slept. Saying that is more useful than
      // "TypeError: Failed to fetch".
      if (cause instanceof DOMException && cause.name === "AbortError") {
        throw cause;
      }
      throw new ApiError(
        url,
        0,
        `could not reach allium-inspect at ${url}. Is it still running?`,
      );
    }

    if (!response.ok) {
      const detail = await response.text().catch(() => "");
      throw new ApiError(
        url,
        response.status,
        detail.trim() || `${url} returned ${response.status}`,
      );
    }

    try {
      return (await response.json()) as T;
    } catch {
      throw new ApiError(url, response.status, `${url} did not return JSON`);
    }
  }
}
