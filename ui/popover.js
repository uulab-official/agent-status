const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const TRAY_MODES = ["minimal", "compact", "detailed"];

function toneColor(tone) {
  switch (tone) {
    case "critical":
      return "#ef4444";
    case "warning":
      return "#f59e0b";
    default:
      return "#22c55e";
  }
}

// `index.html`'s CSP is `style-src 'self'` (no `unsafe-inline`), which
// silently blocks every HTML `style="..."` attribute from ever taking
// effect — the width/color always fell back to CSS defaults, so every
// "progress bar" in this popover looked like a flat, uncolored track no
// matter what percent/tone was passed in. This went unnoticed because the
// only providers exercised live here (Claude, Codex, Cursor) never had a
// real known-limit bar to compare against. CSP's style-src does NOT block
// script-set styles via the CSSOM (`element.style.foo = ...`), so bar
// fills are rendered as plain `data-percent`/`data-tone` attributes and
// given real width/color here instead of in the HTML template string.
function applyBarFillStyles(root) {
  root.querySelectorAll(".bar-fill[data-percent]").forEach((el) => {
    el.style.width = `${el.dataset.percent}%`;
    el.style.background = toneColor(el.dataset.tone);
  });
}

// Abbreviates large counts the same way the Rust side does for value_text
// (e.g. Claude's token totals), so the peak caption reads "41.2M" not
// "41213024".
function formatCount(value) {
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(1)}B`;
  if (abs >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return `${Math.round(value)}`;
}

// For windows with no known cap (e.g. Claude's token counts), there's no
// real percentage to show — but a bare line chart read as "just a graph,"
// not the progress-bar-at-a-glance the rest of the popover uses. Instead,
// show a real progress bar against the highest reading seen recently: how
// close is *right now* to the peak. Still never a percentage of anything
// invented — it's an honest ratio of two numbers that were both actually
// observed. Values come from `get_usage_history`, fetched separately after
// the main render (see loadRelativeUsageBars) since history isn't part of
// the view model itself.
function renderRelativeUsageBar(valuesNewestFirst) {
  if (valuesNewestFirst.length === 0) return "";
  const current = valuesNewestFirst[0];
  const peak = Math.max(...valuesNewestFirst);
  const percent = peak > 0 ? Math.min(100, Math.round((current / peak) * 100)) : 0;
  const tone = percent >= 90 ? "critical" : percent >= 70 ? "warning" : "ok";
  return `
    <div class="bar-track">
      <div class="bar-fill" data-percent="${percent}" data-tone="${tone}"></div>
    </div>
    <div class="limit-meta">${percent}% of recent peak (${formatCount(peak)})</div>`;
}

// For providers that report zero LimitWindows but are genuinely connected
// (Codex/Cursor/Antigravity: a real CLI login check succeeded, there's just
// no usage/quota API to call) — a plain text block with no visual fill read
// as "broken" next to providers that do have a bar. This isn't a usage
// percentage (there's no number to base one on) — it's just "reachable,"
// shown the same way a known-limit row is so the popover doesn't have two
// visually different classes of "this is fine" rows.
function renderConnectionBar(state) {
  if (state !== "online") return "";
  return `
    <div class="bar-track">
      <div class="bar-fill" data-percent="100" data-tone="ok"></div>
    </div>`;
}

function renderProvider(provider) {
  const limits = provider.limits
    .map(
      (limit) => `
        <div class="limit-row">
          <div class="limit-label">
            <span>${limit.label}</span>
            <span class="limit-percent">${limit.hasLimit ? `${limit.percent}%` : limit.valueText}</span>
          </div>
          ${
            limit.hasLimit
              ? `<div class="bar-track">
                  <div class="bar-fill" data-percent="${limit.percent}" data-tone="${limit.tone}"></div>
                </div>`
              : `<div class="relative-usage-bar" data-provider="${provider.id}" data-window="${limit.id}"></div>`
          }
          ${limit.resetText ? `<div class="limit-meta">${limit.resetText}</div>` : ""}
        </div>`,
    )
    .join("");

  return `
    <section class="provider">
      <header>
        <span class="indicator">${provider.indicator}</span>
        <span class="name">${provider.displayName}</span>
        <span class="state">${provider.state}</span>
      </header>
      ${limits || `${renderConnectionBar(provider.state)}<div class="empty">${provider.detail ?? "No limit data reported"}</div>`}
      ${limits && provider.detail ? `<div class="limit-meta">${provider.detail}</div>` : ""}
      ${provider.costText ? `<div class="cost">${provider.costText}</div>` : ""}
    </section>`;
}

function renderProviders(viewModel) {
  const root = document.getElementById("root");
  if (!root) return;

  if (viewModel.providers.length === 0) {
    root.innerHTML = `<div class="empty-state">No providers detected on this machine yet.</div>`;
    return;
  }

  root.innerHTML = viewModel.providers.map(renderProvider).join("");
  applyBarFillStyles(root);
}

function renderSettings(settings) {
  const container = document.getElementById("settings");
  if (!container) return;

  container.innerHTML = `
    <div class="settings-row">
      <span class="settings-label">Menu bar</span>
      <div class="mode-switch">
        ${TRAY_MODES.map(
          (mode) =>
            `<button data-mode="${mode}" class="${mode === settings.trayMode ? "active" : ""}">${mode}</button>`,
        ).join("")}
      </div>
    </div>
    <label class="settings-row settings-checkbox">
      <span class="settings-label">Launch at Login</span>
      <input type="checkbox" id="launch-at-login" ${settings.launchAtLogin ? "checked" : ""} />
    </label>`;
}

// Recent-peak comparison per no-known-limit window, fetched separately from
// the main view model (history is a growing table, not something
// recomputed on every tick). Fire-and-forget per provider — a slow/failed
// history read for one provider shouldn't block the others' bars from
// appearing.
function loadRelativeUsageBars(viewModel) {
  for (const provider of viewModel.providers) {
    if (provider.limits.length === 0) continue;
    invoke("get_usage_history", { providerId: provider.id })
      .then((history) => {
        const byWindow = new Map();
        for (const row of history.usage) {
          if (!byWindow.has(row.windowId)) byWindow.set(row.windowId, []);
          byWindow.get(row.windowId).push(row.used);
        }
        for (const [windowId, usedNewestFirst] of byWindow) {
          const el = document.querySelector(`.relative-usage-bar[data-provider="${provider.id}"][data-window="${windowId}"]`);
          if (el) {
            el.innerHTML = renderRelativeUsageBar(usedNewestFirst);
            applyBarFillStyles(el);
          }
        }
      })
      .catch(() => {});
  }
}

function render(viewModel) {
  renderProviders(viewModel);
  renderSettings(viewModel.settings);
  loadRelativeUsageBars(viewModel);
}

function refresh() {
  invoke("get_view_model").then(render).catch(() => {});
}

// `type="module"` scripts don't leak top-level declarations onto `window` —
// exposed explicitly so the Rust side can force a real invoke()
// request/response via `window.eval("window.refresh && window.refresh()")`
// right when the popover is shown, instead of relying on the "status-update"
// push event (which can be missed entirely while the window is hidden).
window.refresh = refresh;

// Event delegation on the never-replaced #settings container, so listeners
// survive renderSettings() re-generating its innerHTML on every update.
// Each handler re-fetches the view model directly via invoke() rather than
// relying solely on the pushed "status-update" event — listen()'s
// subscription is itself async (a round-trip to Rust to register), so an
// event emitted right after a command completes can otherwise race ahead of
// that registration and get missed. Same class of bug as the Electron
// version's "first popover open stuck on Loading" fix.
document.getElementById("settings")?.addEventListener("click", (event) => {
  const mode = event.target.dataset.mode;
  if (mode) invoke("set_tray_mode", { mode }).then(refresh);
});

document.getElementById("settings")?.addEventListener("change", (event) => {
  if (event.target.id === "launch-at-login") {
    invoke("set_launch_at_login", { enabled: event.target.checked }).then(refresh);
  }
});

document.getElementById("quit")?.addEventListener("click", () => invoke("quit_app"));
document.getElementById("test-notification")?.addEventListener("click", () => invoke("send_test_notification"));

listen("status-update", (event) => render(event.payload));

// Ask for the current snapshot immediately — don't wait for the next
// scheduler tick, which could be tens of seconds away.
refresh();
