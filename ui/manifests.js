"use strict";

// Windows game manifests recorded by the Isle map preservation guide linked in README.md.
const archiveReleases = [
  { name: "Region 2 · January 2018", game: "261960182848514966" },
  { name: "Region 2 Redwoods · September 2017", game: "8943967368756614294" },
  { name: "Misty Valley · May 2016", game: "3358028775976615874" },
  { name: "Isla Mesa · December 2015", game: "4566195345998434786" }
].map(entry => ({ ...entry, branch: "public", server: null }));

const cacheKey = "isle-manifest-history-v1";
const maxManifest = 18446744073709551615n;
function isManifest(value) {
  return typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value) && BigInt(value) < maxManifest;
}

function releaseKey(release) {
  return [release.branch, release.game, release.server ?? ""].join(":");
}

function cachedReleases() {
  try {
    const entries = JSON.parse(localStorage.getItem(cacheKey));
    return Array.isArray(entries) ? entries.filter(e => e && typeof e.name === "string" &&
      typeof e.branch === "string" && isManifest(e.game) && (e.server === null || isManifest(e.server))) : [];
  } catch { return []; }
}

async function fetchDepots(appId, depotId) {
  const response = await fetch(`https://api.steamcmd.net/v1/info/${appId}`, {
    signal: AbortSignal.timeout(12000), credentials: "omit", cache: "no-store"
  });
  if (!response.ok) throw new Error(`SteamCMD returned HTTP ${response.status}.`);
  const depots = (await response.json()).data?.[appId]?.depots;
  const manifests = depots?.[depotId]?.manifests;
  if (!manifests || !Object.values(manifests).some(entry => isManifest(entry?.gid))) {
    throw new Error(`SteamCMD returned no public manifests for depot ${depotId}.`);
  }
  return { manifests, branches: depots.branches ?? {} };
}

async function fetchReleases() {
  const [game, server] = await Promise.all([fetchDepots("376210", "376212"), fetchDepots("412680", "412681")]);
  const names = { public: "Legacy", evrima: "Evrima", hordetesting: "Hordetesting", "spiro0.11.59.04": "Isla Spiro 0.11.59.04" };
  return Object.entries(game.manifests).filter(([, info]) => isManifest(info?.gid)).map(([branch, info]) => {
    const build = game.branches[branch]?.buildid;
    const title = Object.hasOwn(names, branch) ? names[branch] : branch;
    const serverId = server.manifests[branch]?.gid;
    return {
      name: `${title} · ${build ? "Build " + build : info.gid}`, branch, game: info.gid,
      server: isManifest(serverId) ? serverId : null
    };
  });
}

function setupManifestPicker(form, getLibrary, getSpiroReleases) {
  const picker = form.querySelector("#release");
  const reload = form.querySelector("#reload-manifests");
  const status = form.querySelector("#catalog-status");
  let current = [];
  let history = cachedReleases();
  let releases = new Map();

  function renderOptions() {
    const previous = picker.value;
    releases = new Map();
    picker.replaceChildren(new Option("Choose a release or enter IDs below", ""));
    const groups = [
      ["Evrima · Isla Spiro updates", getSpiroReleases()],
      ["Current Steam branches", current],
      ["Previously loaded", history],
      ["Older game builds", archiveReleases],
      ["In your library", getLibrary()]
    ];
    for (const [label, entries] of groups) {
      const group = document.createElement("optgroup");
      group.label = label;
      for (const entry of entries) {
        const key = releaseKey(entry);
        if (releases.has(key)) continue;
        releases.set(key, entry);
        let text = entry.name;
        if (entry.date) {
          text = text.replace(/^Isla Spiro · /, "");
          if (!text.startsWith(entry.date)) text = `${entry.date} · ${text}`;
        }
        if (!entry.game) text += " (server only)";
        group.append(new Option(text, key));
      }
      if (group.children.length) picker.append(group);
    }
    picker.value = releases.has(previous) ? previous : "";
  }

  async function load() {
    if (reload.disabled) return;
    reload.disabled = true;
    status.textContent = "Loading Steam branch manifests…";
    try {
      current = await fetchReleases();
      const merged = new Map();
      for (const entry of [...current, ...history]) {
        const key = releaseKey(entry);
        if (!merged.has(key)) merged.set(key, entry);
      }
      history = Array.from(merged.values()).slice(0, 500);
      try { localStorage.setItem(cacheKey, JSON.stringify(history)); } catch { /* The session still has the entries. */ }
      status.textContent = `Loaded ${current.length} public branches from SteamCMD.`;
    } catch (error) {
      current = [];
      status.textContent = `Live lookup unavailable. Saved and bundled releases are available. ${error.message}`;
    } finally {
      renderOptions();
      reload.disabled = false;
    }
  }

  picker.addEventListener("change", () => {
    const entry = releases.get(picker.value);
    if (!entry) return;
    for (const field of ["name", "branch", "game", "server"]) form.elements[field].value = entry[field] ?? "";
    form.querySelector(".dialog-error").textContent = "";
  });
  for (const field of ["game", "server", "branch"]) {
    form.elements[field].addEventListener("input", () => {
      const previous = releases.get(picker.value);
      if (previous && (field === "branch" || (field === "game" && previous.game))) form.elements.server.value = "";
      picker.value = "";
    });
  }
  for (const [field, app, depot] of [["game", "376210", "376212"], ["server", "412680", "412681"]]) {
    form.elements[field].addEventListener("paste", event => {
      const text = event.clipboardData?.getData("text").trim() ?? "";
      if (!text.startsWith("download_depot")) return;
      event.preventDefault();
      const [command, appId, depotId, id, extra] = text.split(/\s+/);
      const error = form.querySelector(".dialog-error");
      if (command !== "download_depot" || appId !== app || depotId !== depot || extra || !isManifest(id)) {
        error.textContent = `Choose a Windows ${field} manifest for depot ${depot}.`;
        return;
      }
      form.elements[field].value = id;
      form.elements[field].dispatchEvent(new Event("input"));
      error.textContent = "";
    });
  }
  reload.addEventListener("click", load);
  return () => {
    renderOptions();
    if (!current.length) void load();
  };
}
