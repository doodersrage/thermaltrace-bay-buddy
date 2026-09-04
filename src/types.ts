export type Mood =
  | "cozy"
  | "drafty"
  | "shiver"
  | "panic"
  | "offline"
  | "hero";

export interface BuddyState {
  spaceName: string;
  connected: boolean;
  temperatureF: number | null;
  freezeThresholdF: number;
  freezeMarginF: number | null;
  timeToFreezeHours: number | null;
  doorOpen: boolean;
  wetContact: boolean;
  feedHealthy: boolean;
  mood: Mood;
  caption: string;
  lastUpdated: string;
}

export interface NearMiss {
  id: string;
  at: string;
  kind: "freeze" | "leak" | "door";
  summary: string;
}
