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
                  <div class="bar-fill" style="width:${limit.percent}%;background:${toneColor(limit.tone)}"></div>
                </div>`
              : ""
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
      ${limits || `<div class="empty">${provider.detail ?? "No limit data reported"}</div>`}
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

function render(viewModel) {
  renderProviders(viewModel);
  renderSettings(viewModel.settings);
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
