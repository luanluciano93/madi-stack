import { writable, derived } from 'svelte/store';

/// One entry in the global activity log. Keep the shape tiny — the store
/// is bounded to the last N entries, and the UI renders it in a fixed
/// footer panel; fat payloads wouldn't fit anyway.
export type EventLevel = 'info' | 'warn' | 'error';

export interface AppEvent {
  /// Monotonic, assigned by `pushEvent`. Used as Svelte keyed-block key so
  /// DOM nodes don't get reused across unrelated events when older entries
  /// scroll off the ring.
  id: number;
  /// Epoch millis when the event was enqueued (frontend clock).
  ts: number;
  level: EventLevel;
  /// Optional tag — component slug, subsystem name — rendered as a prefix.
  source?: string;
  /// Human-readable message. May be translated or not; we don't translate
  /// these in the store because they often include dynamic fragments that
  /// svelte-i18n can't reinterpret on locale change.
  message: string;
}

const MAX_ENTRIES = 200;
let nextId = 1;

function createStore() {
  const { subscribe, update, set } = writable<AppEvent[]>([]);
  return {
    subscribe,
    push(event: Omit<AppEvent, 'id' | 'ts'>) {
      update((list) => {
        const next = [
          ...list,
          { id: nextId++, ts: Date.now(), ...event },
        ];
        if (next.length > MAX_ENTRIES) {
          next.splice(0, next.length - MAX_ENTRIES);
        }
        return next;
      });
    },
    clear() {
      set([]);
    },
  };
}

export const events = createStore();

/// Most-recent entry, or `null` when the log is empty. Used by the
/// collapsed footer to show a one-line preview of the last activity.
export const lastEvent = derived(events, ($events) =>
  $events.length > 0 ? $events[$events.length - 1] : null,
);
