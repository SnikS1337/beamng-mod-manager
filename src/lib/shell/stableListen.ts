import { isTauri } from "@tauri-apps/api/core";
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

type Handler = (event: Event<unknown>) => void;
interface Hub {
  handlers: Set<Handler>;
  ready: Promise<void> | null;
}

const hubs = new Map<string, Hub>();

/** Один Tauri-listener на событие: исчезновение компонента не оставляет мёртвый callback. */
export async function stableListen<T>(
  eventName: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  let hub = hubs.get(eventName);
  if (!hub) {
    hub = { handlers: new Set(), ready: null };
    hubs.set(eventName, hub);
  }
  const wrapped: Handler = (event) => handler(event as Event<T>);
  hub.handlers.add(wrapped);
  if (!hub.ready) {
    hub.ready = listen(eventName, (event) => {
      for (const handler of hub!.handlers) handler(event as Event<unknown>);
    }).then(() => {});
  }
  await hub.ready;
  return () => { hub!.handlers.delete(wrapped); };
}