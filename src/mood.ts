import type { BuddyState, Mood, NearMiss } from "./types";

const MOOD_PRIORITY: Mood[] = [
  "panic",
  "offline",
  "shiver",
  "drafty",
  "hero",
  "cozy",
];

const CAPTIONS: Record<Mood, string[]> = {
  cozy: ["Bay is content.", "Warm enough. Go make something.", "All quiet in the shop."],
  drafty: ["Door’s open — heat’s escaping.", "Drafty. Close the bay.", "Someone left the door ajar."],
  shiver: ["Getting chilly…", "Freeze margin shrinking.", "Jacket on. Watch that probe."],
  panic: ["Wet contact! Check now.", "Flood mood. Grab a towel.", "Leak panic — go look."],
  offline: ["Probe went quiet.", "No readings lately.", "Sleeping with one eye open…?"],
  hero: ["Told you so.", "Near-miss survived.", "You fixed it. Buddy approves."],
};

export function resolveMood(input: {
  wetContact: boolean;
  feedHealthy: boolean;
  freezeMarginF: number | null;
  doorOpen: boolean;
  recentlyRecovered: boolean;
}): Mood {
  const candidates: Mood[] = [];
  if (input.wetContact) candidates.push("panic");
  if (!input.feedHealthy) candidates.push("offline");
  if (input.freezeMarginF !== null && input.freezeMarginF <= 5) {
    candidates.push("shiver");
  }
  if (input.doorOpen) candidates.push("drafty");
  if (input.recentlyRecovered) candidates.push("hero");
  candidates.push("cozy");

  return MOOD_PRIORITY.find((m) => candidates.includes(m)) ?? "cozy";
}

export function captionFor(mood: Mood): string {
  const options = CAPTIONS[mood];
  return options[Math.floor(Math.random() * options.length)] ?? mood;
}

/** Demo state when ThermalTrace isn’t connected yet. */
export function demoBuddyState(tick = 0): BuddyState {
  const scenes: Array<Omit<BuddyState, "mood" | "caption" | "lastUpdated">> = [
    {
      spaceName: "Garage",
      connected: false,
      temperatureF: 48.2,
      freezeThresholdF: 35,
      freezeMarginF: 13.2,
      timeToFreezeHours: null,
      doorOpen: false,
      wetContact: false,
      feedHealthy: true,
    },
    {
      spaceName: "Garage",
      connected: false,
      temperatureF: 38.1,
      freezeThresholdF: 35,
      freezeMarginF: 3.1,
      timeToFreezeHours: 6.5,
      doorOpen: true,
      wetContact: false,
      feedHealthy: true,
    },
    {
      spaceName: "Garage",
      connected: false,
      temperatureF: 34.4,
      freezeThresholdF: 35,
      freezeMarginF: -0.6,
      timeToFreezeHours: 0.8,
      doorOpen: false,
      wetContact: false,
      feedHealthy: true,
    },
    {
      spaceName: "Garage",
      connected: false,
      temperatureF: null,
      freezeThresholdF: 35,
      freezeMarginF: null,
      timeToFreezeHours: null,
      doorOpen: false,
      wetContact: false,
      feedHealthy: false,
    },
    {
      spaceName: "Garage",
      connected: false,
      temperatureF: 42.0,
      freezeThresholdF: 35,
      freezeMarginF: 7.0,
      timeToFreezeHours: null,
      doorOpen: false,
      wetContact: true,
      feedHealthy: true,
    },
  ];

  const base = scenes[tick % scenes.length]!;
  const mood = resolveMood({
    wetContact: base.wetContact,
    feedHealthy: base.feedHealthy,
    freezeMarginF: base.freezeMarginF,
    doorOpen: base.doorOpen,
    recentlyRecovered: tick > 0 && scenes[(tick - 1) % scenes.length]!.freezeMarginF !== null
      && (scenes[(tick - 1) % scenes.length]!.freezeMarginF ?? 99) <= 5
      && (base.freezeMarginF ?? 0) > 5
      && !base.wetContact
      && base.feedHealthy,
  });

  return {
    ...base,
    mood,
    caption: captionFor(mood),
    lastUpdated: new Date().toISOString(),
  };
}

export const DEMO_NEAR_MISSES: NearMiss[] = [
  {
    id: "1",
    at: "Last Tuesday · 2:14 AM",
    kind: "freeze",
    summary: "Corner probe dipped to 34.2°F — door seal fixed next day.",
  },
  {
    id: "2",
    at: "Jan 12 · 6:03 AM",
    kind: "door",
    summary: "Bay door left open overnight during a cold snap.",
  },
  {
    id: "3",
    at: "Dec 3 · 9:41 PM",
    kind: "leak",
    summary: "Wet contact under the heater — drip caught early.",
  },
];
