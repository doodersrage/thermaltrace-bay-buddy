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

interface SerialPortInfo {
  path: string;
  name: string;
}

interface ClaimPuckResult {
  deviceId: string;
  bayId: string;
  spaceName: string;
  message: string;
}

interface BayMoodPayload {
  bayId: string;
  mood: string;
  spaceName?: string | null;
  source?: string | null;
}

let demoTick = 0;
let previousMargin: number | null = null;
let state: BuddyState = demoBuddyState(0);
let statusMessage = "";
let connecting = false;
let pollTimer: number | null = null;
let selectedSpace: string | null = null;

let serialPorts: SerialPortInfo[] = [];
let selectedPort = "";
let puckBusy = false;
let followEnabled = false;
let followTimer: number | null = null;
let lastPuckMood: string | null = null;
let claimedBayId: string | null = null;

function bayIdFromSpace(space: string): string {
  return space
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "_")
    .replace(/[^a-z0-9_.:-]/g, "")
    .slice(0, 32);
}

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

function stopFollow() {
  followEnabled = false;
  if (followTimer !== null) {
    window.clearInterval(followTimer);
    followTimer = null;
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
      stopFollow();
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
    await refreshPorts(app, false);
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
  stopFollow();
  await invoke("disconnect_companion");
  previousMargin = null;
  selectedSpace = null;
  claimedBayId = null;
  lastPuckMood = null;
  state = demoBuddyState(0);
  statusMessage = "Disconnected — back to demo mode";
  render(app);
}

async function refreshPorts(app: HTMLElement, showStatus: boolean) {
  try {
    serialPorts = await invoke<SerialPortInfo[]>("list_serial_ports");
    if (!selectedPort || !serialPorts.some((p) => p.path === selectedPort)) {
      const preferred =
        serialPorts.find((p) => /acm|usb|cu\.usb/i.test(p.path)) ?? serialPorts[0];
      selectedPort = preferred?.path ?? "";
    }
    if (showStatus) {
      statusMessage = serialPorts.length
        ? `Found ${serialPorts.length} serial port${serialPorts.length === 1 ? "" : "s"}`
        : "No serial ports found — plug in the RP2040-Zero";
    }
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Port scan failed: ${detail}`;
  }
  render(app);
}

async function claimSelectedPuck(app: HTMLElement) {
  if (!selectedPort) {
    statusMessage = "Pick a serial port first";
    render(app);
    return;
  }
  const bay = bayIdFromSpace(selectedSpace || state.spaceName || "garage");
  puckBusy = true;
  statusMessage =
    "Claiming… when the puck LED turns yellow, press the GP4 button within 30s.";
  render(app);
  try {
    const result = await invoke<ClaimPuckResult>("claim_puck", {
      port: selectedPort,
      bayId: bay,
      spaceName: state.spaceName || bay,
    });
    claimedBayId = result.bayId;
    lastPuckMood = "cozy";
    statusMessage = result.message;
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Claim failed: ${detail}`;
  } finally {
    puckBusy = false;
    render(app);
  }
}

async function followTick(app: HTMLElement) {
  if (!followEnabled || !selectedPort || !claimedBayId) return;
  try {
    const bay = await invoke<BayMoodPayload>("fetch_bay_mood", {
      bayId: claimedBayId,
    });
    if (bay.mood !== lastPuckMood) {
      const resp = await invoke<string>("push_puck_mood", {
        port: selectedPort,
        mood: bay.mood,
      });
      lastPuckMood = bay.mood;
      statusMessage = `Puck ← ${bay.mood}${bay.source ? ` (${bay.source})` : ""} · ${resp}`;
      render(app);
    }
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Follow paused: ${detail}`;
    stopFollow();
    render(app);
  }
}

function toggleFollow(app: HTMLElement) {
  if (followEnabled) {
    stopFollow();
    statusMessage = "Stopped driving the claim puck";
    render(app);
    return;
  }
  if (!selectedPort) {
    statusMessage = "Pick a serial port first";
    render(app);
    return;
  }
  if (!claimedBayId) {
    claimedBayId = bayIdFromSpace(selectedSpace || state.spaceName || "garage");
  }
  followEnabled = true;
  statusMessage = `Driving puck for bay “${claimedBayId}”…`;
  render(app);
  void followTick(app);
  followTimer = window.setInterval(() => {
    void followTick(app);
  }, 2_000);
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

function renderPuckSection(): string {
  if (!state.connected) return "";
  const options = serialPorts
    .map(
      (p) =>
        `<option value="${p.path}" ${p.path === selectedPort ? "selected" : ""}>${p.name}</option>`,
    )
    .join("");
  const bay = claimedBayId || bayIdFromSpace(selectedSpace || state.spaceName || "garage");
  return `
    <section class="puck-panel">
      <h2>Claim puck</h2>
      <p class="puck-hint">
        Plug in an RP2040-Zero running claim-puck firmware. Wire a button from GP4 to GND.
        Bay id: <code>${bay}</code>
      </p>
      <label class="field-label" for="puck-port">Serial port</label>
      <select id="puck-port" ${puckBusy ? "disabled" : ""}>
        <option value="">Select port…</option>
        ${options}
      </select>
      <div class="puck-actions">
        <button type="button" class="btn-secondary" id="btn-ports" ${puckBusy ? "disabled" : ""}>
          Rescan ports
        </button>
        <button type="button" class="btn-secondary" id="btn-claim" ${puckBusy || !selectedPort ? "disabled" : ""}>
          Claim this bay
        </button>
        <button type="button" class="btn-ghost" id="btn-follow" ${puckBusy || !selectedPort ? "disabled" : ""}>
          ${followEnabled ? "Stop driving puck" : "Drive puck mood"}
        </button>
      </div>
      ${
        lastPuckMood
          ? `<p class="puck-status">Last mood on puck: <strong>${lastPuckMood}</strong></p>`
          : ""
      }
    </section>
  `;
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
        <button type="button" class="btn-primary" id="btn-refresh" ${connecting || puckBusy ? "disabled" : ""}>
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

    ${renderPuckSection()}

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
      Bay Buddy is a glanceable companion. Devices, alerts, and history stay on
      <a href="${THERMALTRACE_URL}" id="link-tt">thermaltrace.dev</a>.
      Claim puck binds a physical presence key to this bay.
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
  app.querySelector("#btn-ports")?.addEventListener("click", () => {
    void refreshPorts(app, true);
  });
  app.querySelector("#btn-claim")?.addEventListener("click", () => {
    void claimSelectedPuck(app);
  });
  app.querySelector("#btn-follow")?.addEventListener("click", () => {
    toggleFollow(app);
  });
  app.querySelector("#puck-port")?.addEventListener("change", (e) => {
    selectedPort = (e.target as HTMLSelectElement).value;
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
      if (state.connected) {
        startPolling(app);
        await refreshPorts(app, false);
      }
    } else {
      state = demoBuddyState(0);
      render(app);
    }
  })();
});
