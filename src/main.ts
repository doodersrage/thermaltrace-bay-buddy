import { invoke } from "@tauri-apps/api/core";
import { DEMO_NEAR_MISSES, captionFor, demoBuddyState, resolveMood } from "./mood";
import type { BuddyState, NearMiss } from "./types";

const THERMALTRACE_URL = "https://thermaltrace.dev";
const ALERTS_URL = "https://thermaltrace.dev/dashboard/alerts";

interface LiveBuddyPayload {
  connected: boolean;
  spaceName: string;
  temperatureF: number | null;
  freezeThresholdF: number;
  freezeMarginF: number | null;
  timeToFreezeHours: number | null;
  doorOpen: boolean;
  wetContact: boolean;
  feedHealthy: boolean;
  spaces: string[];
  lastUpdated: string;
}

let demoTick = 0;
let previousMargin: number | null = null;
let state: BuddyState = demoBuddyState(0);
let statusMessage = "";
let connecting = false;
let pollTimer: number | null = null;
let selectedSpace: string | null = null;

function formatTemp(f: number | null): string {
  if (f === null) return "—";
  return `${f.toFixed(1)}°F`;
}

function formatMargin(f: number | null): string {
  if (f === null) return "—";
  const sign = f > 0 ? "+" : "";
  return `${sign}${f.toFixed(1)}°F`;
}

function formatHours(h: number | null): string {
  if (h === null) return "—";
  if (h < 1) return `${Math.round(h * 60)}m`;
  return `${h.toFixed(1)}h`;
}

function liveToBuddy(payload: LiveBuddyPayload): BuddyState {
  const recentlyRecovered =
    previousMargin !== null &&
    previousMargin <= 5 &&
    (payload.freezeMarginF ?? 99) > 5 &&
    !payload.wetContact &&
    payload.feedHealthy;

  previousMargin = payload.freezeMarginF;

  const mood = resolveMood({
    wetContact: payload.wetContact,
    feedHealthy: payload.feedHealthy,
    freezeMarginF: payload.freezeMarginF,
    doorOpen: payload.doorOpen,
    recentlyRecovered,
  });

  return {
    spaceName: payload.spaceName,
    connected: true,
    temperatureF: payload.temperatureF,
    freezeThresholdF: payload.freezeThresholdF,
    freezeMarginF: payload.freezeMarginF,
    timeToFreezeHours: payload.timeToFreezeHours,
    doorOpen: payload.doorOpen,
    wetContact: payload.wetContact,
    feedHealthy: payload.feedHealthy,
    mood,
    caption: captionFor(mood),
    lastUpdated: payload.lastUpdated,
  };
}

async function openExternal(url: string, app: HTMLElement) {
  statusMessage = "";
  try {
    await invoke("open_external", { url });
    statusMessage = `Opening ${url}`;
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Couldn’t open browser. Copy this URL: ${url} (${detail})`;
  }
  render(app);
  if (statusMessage.startsWith("Opening ")) {
    window.setTimeout(() => {
      statusMessage = "";
      render(app);
    }, 3500);
  }
}

function stopPolling() {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
}

function startPolling(app: HTMLElement) {
  stopPolling();
  pollTimer = window.setInterval(() => {
    void refreshLive(app, false);
  }, 45_000);
}

async function refreshLive(app: HTMLElement, showStatus: boolean) {
  try {
    const payload = await invoke<LiveBuddyPayload>("fetch_buddy_state", {
      space: selectedSpace,
    });
    state = liveToBuddy(payload);
    if (showStatus) statusMessage = "Live readings updated";
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    if (detail.toLowerCase().includes("session expired")) {
      stopPolling();
      state = demoBuddyState(0);
      statusMessage = "Session expired — connect again";
    } else if (showStatus) {
      statusMessage = `Couldn’t refresh: ${detail}`;
    }
  }
  render(app);
}

async function connect(app: HTMLElement, provider?: string) {
  connecting = true;
  statusMessage = "Waiting for browser sign-in… finish there, then we’ll pull you back.";
  render(app);
  try {
    const payload = await invoke<LiveBuddyPayload>("start_companion_login", {
      provider: provider ?? null,
    });
    state = liveToBuddy(payload);
    selectedSpace = payload.spaceName;
    statusMessage = `Connected to ${payload.spaceName}`;
    startPolling(app);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Connect failed: ${detail}`;
  } finally {
    connecting = false;
    render(app);
  }
}

async function disconnect(app: HTMLElement) {
  stopPolling();
  await invoke("disconnect_companion");
  previousMargin = null;
  selectedSpace = null;
  state = demoBuddyState(0);
  statusMessage = "Disconnected — back to demo mode";
  render(app);
}

function renderNearMisses(items: NearMiss[]): string {
  return items
    .map(
      (item) => `
      <li>
        <span class="kind kind-${item.kind}">${item.kind}</span>
        <span class="when">${item.at}</span>
        <p class="summary">${item.summary}</p>
      </li>`,
    )
    .join("");
}

function render(app: HTMLElement) {
  const connectedLabel = state.connected ? "Linked to ThermalTrace" : "Demo mode";

  app.innerHTML = `
    <header>
      <div class="brand" aria-label="Bay Buddy">
        <span class="bay">Bay</span><span class="buddy">Buddy</span>
      </div>
      <p class="brand-sub">Companion for ThermalTrace freeze &amp; flood watches</p>
      <div class="status-pill" data-connected="${state.connected}" style="margin-top:0.65rem">
        <span class="dot" aria-hidden="true"></span>
        ${connectedLabel}
      </div>
    </header>

    <section class="buddy-stage" data-mood="${state.mood}" aria-live="polite">
      <div class="buddy-face" aria-hidden="true">
        <div>
          <div class="eyes"><span class="eye"></span><span class="eye"></span></div>
          <div class="mouth"></div>
        </div>
      </div>
      <p class="mood-label">${state.spaceName} · ${state.mood}</p>
      <p class="caption">${state.caption}</p>
      <dl class="metrics">
        <div class="metric">
          <dt>Probe</dt>
          <dd>${formatTemp(state.temperatureF)}</dd>
        </div>
        <div class="metric">
          <dt>Freeze margin</dt>
          <dd>${formatMargin(state.freezeMarginF)}</dd>
        </div>
        <div class="metric">
          <dt>Time to freeze</dt>
          <dd>${formatHours(state.timeToFreezeHours)}</dd>
        </div>
        <div class="metric">
          <dt>Threshold</dt>
          <dd>${state.freezeThresholdF}°F</dd>
        </div>
      </dl>
    </section>

    <div class="actions">
      ${
        state.connected
          ? `
        <button type="button" class="btn-primary" id="btn-refresh" ${connecting ? "disabled" : ""}>
          Refresh live mood
        </button>
        <button type="button" class="btn-secondary" id="btn-alerts">
          Alert settings on thermaltrace.dev
        </button>
        <button type="button" class="btn-ghost" id="btn-disconnect">
          Disconnect
        </button>`
          : `
        <p class="connect-hint">Sign in to ThermalTrace — we’ll open your browser, then return here with live readings.</p>
        <button type="button" class="btn-primary" id="btn-google" ${connecting ? "disabled" : ""}>
          Connect with Google
        </button>
        <button type="button" class="btn-secondary" id="btn-github" ${connecting ? "disabled" : ""}>
          Connect with GitHub
        </button>
        <button type="button" class="btn-secondary" id="btn-email" ${connecting ? "disabled" : ""}>
          Connect with email in browser
        </button>
        <button type="button" class="btn-ghost" id="btn-demo" ${connecting ? "disabled" : ""}>
          Cycle demo mood
        </button>`
      }
      ${
        statusMessage
          ? `<p class="action-status" role="status">${statusMessage}</p>`
          : ""
      }
    </div>

    ${
      state.connected
        ? ""
        : `<section>
      <h2>Near-miss reel</h2>
      <ul class="near-misses">
        ${renderNearMisses(DEMO_NEAR_MISSES)}
      </ul>
    </section>`
    }

    <p class="footer-note">
      Bay Buddy is a glanceable companion. Devices, alerts, history, and claims stay on
      <a href="${THERMALTRACE_URL}" id="link-tt">thermaltrace.dev</a>.
    </p>
  `;

  app.querySelector("#btn-google")?.addEventListener("click", () => {
    void connect(app, "google");
  });
  app.querySelector("#btn-github")?.addEventListener("click", () => {
    void connect(app, "github");
  });
  app.querySelector("#btn-email")?.addEventListener("click", () => {
    void connect(app);
  });
  app.querySelector("#btn-refresh")?.addEventListener("click", () => {
    void refreshLive(app, true);
  });
  app.querySelector("#btn-disconnect")?.addEventListener("click", () => {
    void disconnect(app);
  });
  app.querySelector("#btn-alerts")?.addEventListener("click", () => {
    void openExternal(ALERTS_URL, app);
  });
  app.querySelector("#link-tt")?.addEventListener("click", (e) => {
    e.preventDefault();
    void openExternal(THERMALTRACE_URL, app);
  });
  app.querySelector("#btn-demo")?.addEventListener("click", () => {
    demoTick += 1;
    state = demoBuddyState(demoTick);
    render(app);
  });
}

window.addEventListener("DOMContentLoaded", () => {
  const app = document.querySelector<HTMLElement>("#app");
  if (!app) return;

  void (async () => {
    const hasSession = await invoke<boolean>("has_companion_session");
    if (hasSession) {
      statusMessage = "Restoring ThermalTrace session…";
      render(app);
      await refreshLive(app, false);
      if (state.connected) startPolling(app);
    } else {
      state = demoBuddyState(0);
      render(app);
    }
  })();
});
