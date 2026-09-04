import { invoke } from "@tauri-apps/api/core";
import { DEMO_NEAR_MISSES, demoBuddyState } from "./mood";
import type { BuddyState, NearMiss } from "./types";

const THERMALTRACE_URL = "https://thermaltrace.dev";
const CONNECT_URL = "https://thermaltrace.dev/signin";
const ALERTS_URL = "https://thermaltrace.dev/dashboard/alerts";

let demoTick = 0;
let state: BuddyState = demoBuddyState(0);
let statusMessage = "";

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

async function openExternal(url: string, app: HTMLElement) {
  statusMessage = "";
  try {
    await invoke("open_external", { url });
    statusMessage = `Opening ${url}`;
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    statusMessage = `Couldn’t open browser. Copy this URL: ${url} (${detail})`;
    console.error("open_external failed", err);
  }
  render(app);
  if (statusMessage.startsWith("Opening ")) {
    window.setTimeout(() => {
      statusMessage = "";
      render(app);
    }, 3500);
  }
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
      <button type="button" class="btn-primary" id="btn-connect">
        ${state.connected ? "Open ThermalTrace" : "Connect ThermalTrace"}
      </button>
      <button type="button" class="btn-secondary" id="btn-demo">
        Cycle demo mood
      </button>
      <button type="button" class="btn-ghost" id="btn-alerts">
        Alert settings on thermaltrace.dev
      </button>
      ${
        statusMessage
          ? `<p class="action-status" role="status">${statusMessage}</p>`
          : ""
      }
    </div>

    <section>
      <h2>Near-miss reel</h2>
      <ul class="near-misses">
        ${renderNearMisses(DEMO_NEAR_MISSES)}
      </ul>
    </section>

    <p class="footer-note">
      Bay Buddy is a glanceable companion. Devices, alerts, history, and claims stay on
      <a href="${THERMALTRACE_URL}" id="link-tt">thermaltrace.dev</a>.
    </p>
  `;

  app.querySelector("#btn-connect")?.addEventListener("click", () => {
    void openExternal(state.connected ? THERMALTRACE_URL : CONNECT_URL, app);
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
  state = demoBuddyState(0);
  render(app);
});
