"use strict";

const $ = (selector) => document.querySelector(selector);
const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
const labels = {
  installed: "Installed", downloading: "Downloading", available: "Available",
  incomplete: "Incomplete", unavailable: "No manifest"
};
let view = "game";
let snapshot;
let spiroReleases = [];
let refreshId = 0;
let pending = null;
let configDraft;
let editingIndex;

function announce(message = "", error = false) {
  $("#notice").textContent = message;
  $("#notice").classList.toggle("error", error);
}

function report(error) {
  const dialog = $("dialog[open] .dialog-error");
  if (dialog) dialog.textContent = String(error);
  else announce(String(error), true);
}

async function refresh() {
  const id = ++refreshId;
  const data = await invoke("snapshot");
  if (id !== refreshId) return;
  const changed = JSON.stringify(data) !== JSON.stringify(snapshot);
  snapshot = data;
  if (changed) render();
}

function button(label, action, disabled = false) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  el.disabled = disabled;
  handle(el, "click", action);
  return el;
}

function renderDownloads() {
  const { library, statuses } = snapshot;
  const busy = pending !== null || statuses.some(pair => pair.includes("downloading"));
  const rows = library.versions.map((version, index) => {
    const row = document.createElement("tr");
    const name = row.insertCell();
    name.append(version.name);
    const branch = document.createElement("small");
    branch.textContent = version.branch;
    name.append(branch);
    const preset = spiroReleases.find(p => p.name === version.name && p.branch === version.branch);
    if (preset && (preset.game !== version.game || preset.server !== version.server)) {
      const repair = button("Use preset manifests", async () => {
        await invoke("use_preset_manifests", { index, name: version.name });
        await refresh();
        announce("Applied the preset manifests.");
      }, busy);
      repair.className = "link";
      name.append(repair);
    }
    ["game", "server"].forEach((component, column) => {
      const cell = row.insertCell();
      const active = pending?.index === index && pending.component === component;
      const status = active ? "downloading" : statuses[index][column];
      if (status === "available" || status === "incomplete") {
        const label = status === "incomplete" ? "Resume" : "Download";
        const download = button(label, () => downloadVersion(index, component, version), busy || !snapshot.tool_available);
        download.setAttribute("aria-label", label + " " + version.name + " " + component);
        cell.append(download);
      } else {
        cell.className = status;
        cell.textContent = labels[status];
      }
    });
    return row;
  });
  $("#versions").replaceChildren(...rows);
  $("#setup-hint").hidden = snapshot.tool_available;
  $("#add").disabled = false;
}

function renderInstances() {
  const game = view === "game";
  const entries = game ? snapshot.games : snapshot.servers;
  $("#instances table").hidden = entries.length === 0;
  $("#empty-instances").hidden = $("#get-instances").hidden = entries.length > 0;
  $("#empty-instances").textContent = game ? "No installed games in this builds folder." : "No installed servers in this builds folder.";
  const rows = entries.map(instance => {
    const row = document.createElement("tr");
    const name = row.insertCell();
    name.append(instance.name);
    const info = document.createElement("small");
    info.textContent = [instance.branch, instance.manifest].filter(Boolean).join(" · ");
    name.append(info);
    const actions = document.createElement("div");
    actions.className = "row-actions";
    const target = { component: view, manifest: instance.manifest, index: instance.profile };
    const start = button(game ? "Play" : "Start server", async () => {
      await invoke("launch", target);
      announce("Started " + instance.name + ".");
    });
    start.className = "primary";
    actions.append(start);
    if (!game) {
      actions.append(button("Configure", () => openServerConfig(instance)));
      actions.append(button("Steam Favorites", () => openFavorite(instance)));
      if (instance.profile !== null) actions.append(button("Arguments", () => openArguments(instance.profile)));
    }
    actions.append(button("Open folder", () => invoke("open_installation", target)));
    row.insertCell().append(actions);
    return row;
  });
  $("#installed-rows").replaceChildren(...rows);
}

const pages = {
  game: ["Play", "Launch a game installed in your builds folder."],
  server: ["Dedicated server", "Configure and start an installed server."],
  downloads: ["Downloads", "Download or resume game and server files for a release."],
  settings: ["Settings", "Manage storage and Steam downloads."]
};

function render() {
  if (!snapshot) return;
  for (const item of document.querySelectorAll("aside [data-view]")) {
    if (item.dataset.view === view) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  [$("#heading").textContent, $("#intro").textContent] = pages[view];
  $("#instances").hidden = view !== "game" && view !== "server";
  $("#build-view").hidden = view !== "downloads";
  $("#settings").hidden = view !== "settings";
  if (view === "downloads") renderDownloads();
  else if (view !== "settings") renderInstances();
}

function showView(next) {
  if (!snapshot) return;
  view = next;
  announce();
  if (view === "settings") {
    $("#library-path").value = snapshot.library.folder;
    $("#tool-path").value = snapshot.library.downloader;
    $("#username").value = snapshot.library.username;
  }
  render();
  void refresh().catch(report);
}

async function downloadVersion(index, component, version) {
  if (pending) return;
  const name = version.name;
  pending = { index, component };
  render();
  announce("Opening the download window for " + name + ".");
  try {
    await invoke("download_build", { index, component, manifest: version[component] });
    announce("Installed " + name + " " + component + ".");
  } finally {
    pending = null;
    await refresh();
    render();
  }
}

async function openServerConfig(instance) {
  const data = await invoke("read_server_config", { manifest: instance.manifest });
  configDraft = { data, instance };
  $("#config-version").textContent = instance.name;
  $("#config-text").value = data.text;
  $("#admin-id").value = "";
  $("#admin-feedback").textContent = "";
  $("#my-id").hidden = !data.steam_id;
  $("#config-editor .dialog-error").textContent = "";
  $("#config-editor").showModal();
}

let favoriteRequest = 0;
let verifiedAddress = "";

function resetFavorite() {
  favoriteRequest++;
  verifiedAddress = "";
  $("#favorite-port").value = "";
  $("#favorite-save").disabled = true;
  $("#favorite-check").disabled = false;
  $("#favorite-result").textContent = "";
  $("#favorite-editor .dialog-error").textContent = "";
}

async function checkFavorite() {
  resetFavorite();
  const request = favoriteRequest;
  const address = $("#favorite-address").value.trim();
  $("#favorite-check").disabled = true;
  $("#favorite-result").textContent = "Checking the Steam query port…";
  try {
    const info = await invoke("check_isle_server", { address });
    if (request !== favoriteRequest) return;
    verifiedAddress = address;
    $("#favorite-port").value = info.game_port ?? "";
    $("#favorite-result").textContent = [info.map, info.description].filter(Boolean).join(" · ") +
      (info.game_port ? "" : " — enter the server’s game port below.");
    $("#favorite-save").disabled = false;
  } catch (error) {
    if (request === favoriteRequest) {
      $("#favorite-result").textContent = "";
      $("#favorite-editor .dialog-error").textContent = String(error);
    }
  } finally {
    if (request === favoriteRequest) $("#favorite-check").disabled = false;
  }
}

async function openFavorite(instance) {
  const port = await invoke("local_query_port", { manifest: instance.manifest, index: instance.profile });
  $("#favorite-instance").textContent = instance.name;
  $("#favorite-address").value = "127.0.0.1:" + port;
  $("#favorite-editor").showModal();
  await checkFavorite();
}

function openArguments(index) {
  editingIndex = index;
  const version = snapshot.library.versions[index];
  $("#args-version").textContent = version.name;
  $("#args").value = version.arguments;
  $("#options .dialog-error").textContent = "";
  $("#options").showModal();
}

function handle(element, event, callback) {
  element.addEventListener(event, async (e) => {
    if (event === "submit") e.preventDefault();
    try { await callback(e); } catch (error) { report(error); }
  });
}

for (const button of document.querySelectorAll("[data-view]")) {
  handle(button, "click", () => showView(button.dataset.view));
}
const openManifestPicker = setupManifestPicker($("#add-form"), () => snapshot.library.versions, () => spiroReleases);
handle($("#add"), "click", () => {
  $("#add-form").reset();
  openManifestPicker();
  $("#editor .dialog-error").textContent = "";
  $("#editor").showModal();
});
handle($("#my-id"), "click", () => { $("#admin-id").value = configDraft.data.steam_id; });
handle($("#favorite-address"), "input", resetFavorite);
handle($("#favorite-editor"), "close", resetFavorite);
handle($("#favorite-check"), "click", checkFavorite);
handle($("#favorite-form"), "submit", async () => {
  const address = $("#favorite-address").value.trim();
  if (!verifiedAddress || address !== verifiedAddress) throw new Error("Check the server address first.");
  const request = favoriteRequest;
  $("#favorite-save").disabled = true;
  try {
    await invoke("add_steam_favorite", { address, gamePort: Number($("#favorite-port").value) });
    if (request !== favoriteRequest) return;
    $("#favorite-editor").close();
    announce(address + " is in Steam Favorites. Launch the matching release from Play.");
  } catch (error) {
    if (request === favoriteRequest) {
      $("#favorite-save").disabled = false;
      throw error;
    }
  }
});
handle($("#add-admin"), "click", () => {
  const field = $("#config-text");
  const id = $("#admin-id").value.trim();
  const next = addServerAdmin(field.value, id, configDraft.instance.branch);
  field.value = next;
  $("#config-editor .dialog-error").textContent = "";
  $("#admin-feedback").textContent = "Steam ID " + id + " is in the draft. Save to apply.";
});
handle($("#config-form"), "submit", async () => {
  const { instance, data } = configDraft;
  await invoke("save_server_config", { manifest: instance.manifest, path: data.path, original: data.text, existed: data.exists, edited: $("#config-text").value });
  $("#config-editor").close();
  announce("Configuration saved for " + instance.name + ".");
});
for (const button of document.querySelectorAll("[data-close]")) {
  handle(button, "click", () => button.closest("dialog").close());
}
for (const button of document.querySelectorAll("[data-history]")) {
  handle(button, "click", () => invoke("open_history", { component: button.dataset.history }));
}
handle($("#add-form"), "submit", async () => {
  const form = new FormData($("#add-form"));
  const game = form.get("game").trim() || null;
  const server = form.get("server").trim() || null;
  if (!game && !server) throw new Error("This release needs a game or server manifest ID. Browse its depot history to choose one.");
  await invoke("add_version", {
    name: form.get("name").trim(), branch: form.get("branch").trim(),
    game, server
  });
  $("#editor").close();
  announce();
  await refresh();
});
handle($("#args-form"), "submit", async () => {
  await invoke("save_arguments", { index: editingIndex, arguments: $("#args").value });
  $("#options").close();
  await refresh();
  announce("Server arguments saved.");
});
handle($("#settings"), "submit", async () => {
  await invoke("save_settings", {
    folder: $("#library-path").value.trim(), downloader: $("#tool-path").value.trim(),
    username: $("#username").value.trim()
  });
  await refresh();
  announce("Settings saved.");
});

invoke("spiro_presets").then(releases => { spiroReleases = releases; }).then(refresh).catch(report);
// Also picks up transfers still running after the launcher was closed and reopened.
window.addEventListener("focus", () => refresh().catch(report));
window.setInterval(() => {
  if (pending || snapshot?.statuses.some((pair) => pair.includes("downloading"))) {
    refresh().catch(report);
  }
}, 1500);
