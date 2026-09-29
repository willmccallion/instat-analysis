"use strict";

const { el, css, fmt, pct, signed, clock, minutes, SERIES, Tooltip, hBarChart, lineChart, groupedColumns, heatmap, shiftChart, networkChart, scatterChart, percentileBars, dataTable, chartCard } = window.Charts;

const params = new URLSearchParams(location.search);
const snapshot = window.HOCKEY_SNAPSHOT || null;

const state = {
  token: params.get("t") || "",
  analysis: snapshot,
  request: snapshot ? snapshot.request : { games: [], focus: null, min_minutes: 10, min_unit_minutes: 3 },
  view: "overview",
  player: null,
  goalie: null,
  unitTab: "defence_pairs",
  chemistryMetric: "toi",
  pair: null,
  gameFocus: null,
  customGames: null,
  uploadLog: [],
  pending: [],
  problems: [],
};

const VIEWS = [
  { group: "Start", id: "games", label: "Games & uploads" },
  { group: "Start", id: "overview", label: "Overview" },
  { group: "Start", id: "game", label: "Single game" },
  { group: "Lines", id: "lines", label: "Lines & pairs" },
  { group: "Lines", id: "chemistry", label: "Pair chemistry" },
  { group: "Lines", id: "passing", label: "Passing network" },
  { group: "Players", id: "players", label: "Players" },
  { group: "Players", id: "goalies", label: "Goalies" },
  { group: "Players", id: "impact", label: "Individual impact" },
  { group: "Players", id: "profiles", label: "Player styles" },
  { group: "Team", id: "team", label: "Team" },
  { group: "Team", id: "advanced", label: "Advanced statistics" },
  { group: "Team", id: "help", label: "How to read this" },
];

const VERDICT_TEXT = {
  LikelyReal: "Likely real",
  Maybe: "Maybe — keep watching",
  CouldBeNoise: "Could be noise",
  NotEnoughData: "Not enough data yet",
};

function verdictBadge(verdict) {
  return el("span", { class: `badge verdict-${verdict}` }, [el("span", { class: "dot" }), VERDICT_TEXT[verdict] || verdict]);
}

function probabilityWords(p) {
  if (p === null || p === undefined) return "—";
  if (p >= 0.9) return `very likely above average (${Math.round(p * 100)}%)`;
  if (p >= 0.75) return `probably above average (${Math.round(p * 100)}%)`;
  if (p <= 0.1) return `very likely below average (${Math.round((1 - p) * 100)}%)`;
  if (p <= 0.25) return `probably below average (${Math.round((1 - p) * 100)}%)`;
  return `can't tell yet (${Math.round(p * 100)}% above)`;
}

function names(players) {
  return players.map((p) => p.name).join(" · ");
}

function shortNames(players) {
  return players.map((p) => p.name.split(" ").slice(-1)[0]).join("–");
}

function positionShort(position) {
  return { Defence: "D", Forward: "F", Goalie: "G" }[position] || "?";
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { "X-Hockey-Token": state.token, ...(options.headers || {}) },
  });
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = text;
  }
  if (!response.ok) throw new Error(body && body.error ? body.error : `request failed (${response.status})`);
  return body;
}

async function refresh() {
  if (snapshot) {
    render();
    return;
  }
  const main = document.getElementById("content");
  main.classList.add("busy");
  try {
    const status = await api("/api/state");
    state.pending = status.pending;
    state.problems = status.problems;
    state.analysis = await api("/api/analyze", { method: "POST", body: JSON.stringify(state.request) });
  } catch (error) {
    state.error = error.message;
  } finally {
    main.classList.remove("busy");
  }
  if (state.analysis && state.analysis.games.length === 0 && state.view !== "help") state.view = "games";
  render();
}

function setView(view) {
  state.view = view;
  if (location.hash !== `#${view}`) history.replaceState(null, "", `${location.pathname}${location.search}#${view}`);
  state.player = view === "players" ? state.player : null;
  Tooltip.hide();
  render();
  window.scrollTo(0, 0);
}

function renderSidebar() {
  const nav = document.getElementById("nav");
  nav.replaceChildren();
  let group = null;
  for (const view of VIEWS) {
    if (snapshot && view.id === "games") continue;
    if (view.group !== group) {
      group = view.group;
      nav.append(el("div", { class: "nav-group", text: group }));
    }
    nav.append(el("button", { class: `nav-item ${state.view === view.id ? "active" : ""}`, text: view.label, onclick: () => setView(view.id) }));
  }
  const footer = document.getElementById("sidebar-footer");
  footer.replaceChildren();
  if (!snapshot) {
    footer.append(
      el("button", { text: "Save report (HTML)", onclick: exportReport, title: "Download a single file you can email or open anywhere" }),
      el("button", { text: "Quit app", onclick: quitApp }),
    );
  } else {
    footer.append(el("div", { class: "small muted", text: "Saved report — scope is fixed." }));
  }
  document.getElementById("team-name").textContent = state.analysis?.team_name || "Hockey Stats";
}

function renderFilters() {
  const bar = document.getElementById("filters");
  bar.replaceChildren();
  const a = state.analysis;
  if (!a || a.games.length === 0) return;
  const request = state.request;
  const single = request.games.length === 1 ? request.games[0] : null;
  const custom = request.games.length > 1;
  const setScope = (games, focus = null) => {
    state.request = { ...request, games, focus };
    refresh();
  };
  const segmented = el("div", { class: "segmented" }, [
    el("button", { class: !single && !custom ? "on" : "", text: "Season", onclick: () => setScope([]) }),
    el("button", { class: single ? "on" : "", text: "Single game", onclick: () => setScope([a.games[a.games.length - 1].id]) }),
    el("button", { class: custom ? "on" : "", text: `Custom${custom ? ` (${request.games.length})` : ""}`, onclick: () => setView("games") }),
  ]);
  if (snapshot) segmented.querySelectorAll("button").forEach((b) => { b.disabled = true; });
  bar.append(el("label", {}, ["Scope", segmented]));
  if (single) {
    const select = el("select", { onchange: (e) => setScope([e.target.value]), disabled: !!snapshot },
      a.games.map((g) => el("option", { value: g.id, selected: g.id === single, text: `${g.date} vs ${g.opponent} (${g.goals_for}-${g.goals_against})` })));
    bar.append(select);
  }
  const minInput = el("input", { type: "number", min: 0, max: 600, step: 5, value: request.min_minutes, style: "width:72px", disabled: !!snapshot });
  minInput.addEventListener("change", () => {
    state.request = { ...request, min_minutes: Math.max(0, Number(minInput.value) || 0) };
    refresh();
  });
  bar.append(el("label", { title: "Players below this total ice time are shown but not ranked" }, ["Min. minutes to rank", minInput]));
  bar.append(el("span", { class: "spacer" }));
  const inScope = a.games.filter((g) => g.in_scope).length;
  bar.append(el("span", { class: "muted small", text: `${inScope} of ${a.games.length} game${a.games.length === 1 ? "" : "s"} in scope` }));
}

function page(title, lede, ...children) {
  return [el("h1", { text: title }), lede ? el("p", { class: "lede", text: lede }) : null, ...children];
}

function tiles(items) {
  return el("div", { class: "tiles" }, items.map((t) => el("div", { class: "tile" }, [
    el("div", { class: "label", text: t.label }),
    el("div", { class: "value", text: t.value }),
    t.note ? el("div", { class: "note", text: t.note }) : null,
  ])));
}

function smallSampleNote(a) {
  const n = a.team.games;
  if (n >= 10) return null;
  return el("div", { class: "warning-box" }, [
    `Only ${n} game${n === 1 ? "" : "s"} loaded. Numbers are real but noisy: the shrunk estimates, intervals and "likely real / could be noise" verdicts already account for that, and the win and opponent models switch on at 10 and 5 games.`,
  ]);
}

// ---------- Games & uploads ----------

function viewGames() {
  const a = state.analysis;
  const log = el("div", { class: "upload-log" }, state.uploadLog.map((line) => el("div", { class: line.ok ? "ok" : "err", text: line.text })));
  const input = el("input", { type: "file", accept: "application/pdf,.pdf", multiple: true, style: "display:none" });
  input.addEventListener("change", () => uploadFiles([...input.files]));
  const folder = el("input", { type: "file", webkitdirectory: true, style: "display:none" });
  folder.addEventListener("change", () => uploadFiles([...folder.files].filter((f) => f.name.toLowerCase().endsWith(".pdf"))));
  const zone = el("div", { class: "dropzone" }, [
    el("div", { class: "big", text: "Drop InStat PDFs here" }),
    el("div", { class: "muted", text: "Both the Match report and the Player report for each game. Whole folders work too. Games are remembered, so next time just add the new one." }),
    el("div", { style: "margin-top:14px;display:flex;gap:8px;justify-content:center" }, [
      el("button", { class: "primary", text: "Choose files…", onclick: () => input.click() }),
      el("button", { text: "Choose a folder…", onclick: () => folder.click() }),
    ]),
    input, folder, log,
  ]);
  zone.addEventListener("dragover", (e) => { e.preventDefault(); zone.classList.add("over"); });
  zone.addEventListener("dragleave", () => zone.classList.remove("over"));
  zone.addEventListener("drop", async (e) => {
    e.preventDefault();
    zone.classList.remove("over");
    uploadFiles(await filesFromDrop(e.dataTransfer));
  });
  const custom = state.request.games.length > 1 ? new Set(state.request.games) : new Set((a?.games || []).map((g) => g.id));
  const rows = (a?.games || []).map((g) => {
    const box = el("input", { type: "checkbox", checked: custom.has(g.id) });
    box.addEventListener("change", () => {
      if (box.checked) custom.add(g.id); else custom.delete(g.id);
    });
    return el("tr", {}, [
      el("td", { class: "left" }, [box]),
      el("td", { class: "left", text: g.date }),
      el("td", { class: "left", text: g.opponent }),
      el("td", { text: `${g.goals_for}-${g.goals_against}` }),
      el("td", { text: g.outcome === "Win" ? "W" : g.outcome === "OvertimeLoss" ? "OTL" : "L" }),
      el("td", { class: "left wrap small" }, [g.warnings.length ? el("span", { class: "err", text: g.warnings.join("; ") }) : el("span", { class: "muted", text: "checks passed" })]),
      el("td", {}, [el("button", { class: "small", text: "Remove", onclick: () => removeGame(g) })]),
    ]);
  });
  const library = el("div", { class: "card" }, [
    el("div", { class: "card-head" }, [
      el("div", {}, [el("h3", { text: "Loaded games" }), el("p", { class: "desc", text: "Tick games and press “Analyse selected” for a custom scope. The check column shows the reader's self-checks (e.g. every player's +/- rebuilt from shifts matches InStat)." })]),
      el("div", { class: "actions" }, [
        el("button", { text: "Analyse selected", onclick: () => { state.request = { ...state.request, games: [...custom], focus: null }; state.view = "overview"; refresh(); } }),
        el("button", { text: "Whole season", onclick: () => { state.request = { ...state.request, games: [], focus: null }; state.view = "overview"; refresh(); } }),
      ]),
    ]),
    rows.length ? el("div", { class: "table-wrap" }, [el("table", {}, [
      el("thead", {}, [el("tr", {}, ["", "Date", "Opponent", "Score", "", "Checks", ""].map((h) => el("th", { class: "left", text: h })))]),
      el("tbody", {}, rows),
    ])]) : el("div", { class: "empty", text: "No games yet — drop the PDFs above." }),
    state.pending.length ? el("div", { class: "warning-box", style: "margin-top:12px" }, [`Waiting: ${state.pending.join("; ")}`]) : null,
    state.problems.length ? el("div", { class: "warning-box", style: "margin-top:12px" }, [`Problems: ${state.problems.join("; ")}`]) : null,
  ]);
  return page("Games & uploads", "Everything stays on this computer. Reports are read, checked and stored in the app's library folder.", zone, el("div", { style: "height:16px" }), library);
}

async function filesFromDrop(transfer) {
  const out = [];
  const walk = (entry) => new Promise((resolve) => {
    if (entry.isFile) {
      entry.file((file) => { if (file.name.toLowerCase().endsWith(".pdf")) out.push(file); resolve(); }, () => resolve());
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      const readAll = () => reader.readEntries(async (entries) => {
        if (!entries.length) { resolve(); return; }
        await Promise.all(entries.map(walk));
        readAll();
      }, () => resolve());
      readAll();
    } else resolve();
  });
  const entries = [...(transfer.items || [])].map((i) => (i.webkitGetAsEntry ? i.webkitGetAsEntry() : null)).filter(Boolean);
  if (entries.length) await Promise.all(entries.map(walk));
  else out.push(...[...transfer.files].filter((f) => f.name.toLowerCase().endsWith(".pdf")));
  return out;
}

async function uploadFiles(files) {
  if (!files.length) return;
  // Match reports first so player reports pair immediately.
  files.sort((a, b) => Number(/player/i.test(a.name)) - Number(/player/i.test(b.name)));
  for (const file of files) {
    state.uploadLog.push({ ok: true, text: `Reading ${file.name}…` });
    render();
    try {
      const outcome = await api("/api/upload", { method: "POST", body: await file.arrayBuffer() });
      const text = {
        GameAdded: `Added ${outcome.message}`,
        GameUpdated: `Updated ${outcome.message}`,
        WaitingForMatchReport: `Stored player report for ${outcome.message}; add its match report too`,
      }[outcome.status] || JSON.stringify(outcome);
      state.uploadLog[state.uploadLog.length - 1] = { ok: true, text: `✓ ${file.name}: ${text}` };
    } catch (error) {
      state.uploadLog[state.uploadLog.length - 1] = { ok: false, text: `✗ ${file.name}: ${error.message}` };
    }
    render();
  }
  await refresh();
}

async function removeGame(game) {
  if (!confirm(`Remove ${game.date} vs ${game.opponent} and its PDFs from the library?`)) return;
  try {
    await api(`/api/games/${encodeURIComponent(game.id)}`, { method: "DELETE" });
  } catch (error) {
    alert(error.message);
  }
  state.request = { ...state.request, games: state.request.games.filter((id) => id !== game.id), focus: null };
  refresh();
}

async function exportReport() {
  try {
    const response = await fetch("/api/export", { method: "POST", headers: { "X-Hockey-Token": state.token }, body: JSON.stringify(state.request) });
    if (!response.ok) throw new Error(`export failed (${response.status})`);
    const blob = await response.blob();
    const link = el("a", { href: URL.createObjectURL(blob), download: `${(state.analysis.team_name || "hockey").toLowerCase().replace(/[^a-z0-9]+/g, "-")}-report.html` });
    document.body.append(link);
    link.click();
    link.remove();
  } catch (error) {
    alert(error.message);
  }
}

async function quitApp() {
  if (!confirm("Close Hockey Stats? Your games stay saved.")) return;
  try { await api("/api/quit", { method: "POST" }); } catch { /* the server is gone either way */ }
  document.body.replaceChildren(el("div", { class: "empty", text: "Hockey Stats has closed. Double-click the app to open it again." }));
}

// ---------- Overview ----------

function bestUnits(list, minSeconds, count = 3) {
  return list
    .filter((u) => u.shrunk_corsi && u.toi >= minSeconds)
    .sort((a, b) => b.shrunk_corsi.prob_above_average - a.shrunk_corsi.prob_above_average)
    .slice(0, count);
}

function trendDelta(player) {
  const points = player.trend.filter((p) => p.instat_index !== null);
  if (points.length < 6) return null;
  const recent = points.slice(-3).map((p) => p.instat_index);
  const earlier = points.slice(-8, -3).map((p) => p.instat_index);
  const avg = (xs) => xs.reduce((s, v) => s + v, 0) / xs.length;
  return avg(recent) - avg(earlier);
}

function viewOverview() {
  const a = state.analysis;
  const t = a.team;
  const record = `${t.wins}-${t.losses}${t.overtime_losses ? `-${t.overtime_losses}` : ""}`;
  const kpis = tiles([
    { label: "Record", value: record, note: `${t.games} game${t.games === 1 ? "" : "s"}` },
    { label: "Goals", value: `${t.goals_for}–${t.goals_against}`, note: t.goal_differential ? `${signed(t.goal_differential.value, 2)} per game (95%: ${signed(t.goal_differential.low, 1)} to ${signed(t.goal_differential.high, 1)})` : "per game CI needs 3+ games" },
    { label: "Shot share", value: pct(t.shot_share, 0), note: `${t.shots_for} for, ${t.shots_against} against` },
    { label: "Expected-goals share", value: pct(t.xg_share, 0), note: `xG ${fmt(t.xg_for, 1)} – ${fmt(t.xg_against, 1)}` },
    { label: "Even-strength CF%", value: pct(t.even_strength_corsi_pct, 0), note: "shot attempts at 5v5-type play" },
    { label: "Power play", value: pct(t.power_play_pct, 0), note: `${t.power_play_goals}/${t.power_play_chances}` },
    { label: "Penalty kill", value: pct(t.penalty_kill_pct, 0), note: `${t.power_play_goals_against} allowed in ${t.times_short_handed}` },
    { label: "PDO (luck gauge)", value: fmt(t.pdo, 1), note: "shooting % + save %; ~100 is normal" },
  ]);
  const unitCallout = (title, list) => el("div", { class: "card" }, [
    el("h3", { text: title }),
    ...(list.length ? list.map((u) => el("div", { class: "callout", style: "margin-top:8px" }, [
      el("div", { class: "who", text: names(u.players) }),
      el("div", { class: "small", text: `${pct(u.shrunk_corsi.estimate.value, 0)} of shot attempts (range ${pct(u.shrunk_corsi.estimate.low, 0)}–${pct(u.shrunk_corsi.estimate.high, 0)}) · ${minutes(u.toi)} min` }),
      el("div", { class: "small muted", text: probabilityWords(u.shrunk_corsi.prob_above_average) }),
    ])) : [el("p", { class: "muted small", text: "Not enough ice time yet." })]),
  ]);
  const topPairs = a.pairs
    .filter((p) => p.corsi && p.corsi.shrunk && p.together.toi >= a.request.min_unit_minutes * 60)
    .sort((x, y) => y.corsi.shrunk.prob_above_average - x.corsi.shrunk.prob_above_average)
    .slice(0, 3);
  const pairCallout = el("div", { class: "card" }, [
    el("h3", { text: "Best-looking pairs" }),
    ...(topPairs.length ? topPairs.map((p) => el("div", { class: "callout", style: "margin-top:8px" }, [
      el("div", { class: "who", text: `${p.a.name} & ${p.b.name}` }),
      el("div", { class: "small", text: `${minutes(p.together.toi)} min together · CF% ${pct(p.corsi.corsi_pct, 0)} (expected ${pct(p.corsi.expected_pct, 0)})` }),
      el("div", { class: "small muted", text: probabilityWords(p.corsi.shrunk.prob_above_average) }),
    ])) : [el("p", { class: "muted small", text: "Not enough data." })]),
  ]);
  const movers = a.players.map((p) => ({ p, d: trendDelta(p) })).filter((x) => x.d !== null).sort((x, y) => y.d - x.d);
  const moversCard = el("div", { class: "card" }, [
    el("h3", { text: "Trending (InStat Index, last 3 vs previous 5 games)" }),
    ...(movers.length ? [...movers.slice(0, 3), ...movers.slice(-2).reverse()].map(({ p, d }) => el("div", { class: "callout", style: `margin-top:8px;border-left-color:${d >= 0 ? css("--good") : css("--critical")}` }, [
      el("button", { class: "link who", text: p.player.name, onclick: () => { state.player = p.player.id; setView("players"); } }),
      el("div", { class: "small", text: `${d >= 0 ? "▲" : "▼"} ${signed(d, 0)} points` }),
    ])) : [el("p", { class: "muted small", text: "Needs 6+ games of history." })]),
  ]);
  const log = t.game_log;
  const charts = el("div", { class: "grid two", style: "margin-top:16px" }, [
    chartCard(log.length < 2 ? "Goals by period" : "Goal differential by game", log.length < 2 ? "Ours vs theirs in each period." : "Green bars are wins, red are losses; length is the margin.", (c) => {
      if (log.length < 2) { groupedColumns(c, t.periods.map((p) => ({ label: p.period <= 3 ? `P${p.period}` : "OT", values: [p.goals_for, p.goals_against] })), [{ name: "Our goals", color: css("--series-1") }, { name: "Their goals", color: css("--series-2") }]); return; }
      hBarChart(c, log.map((g) => ({ label: `${g.date} ${g.opponent}`, value: g.goals_for - g.goals_against, color: g.goals_for >= g.goals_against ? css("--good") : css("--critical") })), { valueFormat: (v) => signed(v, 0), labelWidth: 200 });
    }, (c) => dataTable(c, [
      { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
      { key: "gf", label: "GF", value: (g) => g.goals_for }, { key: "ga", label: "GA", value: (g) => g.goals_against },
    ], log)),
    chartCard(log.length < 2 ? "Shots by period" : "Shot share by game", log.length < 2 ? "Ours vs theirs in each period of this game." : "Our share of all shots; above 50% means we out-shot them.", (c) => {
      if (log.length < 2) { groupedColumns(c, t.periods.map((p) => ({ label: p.period <= 3 ? `P${p.period}` : "OT", values: [p.shots_for, p.shots_against] })), [{ name: "Our shots", color: css("--series-1") }, { name: "Their shots", color: css("--series-2") }]); return; }
      lineChart(c, [{ name: "Shot share", color: css("--series-1"), points: log.map((g, i) => ({ x: i, y: 100 * g.shots_for / Math.max(1, g.shots_for + g.shots_against), label: g.opponent })) }], { reference: 50, yFormat: (v) => pct(v, 0), xFormat: (i) => log[i]?.date || "" });
    }, (c) => dataTable(c, [
      { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
      { key: "sf", label: "Shots for", value: (g) => g.shots_for }, { key: "sa", label: "Shots against", value: (g) => g.shots_against },
    ], log)),
  ]);
  return page("Overview", `${a.team_name} — the headline numbers for the games in scope.`,
    smallSampleNote(a), kpis,
    el("div", { class: "callouts" }, [unitCallout("Best defence pairs", bestUnits(a.units.defence_pairs, a.request.min_unit_minutes * 60)), unitCallout("Best forward lines", bestUnits(a.units.forward_lines, a.request.min_unit_minutes * 60)), pairCallout, moversCard]),
    charts);
}

// ---------- Single game ----------

function viewGame() {
  const a = state.analysis;
  if (!a.timelines.length) return page("Single game", "No games in scope.");
  const focusId = state.gameFocus && a.timelines.some((t) => t.game === state.gameFocus) ? state.gameFocus : a.timelines[a.timelines.length - 1].game;
  const timeline = a.timelines.find((t) => t.game === focusId);
  const select = el("select", { onchange: (e) => { state.gameFocus = e.target.value; state.request = { ...state.request, focus: e.target.value }; refresh(); } },
    a.timelines.map((t) => el("option", { value: t.game, selected: t.game === focusId, text: `${t.date} vs ${t.opponent} (${t.goals_for}-${t.goals_against})` })));
  if (a.focus !== focusId && !snapshot) {
    state.request = { ...state.request, focus: focusId };
    queueMicrotask(refresh);
  }
  let highlight = null;
  const positionRank = { Defence: 0, Forward: 1, Goalie: 2, Unknown: 3 };
  const toiOf = (row) => row.shifts.reduce((sum, [s0, e0]) => sum + e0 - s0, 0);
  timeline.players.sort((x, y) => positionRank[x.player.position] - positionRank[y.player.position] || toiOf(y) - toiOf(x));
  const shiftCard = chartCard("Shift chart", "Every shift, power plays and goals. Click a goal to highlight who was on the ice.", (c) => shiftChart(c, timeline, {
    highlight,
    onGoal: (goal) => { highlight = highlight && highlight.join() === goal.on_ice.join() ? null : goal.on_ice; shiftCard.redraw(); },
  }), (c) => dataTable(c, [
    { key: "name", label: "Player", left: true, value: (r) => r.player.name },
    { key: "shifts", label: "Shifts", value: (r) => r.shifts.length },
    { key: "toi", label: "TOI", value: (r) => r.shifts.reduce((s, [a0, b0]) => s + b0 - a0, 0), format: (v) => clock(v) },
    { key: "avg", label: "Avg shift", value: (r) => r.shifts.reduce((s, [a0, b0]) => s + b0 - a0, 0) / Math.max(1, r.shifts.length), format: (v) => clock(v) },
  ], timeline.players, { sortKey: "toi" }));
  const comparisons = a.players
    .filter((p) => p.focus.length)
    .map((p) => ({ p, badges: p.focus.filter((c) => c.badge) }));
  const compareCard = el("div", { class: "card" }, [
    el("h3", { text: "This game vs each player's usual" }),
    el("p", { class: "desc", text: "Badges flag a season best or a value 2+ standard deviations from the player's average in the games in scope." }),
    ...(a.team.games < 3 ? [el("p", { class: "muted small", text: "Load at least 3 games for game-vs-season comparisons to mean much." })] : []),
    el("div", {}, comparisons.filter((x) => x.badges.length).map(({ p, badges }) => el("div", { class: "callout", style: "margin-top:6px" }, [
      el("button", { class: "link who", text: p.player.name, onclick: () => { state.player = p.player.id; setView("players"); } }),
      el("div", { class: "pill-row" }, badges.map((b) => el("span", { class: "badge", text: b.badge }))),
    ]))),
  ]);
  const groupOrder = [...new Set(timeline.team_stats.map((r) => r.group))];
  const statsRows = [...timeline.team_stats].sort((x, y) => groupOrder.indexOf(x.group) - groupOrder.indexOf(y.group));
  const teamStatsCard = el("div", { class: "card", style: "margin-top:16px" }, [el("h3", { text: `Every team stat vs ${timeline.opponent}` })]);
  const statsBody = el("div");
  teamStatsCard.append(statsBody);
  dataTable(statsBody, [
    { key: "group", label: "Section", left: true },
    { key: "label", label: "Stat", left: true },
    { key: "ours", label: "Us", value: (r) => r.ours.text },
    { key: "theirs", label: "Them", value: (r) => r.theirs.text },
  ], statsRows, {});
  return page("Single game", "Pick a game to see its shifts, goals and how each player compared with their usual.",
    el("div", { style: "margin-bottom:14px" }, [select]),
    shiftCard, el("div", { style: "margin-top:16px" }, [compareCard]), teamStatsCard);
}

// ---------- Lines & pairs ----------

const UNIT_TABS = [
  ["defence_pairs", "Defence pairs"],
  ["forward_lines", "Forward lines"],
  ["full_units", "Five-man units"],
  ["power_play", "Power play"],
  ["penalty_kill", "Penalty kill"],
];

function viewLines() {
  const a = state.analysis;
  const list = a.units[state.unitTab];
  const tabs = el("div", { class: "tabs" }, UNIT_TABS.map(([id, label]) => el("button", { class: state.unitTab === id ? "on" : "", text: `${label} (${a.units[id].length})`, onclick: () => { state.unitTab = id; render(); } })));
  const special = state.unitTab === "power_play" || state.unitTab === "penalty_kill";
  const prior = a.units.priors.find(([k]) => ({ defence_pairs: "DefencePair", forward_lines: "ForwardLine", full_units: "FullUnit" })[state.unitTab] === k);
  const ranked = list.filter((u) => u.toi >= a.request.min_unit_minutes * 60);
  let chart;
  if (!special) {
    chart = chartCard("Shot-attempt share, adjusted for sample size",
      "Dot = best estimate of each unit's true share of shot attempts; whiskers = 90% range. Short-time units are pulled toward the team average (the line) until they earn their number.",
      (c) => hBarChart(c, [...ranked].sort((x, y) => (y.shrunk_corsi?.estimate.value ?? 0) - (x.shrunk_corsi?.estimate.value ?? 0)).map((u) => ({
        label: shortNames(u.players),
        value: u.shrunk_corsi?.estimate.value ?? null,
        low: u.shrunk_corsi?.estimate.low,
        high: u.shrunk_corsi?.estimate.high,
        note: `raw ${pct(u.corsi_pct, 0)} over ${minutes(u.toi)} min · ${probabilityWords(u.shrunk_corsi?.prob_above_average)}`,
      })), { dots: true, reference: prior ? prior[1].mean * 100 : undefined, referenceLabel: "team avg", valueFormat: (v) => pct(v, 0), intervalName: "90% range", labelWidth: 190 }),
      null);
  } else {
    chart = chartCard(state.unitTab === "power_play" ? "Power-play shots per 60 minutes" : "Shots against per 60 minutes on the penalty kill",
      state.unitTab === "power_play" ? "Higher is better." : "Lower is better.",
      (c) => hBarChart(c, ranked.map((u) => ({ label: shortNames(u.players), value: u.shots_60, note: `${minutes(u.toi)} min, ${u.goals_for + u.goals_against} goals` })), { valueFormat: (v) => fmt(v, 0), labelWidth: 220 }),
      null);
  }
  const evenColumns = [
    { key: "players", label: "Players", left: true, value: (u) => names(u.players), wrap: true },
    { key: "games", label: "GP" },
    { key: "toi", label: "Min", format: (v) => minutes(v) },
    { key: "corsi_for", label: "CF" }, { key: "corsi_against", label: "CA" },
    { key: "corsi_pct", label: "CF%", format: (v) => pct(v, 0) },
    { key: "shrunk", label: "Adj. CF%", value: (u) => u.shrunk_corsi?.estimate.value, render: (u) => u.shrunk_corsi ? `${pct(u.shrunk_corsi.estimate.value, 0)} (${fmt(u.shrunk_corsi.estimate.low, 0)}–${fmt(u.shrunk_corsi.estimate.high, 0)})` : "—", title: "Shrunk toward team average; 90% range" },
    { key: "prob", label: "Above avg?", value: (u) => u.shrunk_corsi?.prob_above_average, format: (v) => (v === null || v === undefined ? "—" : `${Math.round(v * 100)}%`) },
    { key: "goals_for", label: "GF" }, { key: "goals_against", label: "GA" },
    { key: "corsi_for_60", label: "CF/60", format: (v) => fmt(v, 0) },
    { key: "corsi_against_60", label: "CA/60", format: (v) => fmt(v, 0) },
    { key: "possession_pct", label: "Poss%", format: (v) => pct(v, 0) },
    { key: "pens", label: "Pen ±", value: (u) => u.penalties_drawn - u.penalties_taken, format: (v) => signed(v, 0) },
  ];
  const specialColumns = [
    { key: "players", label: "Players", left: true, value: (u) => names(u.players), wrap: true },
    { key: "games", label: "GP" },
    { key: "toi", label: "Min", format: (v) => minutes(v) },
    { key: "goals", label: state.unitTab === "power_play" ? "Goals for" : "Goals against", value: (u) => u.goals_for + u.goals_against },
    { key: "shots_60", label: "Shots/60", format: (v) => fmt(v, 0) },
    { key: "offensive_zone_share", label: "Time in their zone", format: (v) => pct(v, 0) },
  ];
  const tableCard = el("div", { class: "card", style: "margin-top:16px" }, [el("h3", { text: "All combinations" }), el("p", { class: "desc", text: "Sorted by ice time. Grey rows are under the minimum and not ranked." })]);
  const body = el("div");
  tableCard.append(body);
  dataTable(body, special ? specialColumns : evenColumns, list, { sortKey: "toi", dim: (u) => u.toi < a.request.min_unit_minutes * 60 });
  return page("Lines & pairs", "Every combination InStat tracked, pooled over the games in scope.", smallSampleNote(a), tabs, chart, tableCard);
}

// ---------- Pair chemistry ----------

const CHEMISTRY_METRICS = {
  toi: { label: "Even-strength minutes together", scale: "sequential", value: (p) => p.together.toi / 60, text: (v) => fmt(v, 0) },
  corsi: { label: "Shot-attempt share together (adjusted)", scale: "diverging", center: 50, value: (p) => p.corsi?.shrunk?.estimate.value ?? null, text: (v) => fmt(v, 0) },
  chemistry: { label: "Chemistry: CF% together minus expected", scale: "diverging", center: 0, value: (p) => (p.corsi && p.corsi.expected_pct !== null && p.corsi.corsi_pct !== null && p.corsi.corsi_for + p.corsi.corsi_against >= 5 ? p.corsi.corsi_pct - p.corsi.expected_pct : null), text: (v) => signed(v, 0) },
  goals: { label: "Goal share together (even strength)", scale: "diverging", center: 50, value: (p) => p.together.goals_pct, text: (v) => fmt(v, 0) },
  passes: { label: "Passes between them", scale: "sequential", value: (p) => p.passes_a_to_b + p.passes_b_to_a, text: (v) => fmt(v, 0) },
  lift: { label: "Passing link vs expected (×)", scale: "diverging", center: 1, value: (p) => p.pass_lift, text: (v) => fmt(v, 1) },
};

function skaterOrder(a) {
  const seasons = a.players.filter((p) => p.player.position !== "Goalie");
  const rank = { Defence: 0, Forward: 1 };
  return seasons.sort((x, y) => (rank[x.player.position] ?? 2) - (rank[y.player.position] ?? 2) || y.totals.toi - x.totals.toi).map((p) => p.player);
}

function pairFor(a, idA, idB) {
  return a.pairs.find((p) => (p.a.id === idA && p.b.id === idB) || (p.a.id === idB && p.b.id === idA));
}

function viewChemistry() {
  const a = state.analysis;
  const metric = CHEMISTRY_METRICS[state.chemistryMetric];
  const players = skaterOrder(a);
  const values = a.pairs.map(metric.value).filter((v) => v !== null && v !== undefined);
  const scale = metric.scale === "diverging"
    ? { kind: "diverging", center: metric.center, min: Math.min(metric.center, ...values), max: Math.max(metric.center, ...values) }
    : { kind: "sequential", min: 0, max: Math.max(1, ...values) };
  const select = el("select", { onchange: (e) => { state.chemistryMetric = e.target.value; render(); } },
    Object.entries(CHEMISTRY_METRICS).map(([id, m]) => el("option", { value: id, selected: id === state.chemistryMetric, text: m.label })));
  const heat = chartCard(metric.label, "Defence first, then forwards, by ice time. Click a square for that pair's full breakdown below.", (c) => heatmap(c, players.map((p) => `${p.jersey ?? ""} ${p.name}`.trim()), (i, j) => {
    if (i === j) return null;
    const pair = pairFor(a, players[i].id, players[j].id);
    if (!pair) return null;
    const v = metric.value(pair);
    if (v === null || v === undefined) return null;
    return {
      value: v,
      text: metric.text(v),
      title: `${players[i].name} & ${players[j].name}`,
      tip: [
        { value: metric.text(v), name: metric.label },
        { value: `${minutes(pair.together.toi)} min`, name: "together (even strength)" },
      ],
      onClick: () => { state.pair = [players[i].id, players[j].id]; render(); document.getElementById("pair-explorer")?.scrollIntoView({ behavior: "smooth" }); },
    };
  }, scale, { scaleName: metric.scale === "diverging" ? "" : "", scaleFormat: (v) => metric.text(v) }), (c) => dataTable(c, [
    { key: "a", label: "Player", left: true, value: (p) => p.a.name },
    { key: "b", label: "Partner", left: true, value: (p) => p.b.name },
    { key: "toi", label: "Min together", value: (p) => p.together.toi / 60, format: (v) => fmt(v, 1) },
    { key: "metric", label: metric.label, value: metric.value, format: (v) => (v === null || v === undefined ? "—" : metric.text(v)) },
  ], a.pairs, { sortKey: "metric" }));
  return page("Pair chemistry", "Who plays well together, from shifts (time and goals together) and InStat's line tables (shot attempts together).",
    smallSampleNote(a), el("div", { style: "margin-bottom:12px" }, [el("label", {}, ["Colour by ", select])]), heat, pairExplorer(a, players));
}

function pairExplorer(a, players) {
  const busiest = [...a.pairs].sort((x, y) => y.together.toi - x.together.toi)[0];
  const [defaultA, defaultB] = state.pair || (busiest ? [busiest.a.id, busiest.b.id] : [players[0]?.id, players[1]?.id]);
  const pickA = el("select", {}, players.map((p) => el("option", { value: p.id, selected: p.id === defaultA, text: p.name })));
  const pickB = el("select", {}, players.map((p) => el("option", { value: p.id, selected: p.id === defaultB, text: p.name })));
  const update = () => { state.pair = [pickA.value, pickB.value]; render(); };
  pickA.addEventListener("change", update);
  pickB.addEventListener("change", update);
  const pair = pairFor(a, defaultA, defaultB);
  const card = el("div", { class: "card", id: "pair-explorer", style: "margin-top:16px" }, [
    el("h3", { text: "Pair explorer" }),
    el("div", { style: "display:flex;gap:8px;align-items:center;margin-bottom:12px;flex-wrap:wrap" }, [pickA, "with", pickB]),
  ]);
  if (!pair) {
    card.append(el("p", { class: "muted", text: "These two haven't been on the ice together in the games in scope." }));
    return card;
  }
  const first = pair.a.id === defaultA ? pair.a : pair.b;
  const second = pair.a.id === defaultA ? pair.b : pair.a;
  const firstApart = pair.a.id === defaultA ? pair.a_without_b : pair.b_without_a;
  const secondApart = pair.a.id === defaultA ? pair.b_without_a : pair.a_without_b;
  card.append(tiles([
    { label: "Even-strength minutes together", value: minutes(pair.together.toi), note: `${pair.games_together} game(s); PP ${minutes(pair.pp_toi)}, PK ${minutes(pair.sh_toi)}` },
    { label: "Goals together (EV)", value: `${pair.together.goals_for}–${pair.together.goals_against}`, note: `goal share ${pct(pair.together.goals_pct, 0)}` },
    { label: "Shot attempts together", value: pair.corsi ? `${pair.corsi.corsi_for}–${pair.corsi.corsi_against}` : "—", note: pair.corsi ? `CF% ${pct(pair.corsi.corsi_pct, 0)} · expected ${pct(pair.corsi.expected_pct, 0)}` : "not in InStat's line tables" },
    { label: "Passes", value: `${pair.passes_a_to_b + pair.passes_b_to_a}`, note: `${pair.a.name.split(" ")[0]}→${pair.b.name.split(" ")[0]} ${pair.passes_a_to_b}, back ${pair.passes_b_to_a}${pair.pass_lift ? ` · ${fmt(pair.pass_lift, 1)}× expected` : ""}` },
  ]));
  const rows = [
    { label: "Together", value: pair.corsi?.corsi_pct ?? null, color: css("--series-1") },
    { label: `${first.name} without`, value: pair.corsi ? (pair.a.id === first.id ? pair.corsi.a_apart_pct : pair.corsi.b_apart_pct) : null, color: css("--deemphasis") },
    { label: `${second.name} without`, value: pair.corsi ? (pair.a.id === second.id ? pair.corsi.a_apart_pct : pair.corsi.b_apart_pct) : null, color: css("--deemphasis") },
  ];
  const goalRows = [
    { label: "Together", value: pair.together.goals_pct, color: css("--series-1"), note: `${pair.together.goals_for}–${pair.together.goals_against} in ${minutes(pair.together.toi)} min` },
    { label: `${first.name} without`, value: firstApart.goals_pct, color: css("--deemphasis"), note: `${firstApart.goals_for}–${firstApart.goals_against} in ${minutes(firstApart.toi)} min` },
    { label: `${second.name} without`, value: secondApart.goals_pct, color: css("--deemphasis"), note: `${secondApart.goals_for}–${secondApart.goals_against} in ${minutes(secondApart.toi)} min` },
  ];
  const grid = el("div", { class: "grid two" }, [
    chartCard("Shot-attempt share: together vs apart", pair.corsi && pair.corsi.coverage !== null && pair.corsi.coverage < 0.8 ? `InStat's tables cover ${Math.round(pair.corsi.coverage * 100)}% of their time together, so treat this as partial.` : "Blue = together; grey = each without the other.", (c) => hBarChart(c, rows, { min: 0, max: 100, reference: 50, valueFormat: (v) => pct(v, 0), labelWidth: 170 }), null),
    chartCard("Goal share: together vs apart (even strength)", "From shifts; goals are rare, so this moves a lot with few games.", (c) => hBarChart(c, goalRows, { min: 0, max: 100, reference: 50, valueFormat: (v) => pct(v, 0), labelWidth: 170 }), null),
  ]);
  card.append(grid);
  return card;
}

// ---------- Passing ----------

function viewPassing() {
  const a = state.analysis;
  const p = a.passing;
  if (!p.players.length) return page("Passing network", "No passing data in scope.");
  const label = (id) => p.players.find((x) => x.id === id)?.name || id;
  const nodes = p.players.map((player, i) => ({ id: player.id, label: player.name, size: p.made[i] }));
  const edges = p.edges.map((e) => ({ from: e.from, to: e.to, value: e.passes, lift: e.lift, fromLabel: label(e.from), toLabel: label(e.to) }));
  const network = chartCard("Who passes to whom", "Line thickness = passes. Orange = a connection at least 1.5× what their overall passing volume predicts. Hover a player to isolate their links.", (c) => networkChart(c, nodes, edges), (c) => dataTable(c, [
    { key: "from", label: "From", left: true, value: (e) => label(e.from) },
    { key: "to", label: "To", left: true, value: (e) => label(e.to) },
    { key: "passes", label: "Passes" },
    { key: "expected", label: "Expected", format: (v) => fmt(v, 1) },
    { key: "lift", label: "× expected", format: (v) => fmt(v, 2) },
    { key: "p", label: "p", format: (v) => fmt(v, 3) },
  ], p.edges.filter((e) => e.passes > 0), { sortKey: "passes" }));
  const matrix = chartCard("Pass matrix", "Rows pass to columns.", (c) => heatmap(c, p.players.map((x) => x.name), (i, j) => (i === j ? null : { value: p.matrix[i][j], text: p.matrix[i][j] ? String(p.matrix[i][j]) : "", title: `${p.players[i].name} → ${p.players[j].name}`, tip: [{ value: String(p.matrix[i][j]), name: "passes" }, { value: fmt(p.expected[i][j], 1), name: "expected" }] }), { kind: "sequential", min: 0, max: Math.max(1, ...p.matrix.flat()) }), null);
  const qi = p.quasi_independence;
  const summary = tiles([
    { label: "Passes recorded", value: String(p.total) },
    { label: "Reciprocity", value: p.reciprocity === null ? "—" : `${Math.round(p.reciprocity * 100)}%`, note: "share of passes returned the other way" },
    { label: "Are the links real?", value: qi ? (qi[2] < 0.05 ? "Yes" : "Not yet") : "—", note: qi ? `G² = ${fmt(qi[0], 0)}, df ${qi[1]}, p = ${fmt(qi[2], 3)}` : "needs 20+ passes" },
  ]);
  return page("Passing network", "InStat's pass-distribution tables, pooled over the games in scope.", summary, network, el("div", { style: "height:16px" }), matrix);
}

// ---------- Players ----------

function viewPlayers() {
  const a = state.analysis;
  if (state.player) {
    const season = a.players.find((p) => p.player.id === state.player);
    if (season) return playerDetail(a, season);
  }
  const skaters = a.players;
  const body = el("div");
  const card = el("div", { class: "card" }, [el("p", { class: "desc", text: "Click a player for their card. Rates are per 60 minutes; CF% is even-strength shot-attempt share; adjusted CF% is shrunk toward the team for small samples." }), body]);
  dataTable(body, [
    { key: "name", label: "Player", left: true, value: (p) => `${p.player.jersey ?? ""} ${p.player.name}`.trim() },
    { key: "pos", label: "Pos", left: true, value: (p) => positionShort(p.player.position) },
    { key: "gp", label: "GP", value: (p) => p.totals.games },
    { key: "toi", label: "TOI/GP", value: (p) => p.totals.toi / Math.max(1, p.totals.games), format: (v) => clock(v) },
    { key: "g", label: "G", value: (p) => p.totals.goals },
    { key: "a", label: "A", value: (p) => p.totals.assists },
    { key: "pm", label: "+/-", value: (p) => p.totals.plus_minus, format: (v) => signed(v, 0) },
    { key: "p60", label: "P/60", value: (p) => p.rates.points, format: (v) => fmt(v, 1) },
    { key: "s60", label: "Shots/60", value: (p) => p.rates.shots, format: (v) => fmt(v, 1) },
    { key: "xg60", label: "xG/60", value: (p) => p.rates.xg, format: (v) => fmt(v, 2) },
    { key: "cf", label: "CF%", value: (p) => p.shares.corsi_pct, format: (v) => pct(v, 0) },
    { key: "rel", label: "CF% rel", value: (p) => p.shares.corsi_rel, format: (v) => signed(v, 1), title: "On-ice CF% minus team CF% without them" },
    { key: "adj", label: "Adj. CF%", value: (p) => p.shrunk_corsi?.estimate.value, format: (v) => pct(v, 0) },
    { key: "gfp", label: "EV goal share", value: (p) => p.shares.goals_pct, format: (v) => pct(v, 0) },
    { key: "bat", label: "Battles won", value: (p) => p.shares.battles_pct?.value, format: (v) => pct(v, 0) },
    { key: "idx", label: "InStat", value: (p) => p.instat_mean, format: (v) => fmt(v, 0) },
  ], skaters, { sortKey: "toi", onRow: (p) => { state.player = p.player.id; render(); window.scrollTo(0, 0); }, dim: (p) => !p.qualified });
  return page("Players", "Season numbers for every skater in scope.", smallSampleNote(a), card);
}

function playerDetail(a, s) {
  const back = el("button", { class: "link", text: "← All players", onclick: () => { state.player = null; render(); } });
  const t = s.totals;
  const head = el("div", { class: "player-head" }, [
    el("span", { class: "jersey", text: s.player.jersey ?? "" }),
    el("h1", { text: s.player.name }),
    el("span", { class: "muted", text: `${s.player.position}${s.group ? ` · ${s.group.toLowerCase()}` : ""} · ${t.games} GP` }),
  ]);
  const kpis = tiles([
    { label: "Points", value: `${t.points}`, note: `${t.goals} G, ${t.assists} A · ${fmt(s.rates.points, 1)}/60` },
    { label: "Ice time", value: clock(t.toi / Math.max(1, t.games)), note: `per game · PP ${clock(t.pp_toi / Math.max(1, t.games))}, PK ${clock(t.sh_toi / Math.max(1, t.games))}` },
    { label: "Shot-attempt share", value: pct(s.shares.corsi_pct, 0), note: `${signed(s.shares.corsi_rel, 1)} vs team without them` },
    { label: "Adjusted CF%", value: s.shrunk_corsi ? pct(s.shrunk_corsi.estimate.value, 0) : "—", note: s.shrunk_corsi ? probabilityWords(s.shrunk_corsi.prob_above_average) : "" },
    { label: "EV goals on ice", value: `${t.on_ice_goals_for}–${t.on_ice_goals_against}`, note: `+/- ${signed(t.plus_minus, 0)}` },
    { label: "InStat Index", value: fmt(s.instat_mean, 0), note: s.instat_sd ? `± ${fmt(s.instat_sd, 0)} game to game` : "" },
  ]);
  const PERCENTILE_ORDER = ["InStat Index", "Points/60", "Shots/60", "xG/60", "CF%", "CF% rel", "Passes/60", "Recoveries/60", "Battles won %", "Blocks/60"];
  const pctRows = PERCENTILE_ORDER.filter((label) => label in s.percentiles).map((label) => ({ label, value: s.percentiles[label] }));
  const percentileCard = chartCard("Where they rank on this team", "Percentile among qualified skaters (50 = middle of the team).", (c) => (pctRows.length ? percentileBars(c, pctRows) : c.replaceChildren(el("p", { class: "muted", text: "Below the minimum ice time for ranking." }))), (c) => dataTable(c, [{ key: "label", label: "Metric", left: true }, { key: "value", label: "Percentile", format: (v) => fmt(v, 0) }], pctRows));
  const trendPoints = s.trend.map((p, i) => ({ ...p, i }));
  const trendCard = chartCard("InStat Index over time", "Filled dots are loaded games; hollow dots come from InStat's recent-games table.", (c) => lineChart(c, [{ name: "InStat Index", color: css("--series-1"), points: trendPoints.map((p) => ({ x: p.i, y: p.instat_index, hollow: !p.loaded, label: p.opponent })) }], { xFormat: (i) => trendPoints[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0) }), (c) => dataTable(c, [
    { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
    { key: "instat_index", label: "InStat", format: (v) => fmt(v, 0) }, { key: "toi", label: "TOI", format: (v) => clock(v) },
    { key: "points", label: "P" }, { key: "shots", label: "Shots" }, { key: "plus_minus", label: "+/-", format: (v) => signed(v, 0) },
    { key: "loaded", label: "Source", format: (v) => (v ? "loaded game" : "InStat history") },
  ], s.trend));
  const toiCard = chartCard("Ice time per game", "", (c) => lineChart(c, [{ name: "TOI", color: css("--series-1"), points: trendPoints.map((p) => ({ x: p.i, y: p.toi / 60, hollow: !p.loaded, label: p.opponent })) }], { xFormat: (i) => trendPoints[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0) }), null);
  const focus = s.focus.length ? el("div", { class: "card" }, [el("h3", { text: "Selected game vs their usual" }), el("div")]) : null;
  if (focus) {
    dataTable(focus.lastChild, [
      { key: "metric", label: "Metric", left: true },
      { key: "game", label: "This game", format: (v) => fmt(v, 1) },
      { key: "season_mean", label: "Usual", format: (v) => fmt(v, 1) },
      { key: "z", label: "SDs from usual", format: (v) => signed(v, 1) },
      { key: "badge", label: "", left: true, format: (v) => v || "" },
    ], s.focus);
  }
  const units = ["defence_pairs", "forward_lines", "full_units", "power_play", "penalty_kill"].flatMap((k) => a.units[k].filter((u) => u.players.some((p) => p.id === s.player.id)).map((u) => ({ ...u, tab: k })));
  const unitsCard = el("div", { class: "card" }, [el("h3", { text: "Lines they played on" }), el("div")]);
  dataTable(unitsCard.lastChild, [
    { key: "tab", label: "Type", left: true, format: (v) => UNIT_TABS.find(([id]) => id === v)?.[1] || v },
    { key: "with", label: "With", left: true, value: (u) => names(u.players.filter((p) => p.id !== s.player.id)), wrap: true },
    { key: "toi", label: "Min", format: (v) => minutes(v) },
    { key: "corsi_pct", label: "CF%", format: (v) => pct(v, 0) },
    { key: "gd", label: "Goals", value: (u) => `${u.goals_for}–${u.goals_against}` },
  ], units, { sortKey: "toi" });
  const partners = a.pairs.filter((p) => p.a.id === s.player.id || p.b.id === s.player.id).map((p) => ({ partner: p.a.id === s.player.id ? p.b : p.a, pair: p }));
  const partnerCard = chartCard("Most frequent linemates", "Even-strength minutes together (from shifts).", (c) => hBarChart(c, partners.sort((x, y) => y.pair.together.toi - x.pair.together.toi).slice(0, 10).map(({ partner, pair }) => ({ label: partner.name, value: pair.together.toi / 60, note: `goals ${pair.together.goals_for}–${pair.together.goals_against}${pair.corsi ? ` · CF% ${pct(pair.corsi.corsi_pct, 0)}` : ""}` })), { valueFormat: (v) => `${fmt(v, 0)} min`, labelWidth: 150 }), null);
  const detailCard = el("div", { class: "card" }, [el("h3", { text: "Every InStat number (latest game in scope)" }), el("div")]);
  dataTable(detailCard.lastChild, [
    { key: "group", label: "Table", left: true },
    { key: "label", label: "Stat", left: true },
    { key: "value", label: "Value", value: (r) => r.value.text },
  ], s.focus_details);
  return [back, head, kpis, el("div", { class: "grid two" }, [percentileCard, trendCard, toiCard, partnerCard, focus, unitsCard].filter(Boolean)), el("div", { style: "height:16px" }), detailCard];
}

// ---------- Goalies ----------

function viewGoalies() {
  const a = state.analysis;
  if (!a.goalies.length) return page("Goalies", "No goalie pages in scope (they come from the Player report).");
  return page("Goalies", "Save percentage with 95% ranges; small numbers of shots give wide ranges.", ...a.goalies.map((g) => {
    const points = g.trend.map((p, i) => ({ ...p, i }));
    return el("div", { class: "section" }, [
      el("h2", { text: g.player.name }),
      tiles([
        { label: "Save %", value: g.save_pct ? pct(g.save_pct.value, 1) : "—", note: g.save_pct ? `95%: ${fmt(g.save_pct.low, 1)}–${fmt(g.save_pct.high, 1)}` : "" },
        { label: "Even strength", value: g.even_strength_save_pct ? pct(g.even_strength_save_pct.value, 1) : "—" },
        { label: "Short-handed", value: g.short_handed_save_pct ? pct(g.short_handed_save_pct.value, 1) : "—" },
        { label: "Goals against avg", value: fmt(g.goals_against_average, 2), note: `${g.goals_against} GA in ${minutes(g.toi)} min` },
        { label: "Shots faced", value: String(g.shots_against), note: `${g.games} GP` },
      ]),
      el("div", { class: "grid two" }, [
        chartCard("Save % by game", "Hollow dots come from InStat's recent-games table.", (c) => lineChart(c, [{ name: "Save %", color: css("--series-1"), points: points.map((p) => ({ x: p.i, y: p.save_pct, hollow: !p.loaded, label: `${p.saves}/${p.shots_against} vs ${p.opponent}` })) }], { xFormat: (i) => points[i]?.date.slice(5) || "", yFormat: (v) => pct(v, 0) }), (c) => dataTable(c, [
          { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true }, { key: "shots_against", label: "SA" }, { key: "saves", label: "Saves" }, { key: "save_pct", label: "Sv%", format: (v) => pct(v, 1) },
        ], g.trend)),
        (() => {
          const card = el("div", { class: "card" }, [el("h3", { text: "Every InStat goalie number (latest game)" }), el("div")]);
          dataTable(card.lastChild, [{ key: "label", label: "Stat", left: true }, { key: "v", label: "Value", value: (r) => r.value.text }], g.focus_details);
          return card;
        })(),
      ]),
    ]);
  }));
}

// ---------- Team ----------

function viewTeam() {
  const a = state.analysis;
  const t = a.team;
  const periods = t.periods.map((p) => ({ label: p.period <= 3 ? `Period ${p.period}` : "OT", values: [p.shots_for, p.shots_against] }));
  const goalsByPeriod = t.periods.map((p) => ({ label: p.period <= 3 ? `Period ${p.period}` : "OT", values: [p.goals_for, p.goals_against] }));
  const series = [{ name: "Us", color: css("--series-1") }, { name: "Them", color: css("--series-2") }];
  const grid = el("div", { class: "grid two" }, [
    chartCard("Shots by period", "Watch for fades late in games.", (c) => groupedColumns(c, periods, series), (c) => dataTable(c, [{ key: "period", label: "Period", left: true }, { key: "shots_for", label: "For" }, { key: "shots_against", label: "Against" }, { key: "possession_pct", label: "Possession", format: (v) => pct(v, 0) }], t.periods)),
    chartCard("Goals by period", "", (c) => groupedColumns(c, goalsByPeriod, series), null),
    chartCard("Faceoffs won by zone", "", (c) => hBarChart(c, t.faceoffs.map((z) => ({ label: z.zone, value: z.pct, note: `${z.won} won, ${z.lost} lost` })), { min: 0, max: 100, reference: 50, valueFormat: (v) => pct(v, 0) }), (c) => dataTable(c, [{ key: "zone", label: "Zone", left: true }, { key: "won", label: "Won" }, { key: "lost", label: "Lost" }, { key: "pct", label: "Win %", format: (v) => pct(v, 0) }], t.faceoffs)),
    chartCard("Time by score", "How long we spent leading, tied and trailing.", (c) => hBarChart(c, t.score_states.map((s) => ({ label: s.state, value: s.time / 60 })), { valueFormat: (v) => `${fmt(v, 0)} min` }), null),
    chartCard("Goals by strength", "", (c) => groupedColumns(c, t.strength_goals.map((s) => ({ label: { Even: "Even", PowerPlay: "Power play", ShortHanded: "Short-handed" }[s.strength], values: [s.goals_for, s.goals_against] })), series), null),
    t.cumulative.length >= 2 ? chartCard("Standings points over the season", "2 per win, 1 per overtime loss.", (c) => lineChart(c, [{ name: "Points", color: css("--series-1"), points: t.cumulative.map((p, i) => ({ x: i, y: p.points, label: p.date })) }], { xFormat: (i) => t.cumulative[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0), integer: true }), null) : null,
  ].filter(Boolean));
  const logCard = el("div", { class: "card", style: "margin-top:16px" }, [el("h3", { text: "Game log" }), el("div")]);
  dataTable(logCard.lastChild, [
    { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
    { key: "score", label: "Score", value: (g) => `${g.goals_for}-${g.goals_against}` },
    { key: "outcome", label: "", format: (v) => ({ Win: "W", Loss: "L", OvertimeLoss: "OTL" })[v] },
    { key: "shots_for", label: "SF" }, { key: "shots_against", label: "SA" },
    { key: "xg_for", label: "xGF", format: (v) => fmt(v, 2) }, { key: "xg_against", label: "xGA", format: (v) => fmt(v, 2) },
    { key: "even_strength_corsi_pct", label: "EV CF%", format: (v) => pct(v, 0) },
    { key: "pp", label: "PP", value: (g) => `${g.power_play[0]}/${g.power_play[1]}` },
    { key: "pk", label: "PK", value: (g) => `${g.penalty_kill[0]}/${g.penalty_kill[1]}` },
    { key: "faceoff_pct", label: "FO%", format: (v) => pct(v, 0) },
    { key: "possession_pct", label: "Poss%", format: (v) => pct(v, 0) },
  ], t.game_log, { sortKey: "date" });
  const luck = tiles([
    { label: "Shooting %", value: pct(t.shooting_pct, 1) },
    { label: "Save %", value: pct(t.save_pct, 1) },
    { label: "PDO", value: fmt(t.pdo, 1), note: "far above 100 = likely some luck" },
    { label: "Goals minus xG", value: signed(t.goals_minus_xg, 1), note: "positive = finishing above expectation" },
    { label: "Shot share per game", value: t.shot_share_by_game ? pct(t.shot_share_by_game.value, 0) : "—", note: t.shot_share_by_game ? `95%: ${fmt(t.shot_share_by_game.low, 0)}–${fmt(t.shot_share_by_game.high, 0)} (bootstrap)` : "needs 3+ games" },
  ]);
  return page("Team", "How the team plays as a whole in the games in scope.", luck, grid, logCard);
}

// ---------- Impact ----------

function viewImpact() {
  const a = state.analysis;
  const models = [a.impact.corsi_defence, a.impact.corsi_forwards, a.impact.goals].filter(Boolean);
  if (!models.length) return page("Individual impact", "Not enough data.");
  const cards = models.map((m) => {
    const per = m.label.includes("goals") ? 2 : 1;
    return chartCard(m.label, `Each player's effect per 60 minutes, separated from who they played with (ridge Poisson regression, λ = ${m.lambda}${m.lambda_from_cv ? ", chosen by leave-one-game-out cross-validation" : ", default until 3 games"}). Positive = helps us.`, (c) => hBarChart(c, [...m.rows].sort((x, y) => y.net_per_60 - x.net_per_60).map((r) => ({
      label: r.player.name,
      value: r.net_per_60,
      color: r.net_per_60 >= 0 ? css("--div-pos") : css("--div-neg"),
      note: `for ${signed(r.for_per_60, per)} · against ${signed(r.against_per_60, per)} (per 60) · ${minutes(r.toi)} min`,
    })), { valueFormat: (v) => signed(v, per), labelWidth: 160, valueName: "net per 60 vs average" }), (c) => dataTable(c, [
      { key: "name", label: "Player", left: true, value: (r) => r.player.name },
      { key: "toi", label: "Min", format: (v) => minutes(v) },
      { key: "for_per_60", label: "For /60", format: (v) => signed(v, per) },
      { key: "for_se", label: "± SE", format: (v) => fmt(v, per) },
      { key: "against_per_60", label: "Against /60", format: (v) => signed(v, per) },
      { key: "against_se", label: "± SE", format: (v) => fmt(v, per) },
      { key: "net_per_60", label: "Net /60", format: (v) => signed(v, per) },
    ], m.rows, { sortKey: "net_per_60" }));
  });
  return page("Individual impact", "Plus/minus-style numbers are unfair to players stuck on weak lines. These models credit each player only for the difference they make once their linemates are accounted for.", smallSampleNote(a), el("div", { class: "grid two" }, cards));
}

// ---------- Profiles ----------

function viewProfiles() {
  const a = state.analysis;
  const p = a.profiles;
  const parts = [];
  if (!p.ready) {
    parts.push(el("div", { class: "warning-box", text: `Player style groups need ${p.needs}. Lower “Min. minutes to rank” or load more games.` }));
  } else {
    const groups = p.clusters.map((c, i) => ({ name: `${c.label} (${c.size})`, color: css(SERIES[i]) }));
    parts.push(el("div", { class: "grid two" }, [
      chartCard("Player styles", `Players placed by what they do per 60 minutes (principal components: ${pct(p.explained[0] * 100, 0)} and ${pct(p.explained[1] * 100, 0)} of the variation). Colour = style group (k-means, silhouette ${fmt(p.silhouette, 2)}${p.silhouette < 0.25 ? ": the groups overlap a lot, so treat them as rough" : ""}).`, (c) => scatterChart(c, p.points.map((pt) => ({ x: pt.x, y: pt.y, label: pt.player.name, group: pt.cluster })), groups, { xLabel: "Component 1", yLabel: "Component 2" }), (c) => dataTable(c, [{ key: "name", label: "Player", left: true, value: (r) => r.player.name }, { key: "group", label: "Group", left: true, value: (r) => groups[r.cluster]?.name }], p.points)),
      chartCard("What the components mean", "How strongly each stat drives component 1.", (c) => hBarChart(c, p.features.map((f, i) => ({ label: f, value: p.loadings[i][0], color: p.loadings[i][0] >= 0 ? css("--div-pos") : css("--div-neg"), note: `component 2: ${fmt(p.loadings[i][1], 2)}` })), { valueFormat: (v) => fmt(v, 2), labelWidth: 120 }), null),
    ]));
  }
  parts.push(el("div", { style: "height:16px" }), chartCard("How stats move together (Spearman ρ)", "Blue = rise together, red = one up while the other goes down. Based on qualified skaters.", (c) => heatmap(c, p.correlation_labels, (i, j) => {
    const v = p.correlations[i][j];
    return v === null || i === j ? null : { value: v, text: fmt(v, 1), tip: [{ value: fmt(v, 2), name: "ρ" }] };
  }, { kind: "diverging", center: 0, min: -1, max: 1 }, { scaleFormat: (v) => fmt(v, 1) }), (c) => dataTable(c, [{ key: "a", label: "Stat", left: true }, { key: "b", label: "Outcome", left: true }, { key: "rho", label: "ρ", format: (v) => fmt(v, 2) }, { key: "p", label: "p", format: (v) => fmt(v, 3) }], p.with_results)));
  return page("Player styles", "Groups players by how they play, not how well — useful for building balanced lines.", ...parts);
}

// ---------- Advanced ----------

function viewAdvanced() {
  const a = state.analysis;
  const families = [...new Set(a.tests.map((t) => t.family))];
  const sections = families.map((family) => {
    const rows = a.tests.filter((t) => t.family === family);
    const heading = family.charAt(0).toUpperCase() + family.slice(1);
    const card = el("div", { class: "card section" }, [el("h3", { text: heading }), el("div")]);
    const pValue = (v) => (v === null || v === undefined ? "—" : v < 0.001 ? "<0.001" : fmt(v, 3));
    dataTable(card.lastChild, [
      { key: "verdict", label: "Verdict", left: true, render: (t) => verdictBadge(t.verdict), value: (t) => ({ LikelyReal: 0, Maybe: 1, CouldBeNoise: 2, NotEnoughData: 3 })[t.verdict] },
      { key: "question", label: "Question", left: true, wrap: true },
      { key: "plain", label: "In plain words", left: true, wrap: true },
      { key: "statistic", label: "Statistic", render: (t) => (t.statistic === null ? "—" : `${t.statistic_label} = ${fmt(t.statistic, 2)}`) },
      { key: "df", label: "df", format: (v) => v || "—" },
      { key: "p", label: "p", format: pValue },
      { key: "p_adjusted", label: "Adj. p", format: pValue, title: "Benjamini–Hochberg adjusted within this family" },
      { key: "effect", label: "Effect [95% CI]", render: (t) => (t.effect === null ? "—" : `${fmt(t.effect, 2)}${t.ci ? ` [${fmt(t.ci[0], 1)}, ${fmt(t.ci[1], 1)}]` : ""}`) },
      { key: "how", label: "How", left: true, value: () => "", render: (t) => el("details", {}, [
        el("summary", { text: "details" }),
        el("div", { class: "small", style: "white-space:normal;max-width:340px" }, [
          el("div", {}, [el("strong", { text: "Method: " }), t.method]),
          el("div", {}, [el("strong", { text: "Sample: " }), t.n]),
          t.effect_label ? el("div", {}, [el("strong", { text: "Effect: " }), t.effect_label]) : null,
          t.secondary ? el("div", {}, [el("strong", { text: "Check: " }), t.secondary]) : null,
          t.assumptions ? el("div", {}, [el("strong", { text: "Assumptions: " }), t.assumptions]) : null,
        ]),
      ]) },
    ], rows, { sortKey: "verdict", descending: false });
    return card;
  });
  const powerCard = el("div", { class: "card section" }, [el("h3", { text: "How many games until we know? (power analysis)" }), el("p", { class: "desc", text: "Simulates future games at each unit's current usage, assuming its adjusted shot share is its true level, and finds how many games give an 80% chance of a significant result." }), el("div")]);
  dataTable(powerCard.lastChild, [
    { key: "subject", label: "Unit", left: true, wrap: true },
    { key: "assumed_share", label: "Assumed CF%", format: (v) => pct(v, 0) },
    { key: "baseline_share", label: "Team CF%", format: (v) => pct(v, 0) },
    { key: "attempts_per_game", label: "Attempts/game", format: (v) => fmt(v, 0) },
    { key: "games_so_far", label: "Games so far" },
    { key: "games_needed", label: "Games needed", format: (v) => (v === null ? "60+" : String(v)) },
    { key: "note", label: "", left: true, wrap: true },
  ], a.power, { sortKey: "games_needed", descending: false });
  const m = a.models;
  const modelCard = el("div", { class: "card section" }, [el("h3", { text: "What wins games" })]);
  if (!m.win_model_ready) {
    modelCard.append(el("p", { class: "muted", text: `Switches on at ${m.win_model_needs} games (${m.games} loaded). It will estimate how shot share, expected goals, power plays, faceoffs and possession relate to winning (Firth logistic regression and least-squares on goal differential).` }));
  } else {
    const body = el("div");
    modelCard.append(body);
    dataTable(body, [
      { key: "name", label: "Team stat", left: true },
      { key: "unit", label: "Per", left: true },
      { key: "odds_ratio", label: "Odds ratio (win)", format: (v) => fmt(v, 2) },
      { key: "ci", label: "95% CI", value: (p) => (p.odds_ratio_ci ? p.odds_ratio_ci[0] : null), render: (p) => (p.odds_ratio_ci ? `${fmt(p.odds_ratio_ci[0], 2)}–${fmt(p.odds_ratio_ci[1], 2)}` : "—") },
      { key: "odds_p", label: "p", format: (v) => fmt(v, 3) },
      { key: "goal_diff_slope", label: "Goal diff. slope", format: (v) => signed(v, 2) },
      { key: "r_squared", label: "R²", format: (v) => fmt(v, 2) },
    ], m.predictors);
    if (m.multivariable) {
      modelCard.append(el("p", { class: "small", text: `Together (${m.multivariable.predictors.join(", ")}): R² = ${fmt(m.multivariable.r_squared, 2)} (adjusted ${fmt(m.multivariable.adjusted_r_squared, 2)}), F = ${fmt(m.multivariable.f_statistic, 2)}, p = ${fmt(m.multivariable.f_p, 3)}; predicts wins correctly ${pct((m.multivariable.loo_accuracy || 0) * 100, 0)} of the time on held-out games.` }));
    }
  }
  const opponentCard = el("div", { class: "card section" }, [el("h3", { text: "Lines adjusted for opponent strength" })]);
  if (!m.opponent_model_ready) {
    opponentCard.append(el("p", { class: "muted", text: `Switches on at ${m.opponent_model_needs}. A Poisson mixed model then gives each line a CF% adjusted for who it faced.` }));
  } else {
    const body = el("div");
    opponentCard.append(body);
    dataTable(body, [
      { key: "players", label: "Unit", left: true, value: (u) => names(u.players), wrap: true },
      { key: "raw_corsi_pct", label: "Raw CF%", format: (v) => pct(v, 0) },
      { key: "adjusted_corsi_pct", label: "Opponent-adjusted CF%", format: (v) => pct(v, 0) },
    ], m.adjusted_units, { sortKey: "adjusted_corsi_pct" });
    const opp = el("div", { style: "margin-top:12px" });
    opponentCard.append(opp);
    dataTable(opp, [
      { key: "opponent", label: "Opponent", left: true },
      { key: "attempts_for_them", label: "Their attempts ×", format: (v) => fmt(v, 2) },
      { key: "attempts_for_us", label: "Our attempts ×", format: (v) => fmt(v, 2) },
    ], m.opponents);
  }
  return page("Advanced statistics", "Every test behind the verdicts, with adjusted p-values (Benjamini–Hochberg within each family, so testing many pairs doesn't manufacture false positives).", ...sections, powerCard, modelCard, opponentCard);
}

// ---------- Help ----------

function viewHelp() {
  const terms = [
    ["CF% (Corsi for %)", "Our share of shot attempts (on goal, missed and blocked) while a player or unit is on the ice at even strength. 50% means even; the best predictor of future results available here."],
    ["Adjusted / shrunk CF%", "A unit that played 3 minutes and went 4-0 in attempts is not a 100% unit. We blend each unit's record with the team average, weighted by how much it has played (empirical-Bayes beta-binomial). The range shown is where the true value probably lies (90%)."],
    ["“Probably above average (80%)”", "The model's probability that the unit's true shot share is above the team's average for that kind of unit."],
    ["CF% rel", "A player's on-ice CF% minus the team's CF% when they were off the ice. Helps separate a good player on a bad team from a passenger on a good one."],
    ["xG (expected goals)", "InStat's estimate of how many goals the shots were worth based on location and type. xG share = our xG / (ours + theirs)."],
    ["PDO", "Shooting % plus save %. Teams far above 100 are usually getting lucky and tend to come back down."],
    ["Individual impact", "Ridge-penalised Poisson regressions that estimate each player's effect on shot attempts (or goals) for and against per 60 minutes, while accounting for linemates. The penalty keeps small samples near zero."],
    ["Chemistry", "Whether a pair or line does better than its players' individual ratings predict. Tested overall (likelihood-ratio test) and pair by pair (exact binomial test vs the expected share)."],
    ["With vs without", "Each player's results with the partner compared to their games apart; paired by game once 3+ games exist."],
    ["Verdicts", "Likely real: adjusted p < 0.05. Maybe: < 0.20. Could be noise: otherwise. Not enough data: the test needs more games. p-values are adjusted for the number of pairs/lines tested (Benjamini–Hochberg)."],
    ["Power analysis", "How many more games at the current usage would give an 80% chance of confirming a difference of the size currently estimated."],
    ["Passing lift", "Passes between two players divided by what their overall passing and receiving volumes predict (quasi-independence). Above 1 = a real connection."],
    ["Shift data", "Rebuilt from InStat's time-distribution chart. The reader checks itself: every player's +/- rebuilt from shifts must match InStat's own column, or the game shows a warning."],
    ["Where the numbers come from", "InStat's Match report (team, player, line, shot, challenge and pass tables plus the shift chart) and Player report (full names, jersey numbers, xG, goalie details and each player's recent-games history). InStat prints wrong jersey numbers in some Match-report tables; the reader uses names and ice time instead."],
  ];
  return page("How to read this", "Short explanations of every number in the report.", el("dl", { class: "explain" }, terms.flatMap(([t, d]) => [el("dt", { text: t }), el("dd", { text: d })])));
}

// ---------- Render ----------

function render() {
  renderSidebar();
  renderFilters();
  const main = document.getElementById("content");
  Tooltip.hide();
  if (state.error) {
    main.replaceChildren(el("div", { class: "warning-box", text: state.error }));
    state.error = null;
  }
  const a = state.analysis;
  if (!a) {
    main.replaceChildren(el("div", { class: "empty", text: "Loading…" }));
    return;
  }
  const views = {
    games: viewGames, overview: viewOverview, game: viewGame, lines: viewLines, chemistry: viewChemistry,
    passing: viewPassing, players: viewPlayers, goalies: viewGoalies, team: viewTeam, impact: viewImpact,
    profiles: viewProfiles, advanced: viewAdvanced, help: viewHelp,
  };
  if (a.games.length === 0 && !["games", "help"].includes(state.view)) state.view = "games";
  const content = views[state.view]();
  main.replaceChildren(...[].concat(content).filter(Boolean));
}

let resizeTimer = null;
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(render, 200);
});
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", render);

const [initialView, initialPlayer] = decodeURIComponent(location.hash.slice(1)).split(":");
if (VIEWS.some((v) => v.id === initialView)) state.view = initialView;
if (initialView === "players" && initialPlayer) state.player = initialPlayer;

if (!snapshot) {
  setInterval(() => { api("/api/heartbeat", { method: "POST" }).catch(() => {}); }, 60000);
}
refresh();
