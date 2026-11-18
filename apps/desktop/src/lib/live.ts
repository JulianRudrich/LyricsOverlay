import { api, subscribe, type Listeners } from "./api";
import type { Snapshot } from "./types";

/**
 * Listens to the events of the Rust side, then loads the current state once.
 * Returns the function that stops listening, so it fits `onMount`.
 */
export function connect(listeners: Listeners, loaded: (snapshot: Snapshot) => void): () => void {
  let stop: (() => void) | undefined;
  let alive = true;
  void (async () => {
    const unlisten = await subscribe(listeners);
    if (!alive) return unlisten();
    stop = unlisten;
    const snapshot = await api.snapshot();
    if (alive) loaded(snapshot);
  })();
  return () => {
    alive = false;
    stop?.();
  };
}
