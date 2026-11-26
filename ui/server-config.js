"use strict";

function addServerAdmin(text, steamId, branch) {
  if (!/^[0-9]{17}$/.test(steamId) || BigInt(steamId) <= 76561197960265728n ||
      BigInt(steamId) > 76561202255233023n) throw new Error("Enter an individual account’s 17-digit SteamID64.");
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const legacy = /^\s*\[\/script\/theisle\.igamesession\]\s*$/im.test(text) ||
    (branch === "public" && !/^\s*\[\/script\/theisle\.ti(game(session|statebase))\]\s*$/im.test(text));
  const section = legacy ? "/Script/TheIsle.IGameSession" : "/Script/TheIsle.TIGameStateBase";
  const key = legacy ? "ServerAdmins" : "AdminsSteamIDs";
  let inside = false, insertAt = -1, present = false;
  lines.forEach((line, i) => {
    const header = /^\s*\[([^\]]+)\]/.exec(line);
    if (header) inside = header[1].toLowerCase() === section.toLowerCase();
    if (!inside) return;
    insertAt = i + 1;
    const setting = /^\s*([+.!-]?)(\w+)\s*=\s*"?([^";\s]*)/.exec(line);
    if (!setting || setting[2].toLowerCase() !== key.toLowerCase()) return;
    if (setting[1] === "!") present = false;
    else if (setting[3] === steamId) present = setting[1] !== "-";
  });
  if (present) return text;
  const entry = (legacy ? "+" : "") + key + "=" + steamId;
  if (insertAt === -1) return text + (text && !text.endsWith("\n") ? "\n" : "") + (text ? "\n" : "") + "[" + section + "]\n" + entry + "\n";
  while (insertAt > 0 && !lines[insertAt - 1].trim()) insertAt--;
  lines.splice(insertAt, 0, entry);
  return lines.join("\n");
}
