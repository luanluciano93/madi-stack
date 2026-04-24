import { writable, derived, get } from 'svelte/store';

/// Tab identifiers used by the router in App.svelte. Keep in sync with the
/// `Route` union there — the tour drives navigation by setting this value.
export type TourRoute =
  | 'geral'
  | 'nginx'
  | 'mariadb'
  | 'php'
  | 'sites'
  | 'firewall'
  | 'configuracoes'
  | 'atualizacoes'
  | 'sobre';

export interface TourStep {
  /// Route that must be active for this step. The tour engine flips the
  /// router store before searching for the anchor element.
  route: TourRoute;
  /// `data-tour` attribute value on the target element. The Coachmark
  /// popover anchors to this element via `getBoundingClientRect`.
  anchor: string;
  /// i18n key for the popover title (translated via `$_`).
  titleKey: string;
  /// i18n key for the popover body (translated via `$_`).
  bodyKey: string;
}

/// Coach-mark sequence. Three hotspots covering the main daily actions —
/// install the stack, expose a site over HTTPS, check for updates.
export const TOUR_STEPS: readonly TourStep[] = [
  {
    route: 'geral',
    anchor: 'install-all',
    titleKey: 'tour.step1_title',
    bodyKey: 'tour.step1_body',
  },
  {
    route: 'sites',
    anchor: 'https-toggle',
    titleKey: 'tour.step2_title',
    bodyKey: 'tour.step2_body',
  },
  {
    route: 'atualizacoes',
    anchor: 'check-updates',
    titleKey: 'tour.step3_title',
    bodyKey: 'tour.step3_body',
  },
];

const STORAGE_KEY = 'madistack.tour_done';

/// Index of the current step (-1 means tour is not running). Exposed for
/// reactive access from Svelte components; mutations go through the
/// `tour` helper object below so the state transitions stay coherent.
const stepIndex = writable<number>(-1);

/// Router callback provided by App.svelte via `bindTourNavigation`. The
/// tour needs to flip the active tab before showing a step — keeping this
/// as an injected function avoids a circular dep between tour.ts and the
/// App-level store.
let navigate: ((route: TourRoute) => void) | null = null;

export function bindTourNavigation(fn: (route: TourRoute) => void) {
  navigate = fn;
}

function markDone() {
  try {
    localStorage.setItem(STORAGE_KEY, '1');
  } catch {
    // Storage blocked — accept that the tour may replay next launch.
  }
}

function isDone(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

/// True while the tour is active (stepIndex >= 0). Components subscribe to
/// this to render the Coachmark overlay.
export const tourActive = derived(stepIndex, ($i) => $i >= 0);

/// Currently-visible step, or `null` when the tour is not running.
export const tourCurrentStep = derived(stepIndex, ($i) =>
  $i >= 0 && $i < TOUR_STEPS.length ? TOUR_STEPS[$i] : null,
);

/// Progress counters for the "Passo X de Y" label.
export const tourProgress = derived(stepIndex, ($i) => ({
  current: Math.max(0, $i) + 1,
  total: TOUR_STEPS.length,
}));

export const tour = {
  /// Begin the tour from step 0. Navigates to the first step's route so
  /// the anchor element is mounted before Coachmark measures it.
  start() {
    if (!navigate) return;
    navigate(TOUR_STEPS[0].route);
    stepIndex.set(0);
  },

  /// Auto-start on first launch. Called once from App.svelte's `onMount`.
  /// No-ops if the user has already completed or skipped a previous tour.
  startIfFirstRun() {
    if (isDone()) return;
    this.start();
  },

  /// Advance to the next step, or finish when we run off the end.
  next() {
    const i = get(stepIndex);
    const nextIdx = i + 1;
    if (nextIdx >= TOUR_STEPS.length) {
      this.finish();
      return;
    }
    if (navigate) navigate(TOUR_STEPS[nextIdx].route);
    stepIndex.set(nextIdx);
  },

  /// Finish successfully — marks as done so the tour doesn't replay.
  finish() {
    stepIndex.set(-1);
    markDone();
  },

  /// Skip — same persistence as finish; the user explicitly opted out.
  skip() {
    this.finish();
  },

  /// Manually replay the tour (e.g. a "Restart tutorial" button in
  /// Settings). Clears the done flag so a subsequent full-app restart
  /// would also reshow the tour, matching user expectation.
  restart() {
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // ignore
    }
    this.start();
  },
};
