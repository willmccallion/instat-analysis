"use strict";

const { el, css, fmt, pct, signed, clock, minutes, SERIES, Tooltip, term, hBarChart, lineChart, groupedColumns, heatmap, shiftChart, networkChart, scatterChart, percentileBars, zoneMap, shotMap, shotZoneName, shotPlot, shotDistance, netMap, netAreaName, battleMap, battleAreaName, dataTable, chartCard, inkOn } = window.Charts;

// Plain-English definitions; any label matching a key explains itself on hover or tap.
const PER_60 = "per 60 minutes of ice time, so players with different ice time compare fairly";
window.Charts.setGlossary({
  "Points/60": `Goals plus assists ${PER_60}.`,
  "P/60": `Points (goals + assists) ${PER_60}.`,
  "Goals/60": `Goals ${PER_60}.`,
  "Shots/60": `Shots taken (on goal, missed or blocked) ${PER_60}.`,
  "xG/60": `Expected goals ${PER_60}. xG credits each shot with its chance of scoring based on where and how it was taken, so it measures the quality of chances, not luck in finishing.`,
  "xG": "Expected goals: the sum of every shot's chance of scoring (based on location and shot type). A 0.3 xG shot scores about 3 times in 10.",
  "xGF": "Expected goals for: the quality of the chances we created.",
  "xGA": "Expected goals against: the quality of the chances we allowed.",
  "xG against on ice/60": `Expected goals the opponent generated while this player was on the ice, ${PER_60}. Lower is better.`,
  "Expected-goals share": "Our expected goals divided by both teams' expected goals. Above 50% means we created better chances than we allowed.",
  "Entries/60": `Times the player carried or passed the puck into the offensive zone with control, ${PER_60}.`,
  "Recoveries/60": `Loose pucks won back (takeaways, rebounds, battles) ${PER_60}.`,
  "Puck losses/60": `Times the player lost possession (giveaways, turnovers) ${PER_60}. Lower is better.`,
  "Own-zone losses": "Puck losses in our own zone: the costliest giveaways, because the other team gets the puck close to our net. Lower is better.",
  "Carry-in %": "Share of a player's zone entries carried in on the stick, rather than passed in or dumped in. Carrying keeps possession.",
  "Passes/60": `Completed passes to teammates ${PER_60}.`,
  "Blocks/60": `Opponent shots blocked ${PER_60}.`,
  "Hits/60": `Body checks delivered ${PER_60}.`,
  "Battles won/60": `Puck battles won ${PER_60}.`,
  "Battles won %": "Share of one-on-one puck battles the player won.",
  "Battles won": "Share of one-on-one puck battles the player won.",
  "Shot share vs team": "The team's share of shot attempts with this player on the ice, minus the share without them. Positive means the team does better when they're out there.",
  "CF% rel": "The team's share of shot attempts with this player on the ice, minus the share without them. Positive means the team does better when they're out there.",
  "Attempts against/60": `Opponent shot attempts while the player is on the ice at even strength, ${PER_60}. Lower is better.`,
  "Attempts for on ice/60": `Our shot attempts while the player is on the ice at even strength, ${PER_60}.`,
  "CF": "Corsi for: our shot attempts (on goal, missed and blocked) while this player or unit was on the ice at even strength.",
  "CA": "Corsi against: opponent shot attempts while this player or unit was on the ice at even strength.",
  "CF%": "Corsi for %: our share of all shot attempts while on the ice at even strength. 50% = even; it's the best available predictor of future results.",
  "CF/60": `Our shot attempts ${PER_60} at even strength.`,
  "CA/60": `Opponent shot attempts ${PER_60} at even strength. Lower is better.`,
  "Adj. CF%": "CF% adjusted for sample size: blended with the team average in proportion to how little the player or unit has played, so a hot 3-minute stretch doesn't look like a great unit. The range is where the true value probably lies (90%).",
  "Above avg?": "The model's probability that the unit's true shot share is above the team average for that kind of unit.",
  "Even-strength CF%": "Our share of shot attempts when both teams had the same number of skaters.",
  "EV goal share": "Goals for divided by goals for + against while the player was on the ice at even strength.",
  "Shot share": "Our shots on goal divided by both teams' shots on goal (the game-sheet count). Above 50% means we out-shot the opponent.",
  "Shot attempts": "InStat's 'shots': every attempt directed at the net, whether it hit the net or not.",
  "Attempts": "InStat's 'shots': every attempt directed at the net, whether it hit the net or not.",
  "SF": "Shots on goal for.",
  "SA": "Shots on goal against.",
  "GF": "Goals for while on the ice.",
  "GA": "Goals against while on the ice.",
  "Poss%": "Share of puck-possession time.",
  "Pen ±": "Penalties drawn minus penalties taken while the unit was on the ice.",
  "Pen": "Penalties drawn minus penalties taken while the unit was on the ice.",
  "TOI": "Time on ice.",
  "Pos": "Position: D = defence, F = forward.",
  "TOI/GP": "Average time on ice per game.",
  "Min": "Minutes played.",
  "GP": "Games played.",
  "G": "Goals.",
  "A": "Assists.",
  "P": "Points: goals plus assists.",
  "+/-": "Plus/minus: even-strength and short-handed goals for minus goals against while on the ice (power-play goals don't count).",
  "InStat": "InStat Index: InStat's own overall rating for a game, based on every action the player made. Higher is better; roughly 100 is a typical game.",
  "InStat Index": "InStat's own overall rating for a game, based on every action the player made. Higher is better; roughly 100 is a typical game.",
  "Rating": "This app's position rating: 50 = average at the player's position, 60+ clearly above, 40 or less clearly below. Built from Offence, Defence and Puck play stats compared with teammates at the same position.",
  "Form": "Last few games compared with the player's own usual level, in standard deviations. ▲ 1.0 means one typical game-to-game swing above normal.",
  "Off": "Offence part of the rating (points, shots, chance quality, zone entries; for defence also shot attempts for).",
  "Def": "Defence part of the rating (shot attempts and chances allowed on ice, shot share vs team; for defence also blocks).",
  "Puck": "Puck play part of the rating (battles won, recoveries, passes, and fewer puck losses).",
  "Power play": "Goals scored on the power play divided by power-play chances.",
  "Penalty kill": "Share of short-handed situations where we didn't allow a goal.",
  "PP": "Power play: goals / chances.",
  "PK": "Penalty kill: kills / times short-handed.",
  "PDO (luck gauge)": "Shooting % plus save %. Around 100 is normal; well above means some luck is helping and will likely fade, well below means bad luck.",
  "PDO": "Shooting % plus save %. Around 100 is normal; well above means some luck is helping and will likely fade, well below means bad luck.",
  "Shooting %": "Goals divided by shots on goal.",
  "Save %": "Saves divided by shots on goal faced.",
  "Sv%": "Save percentage: saves divided by shots on goal faced.",
  "Goals minus xG": "Actual goals minus expected goals. Positive = finishing better than chances suggest (often luck that evens out).",
  "FO%": "Faceoff win percentage.",
  "Goals against avg": "Goals allowed per 60 minutes played.",
  "p": "p-value: how often a difference this big would appear by pure chance if there were no real effect. Small (under 0.05) = unlikely to be chance.",
  "Adj. p": "p-value adjusted for testing many things at once (Benjamini–Hochberg), so running lots of tests doesn't create false alarms.",
  "df": "Degrees of freedom: how much independent information the test used.",
  "Statistic": "The test's raw result (e.g. chi-square or t). Bigger means a bigger departure from 'no effect'.",
  "Effect [95% CI]": "How big the difference is, with the range it probably lies in (95%).",
  "Verdict": "Likely real: adjusted p < 0.05. Maybe: < 0.20. Could be noise: otherwise. Not enough data: the test needs more games.",
  "Record": "Wins–losses (–overtime losses).",
  "Goals": "Goals for – goals against.",
  "Even-strength minutes together": "Minutes both players were on the ice together with equal numbers of skaters.",
  "Goals together (EV)": "Even-strength goals for and against while both were on the ice.",
  "Shot attempts together": "Shot attempts for and against while both were on the ice (from InStat's line tables).",
  "Passes": "Passes between the two players, both directions.",
  "SDs vs position": "Standard deviations above (+) or below (−) the average of teammates at the same position.",
  "source:game": "These numbers come from this one game only.",
  "source:season": "These numbers pool every loaded game in the current scope (change it with Season / Single game / Custom at the top).",
  "source:history": "Also uses earlier games from InStat's recent-games table in the Player report (games you haven't loaded). Used for form, trends and 'vs usual'.",
});

const params = new URLSearchParams(location.search);
const snapshot = window.HOCKEY_SNAPSHOT || null;

const state = {
  token: params.get("t") || "",
  analysis: snapshot,
  request: snapshot ? snapshot.request : { games: [], focus: null, min_minutes: 10, min_unit_minutes: 3 },
  view: "summary",
  sub: {},
  rankingTab: "forwards",
  playerColumns: "key",
  player: null,
  playerTab: "overview",
  gameTab: "overview",
  goalieTab: "overview",
  shotPeriod: "all",
  goalie: null,
  unitTab: "defence_pairs",
  chemistryMetric: "toi",
  pair: null,
  gameFocus: null,
  customGames: null,
  uploadLog: [],
  pending: [],
  problems: [],
  team: null,
  changingTeam: false,
  update: null,
  updateInstalling: false,
  updateError: null,
};

const VIEWS = [
  { id: "summary", label: "Summary" },
  { id: "rankings", label: "Rankings" },
  { id: "lines", label: "Lines & pairs" },
  { id: "players", label: "Players" },
  { id: "game", label: "Game" },
  { id: "team", label: "Team & goalies" },
  { id: "deep", label: "Deep dive" },
  { id: "help", label: "How to read this" },
];

const SUBVIEWS = {
  lines: [["units", "Lines"], ["chemistry", "Pair chemistry"], ["passing", "Passing"]],
  team: [["team", "Team"], ["play", "Possession & shots"], ["focus", "Practice focus"], ["goalies", "Goalies"]],
  deep: [["impact", "Individual impact"], ["profiles", "Player styles"], ["advanced", "Statistical tests"]],
};

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

/** The app's server isn't answering: it was quit, closed with its page, or timed out. */
class ServerGone extends Error {}

async function api(path, options = {}) {
  let response;
  try {
    response = await fetch(path, {
      ...options,
      headers: { "X-Hockey-Token": state.token, ...(options.headers || {}) },
    });
  } catch {
    throw new ServerGone("Hockey Stats is not running");
  }
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
    state.team = status.team;
    state.pending = status.pending;
    state.problems = status.problems;
    state.analysis = await api("/api/analyze", { method: "POST", body: JSON.stringify(state.request) });
  } catch (error) {
    if (error instanceof ServerGone) {
      showClosed();
      return;
    }
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
  const choosingTeam = !snapshot && !state.team;
  nav.replaceChildren(...(choosingTeam ? [] : VIEWS).map((view) => el("button", { class: `nav-item ${state.view === view.id ? "active" : ""}`, text: view.label, onclick: () => setView(view.id) })));
  const footer = document.getElementById("sidebar-footer");
  footer.replaceChildren();
  if (choosingTeam) {
    footer.append(el("button", { text: "Quit app", onclick: quitApp }));
  } else if (!snapshot) {
    footer.append(
      el("button", { class: state.view === "games" ? "primary" : "", text: "Add games / manage", onclick: () => setView("games") }),
      el("button", { text: "Save report (HTML)", onclick: exportReport, title: "Download a single file you can email or open anywhere" }),
      el("button", { text: "Quit app", onclick: quitApp }),
    );
  } else {
    footer.append(el("div", { class: "small muted", text: "Saved report — scope is fixed." }));
  }
  document.getElementById("team-name").textContent = state.analysis?.team_name || "";
}

// Pages that pool several games show the scope picker; pages that rank players also show
// the ice-time minimum. Game, Help and Uploads have no filter bar.
const SCOPE_VIEWS = new Set(["summary", "rankings", "lines", "players", "team", "deep"]);
const MIN_MINUTES_VIEWS = new Set(["rankings", "lines", "players", "deep"]);

function renderFilters() {
  const bar = document.getElementById("filters");
  bar.replaceChildren();
  const a = state.analysis;
  const visible = a && a.games.length > 0 && SCOPE_VIEWS.has(state.view);
  bar.hidden = !visible;
  if (!visible) return;
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
  if (MIN_MINUTES_VIEWS.has(state.view)) {
    bar.append(el("label", { title: "Players below this total ice time are shown but not ranked" }, ["Min. minutes to rank", minInput]));
  }
  bar.append(el("span", { class: "spacer" }));
  const inScope = a.games.filter((g) => g.in_scope).length;
  if (inScope < 10) {
    const pill = el("span", { class: "badge", title: "Numbers are real but noisy with few games. Estimates are already pulled toward average for small samples, and verdicts say \"not enough data\" where needed. Some models switch on at 5 and 10 games." }, [`Early season: ${inScope} game${inScope === 1 ? "" : "s"}, treat as first impressions`]);
    bar.append(pill);
  } else {
    bar.append(el("span", { class: "muted small", text: `${inScope} of ${a.games.length} games in scope` }));
  }
}

let insideTabs = false;

function pageTitle(title) {
  return el("h1", { class: "title-row" }, [title, SCOPE_VIEWS.has(state.view) && state.analysis?.games.length ? scopeChip() : null]);
}

function page(title, lede, ...children) {
  return [insideTabs ? null : pageTitle(title), lede ? el("p", { class: "lede", text: lede }) : null, ...children];
}

function scopeChip() {
  const a = state.analysis;
  const inScope = a.games.filter((g) => g.in_scope);
  const single = state.request.games.length === 1 && inScope.length === 1;
  const count = `${inScope.length} game${inScope.length === 1 ? "" : "s"}`;
  const label = single ? `This game: ${inScope[0].date.slice(5)} vs ${inScope[0].opponent}` : `${state.request.games.length > 1 ? "Custom" : "Season"} · ${count}`;
  const chip = el("span", { class: "source scope" }, [single ? "◉ " : "▦ ", label]);
  window.Charts.explain(chip, single ? "source:game" : "source:season");
  return chip;
}

function historyChip() {
  const chip = el("span", { class: "source history" }, ["⟲ + InStat history"]);
  window.Charts.explain(chip, "source:history");
  return chip;
}

function gameChip(label) {
  const chip = el("span", { class: "source scope" }, [`◉ ${label}`]);
  window.Charts.explain(chip, "source:game");
  return chip;
}

function cardTitle(title, ...chips) {
  return el("h3", { class: "title-row" }, [title, ...chips]);
}

function ratingDot(rating) {
  if (rating === null || rating === undefined) return null;
  const color = window.Charts.divergingColor(Math.max(-1, Math.min(1, (rating - 50) / 6)));
  return el("span", { class: "rating-dot", style: `background:${color}`, title: `Rating ${fmt(rating, 0)}` });
}

/** A collapsed section for detail that most readers can skip. */
function more(label, ...children) {
  return el("details", { class: "more" }, [el("summary", { text: label }), el("div", { class: "more-body" }, children)]);
}

/** A row of tabs: `tabs` is [[id, label]], `choose(id)` switches to one. */
function tabBar(tabs, current, choose) {
  return el("div", { class: "tabs" }, tabs.map(([id, label]) => el("button", { class: current === id ? "on" : "", text: label, onclick: () => choose(id) })));
}

function subTabs(view) {
  const tabs = SUBVIEWS[view];
  return tabBar(tabs, state.sub[view] || tabs[0][0], (id) => { state.sub[view] = id; render(); });
}

/** Tabs within one page (a player card, a game); `key` names the state field holding the choice. */
function pageTabs(key, tabs) {
  const current = tabs.some(([id]) => id === state[key]) ? state[key] : tabs[0][0];
  return [tabBar(tabs, current, (id) => { state[key] = id; render(); }), current];
}

function emptyNote(text) {
  return el("p", { class: "muted", text });
}

function tiles(items) {
  return el("div", { class: "tiles" }, items.map((t) => el("div", { class: "tile" }, [
    el("div", { class: "label" }, [term(t.label)]),
    el("div", { class: "value", text: t.value }),
    t.note ? el("div", { class: "note", text: t.note }) : null,
  ])));
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
  const team = el("p", { class: "small muted" }, [
    `Your team: ${state.team} `,
    el("button", { class: "link small", text: "change", onclick: () => { state.changingTeam = true; render(); } }),
  ]);
  return page("Games & uploads", "Everything stays on this computer. Reports are read, checked and stored in the app's library folder.", team, zone, el("div", { style: "height:16px" }), library);
}

/** The preset for the team this app was built for; any other team can be typed in. */
const PRESET_TEAM = "SSAC";

async function chooseTeam(name) {
  try {
    state.team = await api("/api/team", { method: "POST", body: name });
    state.changingTeam = false;
    state.view = "games";
    await refresh();
  } catch (error) {
    if (error instanceof ServerGone) showClosed();
    else { state.error = error.message; render(); }
  }
}

function viewSetup() {
  const input = el("input", { type: "text", placeholder: "Start of your team's name", style: "min-width:260px" });
  const useTyped = () => { if (input.value.trim()) chooseTeam(input.value); };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") useTyped(); });
  return [
    el("h1", { text: state.team ? "Change your team" : "Welcome to Hockey Stats" }),
    el("p", { class: "lede", text: "Which team do you coach? The app uses this to know which side of each InStat report is yours. You only do this once." }),
    el("div", { class: "card setup" }, [
      el("button", { class: "primary big-choice", text: PRESET_TEAM, onclick: () => chooseTeam(PRESET_TEAM) }),
      el("p", { class: "small muted", text: `Choose this if your team's name starts with ${PRESET_TEAM} on the InStat reports.` }),
      el("div", { class: "group-label", text: "Another team" }),
      el("div", { style: "display:flex;gap:8px;flex-wrap:wrap;align-items:center" }, [input, el("button", { text: "Use this team", onclick: useTyped })]),
      el("p", { class: "small muted", text: "Type the start of your team's name exactly as InStat prints it at the top of the reports (capital letters don't matter)." }),
      state.team ? el("button", { class: "link small", text: "Cancel", onclick: () => { state.changingTeam = false; render(); } }) : null,
    ]),
  ];
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
  showClosed();
}

async function watchForUpdate() {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    try {
      state.update = await api("/api/update");
    } catch {
      return;
    }
    if (state.update.status !== "Checking") break;
    await new Promise((resolve) => { setTimeout(resolve, 1000); });
  }
  if (state.update.status === "Available") render();
}

async function installUpdate() {
  state.updateInstalling = true;
  state.updateError = null;
  render();
  try {
    await api("/api/update", { method: "POST" });
    document.body.replaceChildren(el("div", { class: "empty", text: "Updated! Hockey Stats is reopening in a new tab. You can close this one." }));
  } catch (error) {
    state.updateInstalling = false;
    state.updateError = error.message;
    render();
  }
}

function updateBanner() {
  if (snapshot || state.update?.status !== "Available") return null;
  const { version, notes } = state.update.release;
  return el("div", { class: "update-banner" }, [
    el("div", {}, [
      el("strong", { text: `A new version of Hockey Stats (${version}) is available.` }),
      notes ? el("div", { class: "small", text: notes }) : null,
      state.updateInstalling ? el("div", { class: "small", text: "Downloading and checking the new version…" }) : null,
      state.updateError ? el("div", { class: "small err", text: `The update didn't install: ${state.updateError}. Your current version still works.` }) : null,
    ]),
    el("button", { class: "primary", text: "Update now", disabled: state.updateInstalling, onclick: installUpdate }),
  ]);
}

function showClosed() {
  document.body.replaceChildren(el("div", { class: "empty", text: "Hockey Stats has closed. Double-click the app to open it again." }));
}

// ---------- Overview ----------



function goToPlayer(id) {
  state.player = id;
  setView("players");
}

function playerLink(player) {
  return el("button", { class: "link who", text: player.name, onclick: () => goToPlayer(player.id) });
}

/** Plain-sentence takeaways, good and bad, ordered by how notable they are. */
function takeaways(a) {
  const t = a.team;
  const items = [];
  // Tier orders the list (team result first, individual form last); weight orders within a tier.
  const add = (tone, tier, weight, text, go) => items.push({ tone, tier, weight, text, go });
  if (t.shot_share !== null) {
    const counts = `${t.shots_on_goal_for}–${t.shots_on_goal_against} on goal; attempts ${t.attempts_for}–${t.attempts_against}`;
    if (t.shot_share < 45) add("bad", 1, 50 - t.shot_share, `Out-shot: only ${pct(t.shot_share, 0)} of shots on goal (${counts}).`, () => setView("team"));
    else if (t.shot_share > 55) add("good", 1, t.shot_share - 50, `Out-shooting them: ${pct(t.shot_share, 0)} of shots on goal (${counts}).`, () => setView("team"));
  }
  if (t.goals_minus_xg !== null && Math.abs(t.goals_minus_xg) >= 1) {
    add(t.goals_minus_xg > 0 ? "info" : "bad", 1, Math.abs(t.goals_minus_xg),
      t.goals_minus_xg > 0 ? `Scored ${fmt(t.goals_minus_xg, 1)} more goals than our chances usually produce, so expect some cooling off.` : `Scored ${fmt(-t.goals_minus_xg, 1)} fewer goals than our chances deserved, so finishing should improve.`, () => setView("team"));
  }
  const periods = t.periods.filter((p) => p.shots_on_goal_for + p.shots_on_goal_against > 0);
  if (periods.length >= 2) {
    const share = (p) => 100 * p.shots_on_goal_for / (p.shots_on_goal_for + p.shots_on_goal_against);
    const worst = periods.reduce((x, y) => (share(y) < share(x) ? y : x));
    const best = periods.reduce((x, y) => (share(y) > share(x) ? y : x));
    if (share(best) - share(worst) >= 10) add("bad", 2, share(best) - share(worst), `Period ${worst.period} is the weak spot: ${worst.shots_on_goal_for}–${worst.shots_on_goal_against} in shots on goal, ${worst.goals_for}–${worst.goals_against} in goals.`, () => setView("team"));
  }
  if (t.power_play_chances >= 3) {
    if (t.power_play_pct >= 25) add("good", 2, t.power_play_pct, `Power play is working: ${t.power_play_goals} goals on ${t.power_play_chances} chances.`, () => { state.sub.lines = "units"; state.unitTab = "power_play"; setView("lines"); });
    else if (t.power_play_pct < 12) add("bad", 2, 30, `Power play is struggling: ${t.power_play_goals} goals on ${t.power_play_chances} chances.`, () => { state.sub.lines = "units"; state.unitTab = "power_play"; setView("lines"); });
  }
  if (t.times_short_handed >= 3 && t.penalty_kill_pct < 75) add("bad", 2, 100 - t.penalty_kill_pct, `Penalty kill allowed ${t.power_play_goals_against} goals in ${t.times_short_handed} times short-handed.`, () => { state.sub.lines = "units"; state.unitTab = "penalty_kill"; setView("lines"); });
  const pairs = a.units.defence_pairs.filter((u) => u.shrunk_corsi && u.toi >= a.request.min_unit_minutes * 60);
  if (pairs.length >= 2) {
    const best = pairs.reduce((x, y) => (y.shrunk_corsi.prob_above_average > x.shrunk_corsi.prob_above_average ? y : x));
    const worst = pairs.reduce((x, y) => (y.shrunk_corsi.prob_above_average < x.shrunk_corsi.prob_above_average ? y : x));
    add("good", 3, best.shrunk_corsi.prob_above_average, `Best defence pair so far: ${names(best.players)} (${pct(best.corsi_pct, 0)} of shot attempts).`, () => { state.sub.lines = "units"; state.unitTab = "defence_pairs"; setView("lines"); });
    add("bad", 3, 1 - worst.shrunk_corsi.prob_above_average, `Toughest defence pair: ${names(worst.players)} (${worst.corsi_for}–${worst.corsi_against} in shot attempts).`, () => { state.sub.lines = "units"; state.unitTab = "defence_pairs"; setView("lines"); });
  }
  const form = a.rankings.form;
  if (form.length >= 2) {
    const hot = form[0];
    const cold = form[form.length - 1];
    if (hot.recent_z >= 1) add("good", 4, hot.recent_z, `${hot.player.name} is in form: well above their usual over the last ${hot.recent_games} games.`, () => goToPlayer(hot.player.id));
    if (cold.recent_z <= -1) add("bad", 4, -cold.recent_z, `${cold.player.name} is below their usual over the last ${cold.recent_games} games.`, () => goToPlayer(cold.player.id));
  }
  return items.sort((x, y) => x.tier - y.tier || y.weight - x.weight).slice(0, 7);
}

function takeawayList(items) {
  const icon = { good: "▲", bad: "▼", info: "●" };
  return el("ul", { class: "takeaways" }, items.map((item) => el("li", { class: `tone-${item.tone}` }, [
    el("span", { class: "icon", text: icon[item.tone], "aria-hidden": "true" }),
    el("span", { class: "sr-only", text: item.tone === "good" ? "Good: " : item.tone === "bad" ? "Needs attention: " : "Note: " }),
    el("span", { text: item.text }),
    item.go ? el("button", { class: "link small", text: "see why", onclick: item.go }) : null,
  ])));
}

function miniRow(name, detail, onClick) {
  return el("div", { class: "mini-row" }, [
    onClick ? el("button", { class: "link quiet who", text: name, onclick: onClick }) : el("span", { class: "who", text: name }),
    el("div", { class: "small muted", text: detail }),
  ]);
}

function extremes(list, count) {
  const qualified = list.filter((r) => r.qualified);
  return { top: qualified.slice(0, count), bottom: qualified.slice(-count).reverse().filter((r) => !qualified.slice(0, count).includes(r)) };
}

function viewSummary() {
  const a = state.analysis;
  const t = a.team;
  const record = `${t.wins}-${t.losses}${t.overtime_losses ? `-${t.overtime_losses}` : ""}`;
  const f = extremes(a.rankings.forwards, 2);
  const d = extremes(a.rankings.defence, 1);
  const units = (list) => list.filter((u) => u.shrunk_corsi && u.toi >= a.request.min_unit_minutes * 60).sort((x, y) => y.shrunk_corsi.prob_above_average - x.shrunk_corsi.prob_above_average);
  const dPairs = units(a.units.defence_pairs);
  const fLines = units(a.units.forward_lines);
  const form = a.rankings.form;
  const ratingNote = (r, good) => `${good ? "strong" : "weak"}: ${(good ? r.strengths : r.weaknesses).join(", ") || "overall"}`;
  const unitNote = (u) => `${pct(u.corsi_pct, 0)} of shot attempts (${u.corsi_for}–${u.corsi_against}) in ${minutes(u.toi)} min`;
  const formNote = (r) => `${signed(r.recent_z, 1)} SD vs their usual (last ${r.recent_games} games)`;
  const good = el("div", { class: "card column good" }, [
    cardTitle("Going well", historyChip()),
    el("div", { class: "group-label", text: "Top-rated players" }),
    ...[...f.top, ...d.top].map((r) => miniRow(`${r.player.name} (${positionShort(r.player.position)})`, ratingNote(r, true), () => goToPlayer(r.player.id))),
    el("div", { class: "group-label", text: "Best units" }),
    ...[dPairs[0], fLines[0]].filter(Boolean).map((u) => miniRow(names(u.players), unitNote(u), () => { state.sub.lines = "units"; setView("lines"); })),
    el("div", { class: "group-label", text: "In form" }),
    ...form.filter((r) => r.recent_z > 0).slice(0, 2).map((r) => miniRow(r.player.name, formNote(r), () => goToPlayer(r.player.id))),
  ]);
  const bad = el("div", { class: "card column bad" }, [
    cardTitle("Needs attention", historyChip()),
    el("div", { class: "group-label", text: "Lowest-rated players" }),
    ...[...f.bottom, ...d.bottom].map((r) => miniRow(`${r.player.name} (${positionShort(r.player.position)})`, ratingNote(r, false), () => goToPlayer(r.player.id))),
    el("div", { class: "group-label", text: "Toughest units" }),
    ...[dPairs[dPairs.length - 1], fLines[fLines.length - 1]].filter((u) => u && dPairs.length + fLines.length > 2).map((u) => miniRow(names(u.players), unitNote(u), () => { state.sub.lines = "units"; setView("lines"); })),
    el("div", { class: "group-label", text: "Below their usual" }),
    ...form.filter((r) => r.recent_z < 0).slice(-2).reverse().map((r) => miniRow(r.player.name, formNote(r), () => goToPlayer(r.player.id))),
  ]);
  const kpis = tiles([
    { label: "Record", value: record, note: `${t.games} game${t.games === 1 ? "" : "s"}` },
    { label: "Goals", value: `${t.goals_for}–${t.goals_against}` },
    { label: "Shot share", value: pct(t.shot_share, 0), note: `${t.shots_on_goal_for}–${t.shots_on_goal_against} on goal · attempts ${t.attempts_for}–${t.attempts_against}` },
    { label: "Expected-goals share", value: pct(t.xg_share, 0), note: `quality of chances, ${fmt(t.xg_for, 1)}–${fmt(t.xg_against, 1)}` },
  ]);
  const extra = tiles([
    { label: "Even-strength CF%", value: pct(t.even_strength_corsi_pct, 0), note: "shot attempts, even strength" },
    { label: "Power play", value: pct(t.power_play_pct, 0), note: `${t.power_play_goals}/${t.power_play_chances}` },
    { label: "Penalty kill", value: pct(t.penalty_kill_pct, 0), note: `${t.power_play_goals_against} allowed in ${t.times_short_handed}` },
    { label: "PDO (luck gauge)", value: fmt(t.pdo, 1), note: "shooting % + save %; ~100 is normal" },
  ]);
  const log = t.game_log;
  const chart = log.length >= 2
    ? chartCard("Results by game", "Goal differential; green = win, red = loss.", (c) => hBarChart(c, log.map((g) => ({ label: `${g.date.slice(5)} ${g.opponent}`, value: g.goals_for - g.goals_against, color: g.goals_for >= g.goals_against ? css("--good") : css("--critical") })), { valueFormat: (v) => signed(v, 0), labelWidth: 220 }), null)
    : chartCard("Shots on goal by period", "Where the game was won or lost.", (c) => groupedColumns(c, t.periods.map((p) => ({ label: p.period <= 3 ? `Period ${p.period}` : "OT", values: [p.shots_on_goal_for, p.shots_on_goal_against] })), [{ name: "Our shots on goal", color: css("--series-1") }, { name: "Their shots on goal", color: css("--series-2") }]), periodTable);
  return page("Summary", `${a.team_name}: what stands out in the games in scope.`,
    kpis,
    el("div", { class: "card" }, [cardTitle("Key takeaways"), takeawayList(takeaways(a))]),
    el("div", { class: "grid two", style: "margin-top:16px" }, [good, bad]),
    practiceSummary(a),
    el("div", { style: "margin-top:16px" }, [chart]),
    more("More team numbers", extra));
}

// ---------- Rankings ----------

function termList(labels) {
  return labels.flatMap((label, i) => (i ? [", ", term(label)] : [term(label)]));
}

function scoreColor(rating) {
  return window.Charts.divergingColor(Math.max(-1, Math.min(1, (rating - 50) / 15)));
}

function rankingList(rows) {
  // Ratings cluster near 50, so the bar spans 35–65 (±1.5 SD).
  const scale = (v) => Math.max(0, Math.min(100, ((v - 35) / 30) * 100));
  return el("div", { class: "rank-list" }, rows.map((r) => el("div", { class: `rank-row ${r.qualified ? "" : "dim"}`, onclick: () => goToPlayer(r.player.id) }, [
    el("div", { class: "rank-num", text: r.rank ?? "–" }),
    el("div", { class: "rank-name" }, [
      el("div", { class: "who", text: `${r.player.jersey ?? ""} ${r.player.name}`.trim() }),
      el("div", { class: "small" }, [
        r.strengths.length ? el("span", { class: "ok" }, ["+ ", ...termList(r.strengths)]) : null,
        r.strengths.length && r.weaknesses.length ? "   " : null,
        r.weaknesses.length ? el("span", { class: "err" }, ["− ", ...termList(r.weaknesses)]) : null,
        r.qualified ? null : el("span", { class: "muted", text: `under ${state.request.min_minutes} min, not ranked` }),
      ]),
    ]),
    el("div", { class: "rank-bar", title: `Rating ${fmt(r.rating, 0)} (50 = position average)` }, [
      el("div", { class: "rank-mid" }),
      el("div", { class: "rank-fill", style: `left:${Math.min(scale(50), scale(r.rating))}%;width:${Math.abs(scale(r.rating) - scale(50))}%;background:${scoreColor(r.rating)}` }),
    ]),
    el("div", { class: "rank-value", text: fmt(r.rating, 0) }),
    el("div", { class: "rank-cats" }, [["Off", r.offence], ["Def", r.defence], ["Puck", r.puck_play]].map(([label, v]) => {
      const chip = el("span", {
        class: "cat term", style: v === null ? "" : `background:${scoreColor(v)};color:${Math.abs(v - 50) > 8 ? "#fff" : "inherit"}`,
      }, [`${label} ${fmt(v, 0)}`]);
      window.Charts.explain(chip, label);
      return chip;
    })),
  ])));
}

function sparkline(values, width = 90, height = 22) {
  const { svg } = window.Charts;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const x = (i) => (i / Math.max(1, values.length - 1)) * (width - 6) + 3;
  const y = (v) => height - 3 - ((v - min) / ((max - min) || 1)) * (height - 6);
  const root = svg("svg", { width, height, class: "chart", "aria-hidden": "true" });
  root.append(svg("path", { d: values.map((v, i) => `${i ? "L" : "M"}${x(i)},${y(v)}`).join(""), fill: "none", stroke: css("--deemphasis"), "stroke-width": 1.5 }));
  const last = values.length - 1;
  root.append(svg("circle", { cx: x(last), cy: y(values[last]), r: 3, fill: css("--series-1") }));
  return root;
}

function formList(rows) {
  return el("div", { class: "rank-list" }, rows.map((r) => el("div", { class: "rank-row compact", onclick: () => goToPlayer(r.player.id) }, [
    el("div", { class: "rank-name" }, [
      el("div", { class: "who", text: r.player.name }),
      el("div", { class: "small muted", text: `${r.source === "InstatIndex" ? "InStat Index" : "Rating"} ${fmt(r.recent_mean, 0)} lately vs ${fmt(r.baseline_mean, 0)} usual` }),
    ]),
    sparkline(r.series.map(([, v]) => v)),
    el("span", { class: `badge ${r.recent_z >= 0 ? "up" : "down"}`, text: `${r.recent_z >= 0 ? "▲" : "▼"} ${fmt(Math.abs(r.recent_z), 1)} SD` }),
  ])));
}

function viewRankings() {
  const a = state.analysis;
  const r = a.rankings;
  const rows = state.rankingTab === "defence" ? r.defence : r.forwards;
  const tabs = el("div", { class: "tabs" }, [["forwards", `Forwards (${r.forwards.length})`], ["defence", `Defence (${r.defence.length})`]].map(([id, label]) => el("button", { class: state.rankingTab === id ? "on" : "", text: label, onclick: () => { state.rankingTab = id; render(); } })));
  const hot = r.form.filter((f) => f.recent_z > 0).slice(0, 5);
  const cold = r.form.filter((f) => f.recent_z < 0).slice(-5).reverse();
  const formSource = r.form[0]?.source === "CompositeRating" ? "their game ratings" : "InStat Index (loaded games plus InStat's recent-games history)";
  const breakdown = el("div");
  dataTable(breakdown, [
    { key: "name", label: "Player", left: true, value: (x) => x.player.name },
    { key: "rating", label: "Rating", format: (v) => fmt(v, 0), tone: "higher" },
    ...rows[0]?.components.map((c, i) => ({ key: `c${i}`, label: c.metric, tone: "higher", value: (x) => x.components[i].score, render: (x) => `${fmt(x.components[i].value, 1)} (${signed(x.components[i].score, 1)})`, title: "value (standing vs position, in SDs)" })) ?? [],
  ], rows, { sortKey: "rating" });
  return page("Rankings", "Who is playing best at each position, and who is above or below their own usual level.",
    el("div", {}, [
      el("div", { class: "card" }, [
        cardTitle("Position rankings"),
        el("p", { class: "desc", text: "Rating: 50 = average at the position, 60+ = clearly above, 40- = clearly below. Off / Def / Puck show where it comes from. Click a player for their card." }),
        tabs, rankingList(rows),
      ]),
      el("div", { class: "card", style: "margin-top:16px" }, [
        cardTitle("Form: last few games vs their usual", historyChip()),
        el("p", { class: "desc", text: `Compares each player's last ${r.recent_window} games with the rest of their games, using ${formSource}.` }),
        el("div", { class: "group-label", text: "Above their usual" }),
        hot.length ? formList(hot) : el("p", { class: "muted small", text: "Nobody yet." }),
        el("div", { class: "group-label", text: "Below their usual" }),
        cold.length ? formList(cold) : el("p", { class: "muted small", text: "Nobody yet." }),
      ]),
    ]),
    more("How the rating is built, stat by stat", el("p", { class: "small muted", text: "Each stat is compared with same-position teammates (in standard deviations; positive is always good), after pulling low-ice-time players toward the average. Offence, Defence and Puck play each count one third." }), breakdown));
}

// ---------- Single game ----------

const GAME_TABS = [["overview", "Overview"], ["shots", "Shots"], ["matchups", "Matchups"], ["stats", "Team stats"]];

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
  const [tabs, current] = pageTabs("gameTab", GAME_TABS);
  const body = { overview: gameOverview, shots: gameShots, matchups: gameMatchups, stats: gameTeamStats }[current](a, timeline);
  return page("Single game", "Pick a game to see its shifts, goals and how each player compared with their usual.",
    el("div", { style: "margin-bottom:14px" }, [select]), tabs, ...[].concat(body));
}

function gameOverview(a, timeline) {
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
  const vsUsual = a.rankings.form
    .filter((r) => r.latest_date === timeline.date || a.timelines.length === 1)
    .sort((x, y) => y.latest_z - x.latest_z);
  const usualRow = (r) => miniRow(r.player.name, `${r.source === "InstatIndex" ? "InStat Index" : "Rating"} ${fmt(r.latest_value, 0)} vs usual ${fmt(r.baseline_mean, 0)} (${signed(r.latest_z, 1)} SD)`, () => goToPlayer(r.player.id));
  const compareCard = el("div", { class: "grid two" }, [
    el("div", { class: "card column good" }, [cardTitle("Above their usual in this game", gameChip(timeline.date.slice(5)), historyChip()), ...vsUsual.filter((r) => r.latest_z > 0.5).slice(0, 5).map(usualRow), vsUsual.some((r) => r.latest_z > 0.5) ? null : el("p", { class: "muted small", text: "Nobody clearly above their usual." })]),
    el("div", { class: "card column bad" }, [cardTitle("Below their usual in this game", gameChip(timeline.date.slice(5)), historyChip()), ...vsUsual.filter((r) => r.latest_z < -0.5).slice(-5).reverse().map(usualRow), vsUsual.some((r) => r.latest_z < -0.5) ? null : el("p", { class: "muted small", text: "Nobody clearly below their usual." })]),
  ]);
  const badgeRows = comparisons.filter((x) => x.badges.length);
  const badges = badgeRows.length ? more("Season bests and unusual numbers", ...badgeRows.map(({ p, badges: list }) => el("div", { class: "callout", style: "margin-top:6px" }, [
    playerLink(p.player),
    el("div", { class: "pill-row" }, list.map((b) => el("span", { class: "badge", text: b.badge }))),
  ]))) : null;
  return [compareCard, el("div", { style: "margin-top:16px" }, [shiftCard]), badges];
}

function periodName(period) {
  return period <= 3 ? `Period ${period}` : "Overtime";
}

/** Hover text for a charted shot; `game` adds the date for multi-game plots. */
function shotTip(shot, { shooter = true, game = false } = {}) {
  return {
    title: shooter ? shot.shooter?.name ?? "Unknown shooter" : shot.goal ? "Goal" : "Shot",
    rows: [
      ...(shooter ? [{ value: shot.goal ? "goal" : "shot", name: "" }] : []),
      { value: periodName(shot.period), name: "" },
      { value: `${fmt(shotDistance(shot.at), 0)} ft`, name: "from the net" },
      ...(game ? [{ value: shot.date, name: "game" }] : []),
    ],
  };
}

/** Shots, goals and distance per shooter, most shots first. */
function shootersTable(container, shots) {
  const byShooter = new Map();
  for (const shot of shots) {
    const key = shot.shooter?.id ?? "";
    const row = byShooter.get(key) || { shooter: shot.shooter, shots: 0, goals: 0, distance: 0, close: 0 };
    row.shots += 1;
    row.goals += shot.goal ? 1 : 0;
    row.distance += shotDistance(shot.at);
    row.close += shotDistance(shot.at) <= 20 ? 1 : 0;
    byShooter.set(key, row);
  }
  dataTable(container, [
    { key: "name", label: "Shooter", left: true, value: (r) => r.shooter?.name ?? "Unknown" },
    { key: "shots", label: "Shots" },
    { key: "goals", label: "Goals" },
    { key: "close", label: "Within 20 ft", title: "Shots from 20 feet of the net or closer", tone: "higher" },
    { key: "avg", label: "Avg distance (ft)", value: (r) => r.distance / r.shots, format: (v) => fmt(v, 0), tone: "lower" },
  ], [...byShooter.values()], { sortKey: "shots", onRow: (r) => r.shooter && goToPlayer(r.shooter.id) });
}

function gameShots(a, timeline) {
  if (!timeline.shots.length) return emptyNote("This game's match report has no shooting chart.");
  const periods = [...new Set(timeline.shots.map((x) => x.period))].sort();
  const chosen = periods.includes(Number(state.shotPeriod)) ? Number(state.shotPeriod) : "all";
  const shots = timeline.shots.filter((x) => chosen === "all" || x.period === chosen);
  const pick = (value) => { state.shotPeriod = value; render(); };
  const filter = el("div", { class: "segmented", style: "margin-bottom:14px" }, [
    el("button", { class: chosen === "all" ? "on" : "", text: "Whole game", onclick: () => pick("all") }),
    ...periods.map((p) => el("button", { class: chosen === p ? "on" : "", text: periodName(p), onclick: () => pick(p) })),
  ]);
  const plot = chartCard("Where we shot from", "Every shot on InStat's shooting chart, placed on a standard rink. Hover a dot for the shooter and distance.", (c) => shotPlot(c, shots, { tip: (x) => shotTip(x) }), null);
  const table = el("div", { class: "card" }, [cardTitle("Shooters"), el("div")]);
  shootersTable(table.lastChild, shots);
  return [filter, el("div", { class: "grid two" }, [plot, table])];
}

function opponentName(opponent) {
  return opponent.jersey === null ? opponent.surname : `#${opponent.jersey} ${opponent.surname}`;
}

/** Our win % in a set of puck battles, for colouring (null when there were none). */
function battleWinPct(battles) {
  const total = battles.won + battles.lost;
  return total ? (100 * battles.won) / total : null;
}

/** Bars of battles won minus lost around zero, labelled won–lost. rows: [{label, battles, note}] */
function battleBalanceChart(container, rows) {
  const net = (b) => b.won - b.lost;
  const most = Math.max(1, ...rows.map((r) => Math.abs(net(r.battles))));
  hBarChart(container, rows.map((r) => ({
    label: r.label,
    value: net(r.battles),
    color: net(r.battles) === 0 ? css("--deemphasis") : window.Charts.divergingColor(net(r.battles) / most),
    battles: r.battles,
    note: r.note,
  })), { min: -most, max: most, valueFormat: (v, row) => `${row.battles.won}–${row.battles.lost}`, labelWidth: 150, valueName: "won–lost", tickFormat: (t) => signed(t, 0) });
}

const hadBattles = (x) => x.battles.won + x.battles.lost > 0;
const hadHits = (x) => x.hits.given + x.hits.taken > 0;

function gameMatchups(a, timeline) {
  const m = timeline.matchups;
  if (!m.opponents.length) return emptyNote("This game's match report has no challenge or hits distribution page.");
  const battlers = m.opponents.filter(hadBattles);
  const opponentsCard = battlers.length ? chartCard("Their skaters in puck battles", "Each opponent skater's one-on-one battles against us, most battles first. Bars show battles we won minus battles they won (right = we came out ahead); labels are won–lost from our side.", (c) => battleBalanceChart(c, battlers.map((o) => ({
    label: opponentName(o.opponent),
    battles: o.battles,
    note: `we won ${o.battles.won}, they won ${o.battles.lost}`,
  }))), (c) => dataTable(c, [
    { key: "name", label: "Opponent", left: true, value: (o) => opponentName(o.opponent) },
    { key: "total", label: "Battles", value: (o) => o.battles.won + o.battles.lost },
    { key: "won", label: "We won", value: (o) => o.battles.won },
    { key: "lost", label: "They won", value: (o) => o.battles.lost },
    { key: "pct", label: "Our win %", value: (o) => battleWinPct(o.battles), format: (v) => pct(v, 0), tone: "higher" },
    { key: "given", label: "Hits by us", value: (o) => o.hits.given },
    { key: "taken", label: "Hits by them", value: (o) => o.hits.taken },
  ], m.opponents, { sortKey: "total" })) : null;
  const cellAt = new Map(m.cells.map((cell) => [`${cell.player}:${cell.opponent}`, cell]));
  const gridCard = battlers.length ? chartCard("Who battled whom", "Our skaters down the side, theirs across the top; each square is won–lost from our side. Click a square to open our player's card.", (c) => heatmap(c, { rows: m.players.map((p) => p.name), columns: m.opponents.map((o) => opponentName(o.opponent)) }, (i, j) => {
    const cell = cellAt.get(`${i}:${j}`);
    if (!cell || !hadBattles(cell)) return null;
    return {
      value: battleWinPct(cell.battles),
      text: `${cell.battles.won}–${cell.battles.lost}`,
      title: `${m.players[i].name} vs ${opponentName(m.opponents[j].opponent)}`,
      tip: [
        { value: String(cell.battles.won), name: "we won" },
        { value: String(cell.battles.lost), name: "they won" },
        ...(hadHits(cell) ? [{ value: `${cell.hits.given}–${cell.hits.taken}`, name: "hits given–taken" }] : []),
      ],
      onClick: () => goToPlayer(m.players[i].id),
    };
  }, { kind: "diverging", min: 0, max: 100, center: 50 }, { valueName: "our win %", labelWidth: 150 }), null) : null;
  const hits = m.cells.filter(hadHits).map((cell) => ({ player: m.players[cell.player], opponent: m.opponents[cell.opponent].opponent, hits: cell.hits }));
  const hitsCard = el("div", { class: "card" }, [cardTitle("Hits between skaters"), hits.length ? el("div") : emptyNote("No hits recorded in this game.")]);
  if (hits.length) {
    dataTable(hitsCard.lastChild, [
      { key: "ours", label: "Our skater", left: true, value: (h) => h.player.name },
      { key: "theirs", label: "Their skater", left: true, value: (h) => opponentName(h.opponent) },
      { key: "given", label: "Hits given", value: (h) => h.hits.given },
      { key: "taken", label: "Hits taken", value: (h) => h.hits.taken },
    ], hits, { sortKey: "given", onRow: (h) => goToPlayer(h.player.id) });
  }
  return [
    ...[opponentsCard, gridCard].filter(Boolean).map((card) => el("div", { style: "margin-bottom:16px" }, [card])),
    hitsCard,
  ];
}

function gameTeamStats(a, timeline) {
  const groupOrder = [...new Set(timeline.team_stats.map((r) => r.group))];
  const statsRows = [...timeline.team_stats].sort((x, y) => groupOrder.indexOf(x.group) - groupOrder.indexOf(y.group));
  const card = el("div", { class: "card" }, [cardTitle(`Every team stat vs ${timeline.opponent}`), el("div")]);
  dataTable(card.lastChild, [
    { key: "group", label: "Section", left: true },
    { key: "label", label: "Stat", left: true },
    { key: "ours", label: "Us", value: (r) => r.ours.text },
    { key: "theirs", label: "Them", value: (r) => r.theirs.text },
  ], statsRows, {});
  return card;
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
  const tabs = el("div", { class: "segmented", style: "margin-bottom:14px" }, UNIT_TABS.map(([id, label]) => el("button", { class: state.unitTab === id ? "on" : "", text: `${label} (${a.units[id].length})`, onclick: () => { state.unitTab = id; render(); } })));
  const special = state.unitTab === "power_play" || state.unitTab === "penalty_kill";
  const prior = a.units.priors.find(([k]) => ({ defence_pairs: "DefencePair", forward_lines: "ForwardLine", full_units: "FullUnit" })[state.unitTab] === k);
  // Five-man and special-teams units play far fewer minutes, so they get a lower bar.
  const minMinutes = { defence_pairs: a.request.min_unit_minutes, forward_lines: a.request.min_unit_minutes, full_units: Math.min(1, a.request.min_unit_minutes), power_play: 0.5, penalty_kill: 0.5 }[state.unitTab];
  const ranked = list.filter((u) => u.toi >= minMinutes * 60);
  const hiddenNote = ranked.length < list.length ? ` ${list.length - ranked.length} unit(s) under ${minMinutes} min are only in the table below.` : "";
  const labelWidth = list.some((u) => u.players.length >= 4) ? 340 : special ? 220 : 190;
  let chart;
  if (!special) {
    chart = chartCard("Shot-attempt share, adjusted for sample size",
      `Dot = best estimate of each unit's true share of shot attempts; whiskers = 90% range. Short-time units are pulled toward the team average (the line) until they earn their number.${hiddenNote}`,
      (c) => hBarChart(c, [...ranked].sort((x, y) => (y.shrunk_corsi?.estimate.value ?? 0) - (x.shrunk_corsi?.estimate.value ?? 0)).map((u) => ({
        label: shortNames(u.players),
        value: u.shrunk_corsi?.estimate.value ?? null,
        low: u.shrunk_corsi?.estimate.low,
        high: u.shrunk_corsi?.estimate.high,
        note: `raw ${pct(u.corsi_pct, 0)} over ${minutes(u.toi)} min · ${probabilityWords(u.shrunk_corsi?.prob_above_average)}`,
      })), { dots: true, reference: prior ? prior[1].mean * 100 : undefined, referenceLabel: "team avg", valueFormat: (v) => pct(v, 0), intervalName: "90% range", labelWidth }),
      null);
  } else {
    chart = chartCard(state.unitTab === "power_play" ? "Power-play shots per 60 minutes" : "Shots against per 60 minutes on the penalty kill",
      `${state.unitTab === "power_play" ? "Higher is better." : "Lower is better."}${hiddenNote}`,
      (c) => hBarChart(c, ranked.map((u) => ({ label: shortNames(u.players), value: u.shots_60, note: `${minutes(u.toi)} min, ${u.goals_for + u.goals_against} goals` })), { valueFormat: (v) => fmt(v, 0), labelWidth }),
      null);
  }
  const evenColumns = [
    { key: "players", label: "Players", left: true, value: (u) => names(u.players), wrap: true },
    { key: "games", label: "GP" },
    { key: "toi", label: "Min", format: (v) => minutes(v) },
    { key: "corsi_for", label: "CF" }, { key: "corsi_against", label: "CA" },
    { key: "corsi_pct", label: "CF%", format: (v) => pct(v, 0), tone: "higher" },
    { key: "shrunk", label: "Adj. CF%", tone: "higher", value: (u) => u.shrunk_corsi?.estimate.value, render: (u) => u.shrunk_corsi ? `${pct(u.shrunk_corsi.estimate.value, 0)} (${fmt(u.shrunk_corsi.estimate.low, 0)}–${fmt(u.shrunk_corsi.estimate.high, 0)})` : "—", title: "Shrunk toward team average; 90% range" },
    { key: "prob", label: "Above avg?", value: (u) => u.shrunk_corsi?.prob_above_average, format: (v) => (v === null || v === undefined ? "—" : `${Math.round(v * 100)}%`), tone: "higher" },
    { key: "goals_for", label: "GF", tone: "higher" }, { key: "goals_against", label: "GA", tone: "lower" },
    { key: "corsi_for_60", label: "CF/60", format: (v) => fmt(v, 0), tone: "higher" },
    { key: "corsi_against_60", label: "CA/60", format: (v) => fmt(v, 0), tone: "lower" },
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
  const body = el("div");
  const tableCard = more("All combinations as a table (sorted by ice time; grey = under the minimum)", body);
  dataTable(body, special ? specialColumns : evenColumns, list, { sortKey: "toi", dim: (u) => u.toi < minMinutes * 60 });
  return page("Lines & pairs", "Every combination InStat tracked, pooled over the games in scope.", tabs, chart, tableCard);
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
    el("div", { style: "margin-bottom:12px" }, [el("label", {}, ["Colour by ", select])]), heat, pairExplorer(a, players));
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
  const ratingOf = (p) => [...a.rankings.forwards, ...a.rankings.defence].find((r) => r.player.id === p.player.id);
  const formOf = (p) => a.rankings.form.find((r) => r.player.id === p.player.id);
  const keyColumns = [
    { key: "name", label: "Player", left: true, value: (p) => `${p.player.jersey ?? ""} ${p.player.name}`.trim(), render: (p) => el("span", {}, [ratingDot(ratingOf(p)?.rating), `${p.player.jersey ?? ""} ${p.player.name}`.trim()]) },
    { key: "pos", label: "Pos", left: true, value: (p) => positionShort(p.player.position) },
    { key: "gp", label: "GP", value: (p) => p.totals.games },
    { key: "toi", label: "TOI/GP", value: (p) => p.totals.toi / Math.max(1, p.totals.games), format: (v) => clock(v) },
    { key: "pts", label: "P", value: (p) => p.totals.points, tone: "higher" },
    { key: "rating", label: "Rating", value: (p) => ratingOf(p)?.rating, format: (v) => fmt(v, 0), title: "50 = average at their position", tone: "higher" },
    { key: "form", label: "Form", value: (p) => formOf(p)?.recent_z, format: (v) => (v === undefined || v === null ? "—" : `${v >= 0 ? "▲" : "▼"} ${fmt(Math.abs(v), 1)}`), title: "Last few games vs their usual, in standard deviations", tone: "higher" },
    { key: "rel", label: "Shot share vs team", value: (p) => p.shares.corsi_rel, format: (v) => signed(v, 1), title: "On-ice CF% minus team CF% without them", tone: "higher" },
    { key: "idx", label: "InStat", value: (p) => p.instat_mean, format: (v) => fmt(v, 0), tone: "higher" },
  ];
  const allColumns = [
    ...keyColumns,
    { key: "g", label: "G", value: (p) => p.totals.goals },
    { key: "a", label: "A", value: (p) => p.totals.assists },
    { key: "pm", label: "+/-", value: (p) => p.totals.plus_minus, format: (v) => signed(v, 0), tone: "higher" },
    { key: "p60", label: "P/60", value: (p) => p.rates.points, format: (v) => fmt(v, 1), tone: "higher" },
    { key: "s60", label: "Shots/60", value: (p) => p.rates.shots, format: (v) => fmt(v, 1), tone: "higher" },
    { key: "xg60", label: "xG/60", value: (p) => p.rates.xg, format: (v) => fmt(v, 2), tone: "higher" },
    { key: "cf", label: "CF%", value: (p) => p.shares.corsi_pct, format: (v) => pct(v, 0), tone: "higher" },
    { key: "adj", label: "Adj. CF%", value: (p) => p.shrunk_corsi?.estimate.value, format: (v) => pct(v, 0), tone: "higher" },
    { key: "gfp", label: "EV goal share", value: (p) => p.shares.goals_pct, format: (v) => pct(v, 0), tone: "higher" },
    { key: "bat", label: "Battles won", value: (p) => p.shares.battles_pct?.value, format: (v) => pct(v, 0), tone: "higher" },
    { key: "ozl", label: "Own-zone losses", value: (p) => p.totals.puck_losses_defensive_zone, title: "Puck losses in our own zone, the costliest kind", tone: "lower" },
    { key: "carry", label: "Carry-in %", value: (p) => sharePct(p.totals.entry_types.carry, p.totals.entries), format: (v) => pct(v, 0), title: "Zone entries carried in rather than passed or dumped", tone: "higher" },
  ];
  const columns = state.playerColumns === "all" ? allColumns : keyColumns;
  const toggle = el("button", { class: "small", text: state.playerColumns === "all" ? "Key columns only" : "All columns", onclick: () => { state.playerColumns = state.playerColumns === "all" ? "key" : "all"; render(); } });
  const card = el("div", { class: "card" }, [el("div", { class: "card-head" }, [el("p", { class: "desc", text: "Click a player for their card. Rating = position ranking (50 = average); Form = recent games vs their usual." }), el("div", { class: "actions" }, [toggle])]), body]);
  dataTable(body, columns, skaters, { sortKey: "toi", onRow: (p) => { state.player = p.player.id; render(); window.scrollTo(0, 0); }, dim: (p) => !p.qualified });
  return page("Players", "Season numbers for every skater in scope.", card);
}

const PLAYER_TABS = [["overview", "Overview"], ["shooting", "Shooting"], ["puck", "Puck play"], ["matchups", "Matchups"], ["numbers", "All numbers"]];

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
  const [tabs, current] = pageTabs("playerTab", PLAYER_TABS);
  const body = { overview: playerOverview, shooting: playerShooting, puck: playerPuckPlay, matchups: playerMatchups, numbers: playerNumbers }[current](a, s);
  return [back, head, kpis, tabs, ...[].concat(body)];
}

function playerOverview(a, s) {
  const PERCENTILE_ORDER = ["InStat Index", "Points/60", "Shots/60", "xG/60", "CF%", "CF% rel", "Passes/60", "Recoveries/60", "Battles won %", "Blocks/60"];
  const ranking = [...a.rankings.forwards, ...a.rankings.defence].find((r) => r.player.id === s.player.id);
  const form = a.rankings.form.find((r) => r.player.id === s.player.id);
  const ratingCard = ranking ? chartCard(`Rating ${fmt(ranking.rating, 0)}${ranking.rank ? `, #${ranking.rank} of the ${s.player.position === "Defence" ? "defence" : "forwards"}` : ""}`, `Each stat vs other ${s.player.position === "Defence" ? "defencemen" : "forwards"} (right = better).${form ? ` Form: ${signed(form.recent_z, 1)} SD vs their usual over the last ${form.recent_games} games.` : ""}`, (c) => hBarChart(c, ranking.components.filter((x) => x.score !== null).map((x) => ({ label: x.metric, value: x.score, color: x.score >= 0 ? css("--div-pos") : css("--div-neg"), note: `value ${fmt(x.value, 2)}` })), { min: -3, max: 3, valueFormat: (v) => signed(v, 1), labelWidth: 160, valueName: "SDs vs position" }), null) : null;
  const pctRows = PERCENTILE_ORDER.filter((label) => label in s.percentiles).map((label) => ({ label, value: s.percentiles[label] }));
  const percentileCard = chartCard("Where they rank on this team", "Percentile among qualified skaters (50 = middle of the team).", (c) => (pctRows.length ? percentileBars(c, pctRows) : c.replaceChildren(el("p", { class: "muted", text: "Below the minimum ice time for ranking." }))), (c) => dataTable(c, [{ key: "label", label: "Metric", left: true }, { key: "value", label: "Percentile", format: (v) => fmt(v, 0) }], pctRows));
  const trendPoints = s.trend.map((p, i) => ({ ...p, i }));
  const trendCard = chartCard("InStat Index over time", "Filled dots are loaded games; hollow dots come from InStat's recent-games table.", (c) => lineChart(c, [{ name: "InStat Index", color: css("--series-1"), points: trendPoints.map((p) => ({ x: p.i, y: p.instat_index, hollow: !p.loaded, label: p.opponent })) }], { xFormat: (i) => trendPoints[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0) }), (c) => dataTable(c, [
    { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
    { key: "instat_index", label: "InStat", format: (v) => fmt(v, 0) }, { key: "toi", label: "TOI", format: (v) => clock(v) },
    { key: "points", label: "P" }, { key: "shots", label: "Shots" }, { key: "plus_minus", label: "+/-", format: (v) => signed(v, 0) },
    { key: "loaded", label: "Source", format: (v) => (v ? "loaded game" : "InStat history") },
  ], s.trend), { source: historyChip() });
  const toiCard = chartCard("Ice time per game", "", (c) => lineChart(c, [{ name: "TOI", color: css("--series-1"), points: trendPoints.map((p) => ({ x: p.i, y: p.toi / 60, hollow: !p.loaded, label: p.opponent })) }], { xFormat: (i) => trendPoints[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0) }), null, { source: historyChip() });
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
  return [
    el("div", { class: "grid two" }, [ratingCard, trendCard, partnerCard, unitsCard].filter(Boolean)),
    more("More: percentiles, ice time, selected game", el("div", { class: "grid two" }, [percentileCard, toiCard, focus].filter(Boolean))),
  ];
}

function playerShooting(a, s) {
  const t = s.totals;
  const several = new Set(s.charted_shots.map((x) => x.game)).size > 1;
  const cards = [
    s.charted_shots.length ? chartCard("Every shot they took", `Where InStat's shooting chart drew each shot${several ? " over the games in scope" : ""}. Hover a dot for the period and distance.`, (c) => shotPlot(c, s.charted_shots, { tip: (x) => shotTip(x, { shooter: false, game: several }) }), null) : null,
    t.shot_zones.some((z) => z.shots > 0) ? shotMapCard("Where they shoot from", "Shots by zone over the games in scope.", t.shot_zones) : null,
    shotMixCard("How their shots came", "Set-up attacks vs counter-attacks, and power-play or short-handed shots (a shot can be in both groups). Hover for how many were on goal.", [
      ["Set-up attack", t.shot_sources.positional_attack],
      ["Counter-attack", t.shot_sources.counter_attack],
      ["Power play", t.shot_sources.power_play],
      ["Short-handed", t.shot_sources.short_handed],
    ]),
    shotMixCard("Shot types", "Only the types InStat's shots table breaks out; other shots aren't typed.", t.shot_types.map((x) => [SPLIT_NAMES.shot_type[x.kind] || x.kind, x.shots])),
  ].filter(Boolean);
  return cards.length ? el("div", { class: "grid two" }, cards) : emptyNote("No shots in the games in scope.");
}

/** Bars of shots per category. rows: [[label, {total, succeeded}]]; null when all are empty. */
function shotMixCard(title, description, rows) {
  const shown = rows.filter(([, x]) => x.total > 0);
  if (!shown.length) return null;
  return chartCard(title, description, (c) => hBarChart(c, shown.map(([label, x]) => ({ label, value: x.total, note: `${x.succeeded} on goal` })), { valueFormat: (v) => fmt(v, 0), labelWidth: 120, valueName: "shots", integer: true }), (c) => dataTable(c, [
    { key: "label", label: "", left: true, value: (r) => r[0] }, { key: "shots", label: "Shots", value: (r) => r[1].total }, { key: "on", label: "On goal", value: (r) => r[1].succeeded },
  ], shown));
}

const sharePct = (part, whole) => (whole ? (100 * part) / whole : null);

function puckTiles(t) {
  const e = t.entry_types;
  const items = [
    { label: "Puck losses", value: String(t.puck_losses), note: `${t.puck_losses_defensive_zone} in our zone${t.puck_losses ? ` (${pct(sharePct(t.puck_losses_defensive_zone, t.puck_losses), 0)})` : ""}` },
    { label: "Puck recoveries", value: String(t.puck_recoveries), note: `${t.puck_recoveries_offensive_zone} in their zone` },
    { label: "Zone entries", value: String(t.entries), note: `${e.carry} carried · ${e.pass} passed · ${e.dump_in} dumped` },
  ];
  if (t.faceoffs) {
    const dz = t.faceoffs_defensive_zone;
    const oz = t.faceoffs_offensive_zone;
    items.push({ label: "Faceoffs", value: pct(sharePct(t.faceoffs_won, t.faceoffs), 0), note: `${t.faceoffs_won}/${t.faceoffs} · our zone ${dz.succeeded}/${dz.total} · theirs ${oz.succeeded}/${oz.total}` });
  }
  return tiles(items);
}

function faceoffZonesCard(t) {
  if (!t.faceoffs) return null;
  const dz = t.faceoffs_defensive_zone;
  const oz = t.faceoffs_offensive_zone;
  const neutral = { total: t.faceoffs - dz.total - oz.total, succeeded: t.faceoffs_won - dz.succeeded - oz.succeeded };
  const rows = [["Our zone", dz], ["Neutral zone", neutral], ["Their zone", oz]].filter(([, x]) => x.total > 0);
  return chartCard("Faceoffs by zone", "Win % in each zone; neutral-zone draws are what's left of their total.", (c) => hBarChart(c, rows.map(([label, x]) => ({ label, value: sharePct(x.succeeded, x.total), note: `won ${x.succeeded} of ${x.total}` })), { min: 0, max: 100, reference: 50, valueFormat: (v) => pct(v, 0), labelWidth: 110, valueName: "won" }), (c) => dataTable(c, [
    { key: "zone", label: "", left: true, value: (r) => r[0] }, { key: "taken", label: "Taken", value: (r) => r[1].total }, { key: "won", label: "Won", value: (r) => r[1].succeeded }, { key: "pct", label: "Win %", value: (r) => sharePct(r[1].succeeded, r[1].total), format: (v) => pct(v, 0) },
  ], rows));
}

function entryTypesCard(t) {
  if (!t.entries) return null;
  const e = t.entry_types;
  const rows = [["Carried in", e.carry], ["Passed in", e.pass], ["Dumped in", e.dump_in]];
  return chartCard("How they enter the zone", "Zone entries by type. Carrying or passing keeps the puck; a dump-in gives it up to be won back.", (c) => hBarChart(c, rows.map(([label, value]) => ({ label, value, note: `${pct(sharePct(value, t.entries), 0)} of entries` })), { valueFormat: (v) => fmt(v, 0), labelWidth: 100, valueName: "entries", integer: true }), null);
}

function playerPuckPlay(a, s) {
  const t = s.totals;
  const cards = [
    t.battle_areas.some((x) => x.battles > 0) ? battleMapCard("Puck battles by area", "Where they win and lose battles (our net on the left).", t.battle_areas) : null,
    entryTypesCard(t),
    faceoffZonesCard(t),
  ].filter(Boolean);
  return [puckTiles(t), cards.length ? el("div", { class: "grid two" }, cards) : null];
}

function playerMatchups(a, s) {
  if (!s.matchups.length) return emptyNote("No head-to-head battles or hits recorded in the games in scope.");
  const severalGames = new Set(s.matchups.map((m) => m.game)).size > 1;
  const label = (m) => (severalGames ? `${opponentName(m.opponent)} (${m.date.slice(5)})` : opponentName(m.opponent));
  const battled = s.matchups.filter(hadBattles).sort((x, y) => (y.battles.won + y.battles.lost) - (x.battles.won + x.battles.lost));
  const table = (c) => dataTable(c, [
    { key: "date", label: "Date", left: true },
    { key: "opponent_team", label: "Team", left: true },
    { key: "name", label: "Opponent", left: true, value: (m) => opponentName(m.opponent) },
    { key: "total", label: "Battles", value: (m) => m.battles.won + m.battles.lost },
    { key: "won", label: "Won", value: (m) => m.battles.won },
    { key: "lost", label: "Lost", value: (m) => m.battles.lost },
    { key: "pct", label: "Win %", value: (m) => battleWinPct(m.battles), format: (v) => pct(v, 0), tone: "higher" },
    { key: "given", label: "Hits given", value: (m) => m.hits.given },
    { key: "taken", label: "Hits taken", value: (m) => m.hits.taken },
  ], s.matchups, { sortKey: "date" });
  if (!battled.length) {
    const card = el("div", { class: "card" }, [cardTitle("Opponents they met", scopeChip()), el("div")]);
    table(card.lastChild);
    return card;
  }
  return chartCard("Opponents they met", "One-on-one puck battles against each opponent skater, most battles first. Bars show battles won minus battles lost (right = came out ahead); labels are won–lost. The table adds hits given and taken.", (c) => battleBalanceChart(c, battled.map((m) => ({
    label: label(m),
    battles: m.battles,
    note: `won ${m.battles.won}, lost ${m.battles.lost}${hadHits(m) ? ` · hits ${m.hits.given}–${m.hits.taken}` : ""}${severalGames ? ` · vs ${m.opponent_team}` : ""}`,
  }))), table, { source: scopeChip() });
}

function playerNumbers(a, s) {
  const card = el("div", { class: "card" }, [cardTitle("Every InStat number (latest game in scope)"), el("div")]);
  dataTable(card.lastChild, [
    { key: "group", label: "Table", left: true },
    { key: "label", label: "Stat", left: true },
    { key: "value", label: "Value", value: (r) => r.value.text },
  ], s.focus_details);
  return card;
}

// ---------- Goalies ----------

const GOALIE_TABS = [["overview", "Overview"], ["net", "Net & body"], ["location", "Shot location"], ["types", "Shot types & situations"], ["numbers", "All numbers"]];

const SPLIT_NAMES = {
  distance: { Slot: "Slot", CloseRange: "Close range", MidRange: "Mid-range", LongRange: "Long range" },
  shot_type: { Wrist: "Wrist shots", Snap: "Snap shots", Slap: "Slap shots", Deflection: "Deflections" },
  situation: { OneOnOne: "1-on-1 with the shooter", Screened: "Screened", CleanView: "Clear view" },
  goalie_state: { Splitting: "Goalie splitting", Beaten: "Goalie beaten", Moving: "Goalie in movement" },
  body_area: {
    AboveRightShoulder: "Above right shoulder", AboveLeftShoulder: "Above left shoulder", AboveBlocker: "Above the blocker", AboveGlove: "Above the glove",
    ChestHead: "Chest, head", RightArmpit: "Right armpit", LeftArmpit: "Left armpit", UnderBlocker: "Under the blocker", UnderGlove: "Under the glove",
    RightPad: "Right pad", LeftPad: "Left pad", BetweenLegs: "Between the legs",
  },
};

const savePct = (x) => (x && x.shots ? (100 * x.saves) / x.shots : null);

/** Blue when a split's save % beats the goalie's overall, red when it falls short. */
function saveColor(value, overall) {
  if (value === null || overall === null || overall === undefined) return css("--surface-2");
  return window.Charts.divergingColor(Math.max(-1, Math.min(1, (value - overall) / 10)));
}

function saveTable(c, rows, name) {
  dataTable(c, [
    { key: "kind", label: "", left: true, format: name },
    { key: "shots", label: "Shots" },
    { key: "saves", label: "Saves" },
    { key: "goals", label: "Goals", value: (x) => x.shots - x.saves },
    { key: "pct", label: "Sv%", value: savePct, format: (v) => pct(v, 1), tone: "higher" },
  ], rows);
}

/** Save % per category of one split, against the goalie's overall save %. */
function saveSplitCard(g, split, title, description) {
  const names = SPLIT_NAMES[split];
  const rows = g.splits[split].filter((x) => x.shots > 0);
  if (!rows.length) return null;
  const overall = g.save_pct ? g.save_pct.value : null;
  return chartCard(title, description, (c) => hBarChart(c, rows.map((x) => ({
    label: names[x.kind] || x.kind,
    value: savePct(x),
    color: saveColor(savePct(x), overall),
    note: `${x.saves} saves on ${x.shots} shots`,
  })), { min: 0, max: 100, reference: overall ?? undefined, referenceLabel: "overall", valueFormat: (v) => pct(v, 0), labelWidth: 160, valueName: "save %" }), (c) => saveTable(c, g.splits[split], (k) => names[k] || k));
}

function goalieNetCard(g) {
  const cells = g.splits.net_area.map((x) => ({ ...x, area: x.kind }));
  if (!cells.some((x) => x.shots > 0)) return null;
  const overall = g.save_pct ? g.save_pct.value : null;
  return chartCard("Where shots were headed on the net", "Seen from the shooter's side (glove on the right for a regular catcher). Each ninth shows save %, then saves / shots on goal; red = below their overall save %.", (c) => netMap(c, cells, (x) => {
    const data = x || { shots: 0, saves: 0 };
    const fill = data.shots ? saveColor(savePct(data), overall) : css("--surface-2");
    return {
      fill,
      ink: data.shots ? inkOn(fill) : css("--text-muted"),
      lines: data.shots ? [pct(savePct(data), 0), `${data.saves} / ${data.shots}`] : ["—"],
      tip: [{ value: `${data.shots}`, name: "shots" }, { value: `${data.saves}`, name: "saves" }, { value: `${data.shots - data.saves}`, name: "goals" }],
    };
  }), (c) => saveTable(c, cells, netAreaName));
}

function goalieZoneCard(g) {
  const zones = g.splits.zone.map((x) => ({ ...x, zone: x.kind }));
  if (!zones.some((x) => x.shots > 0)) return null;
  const overall = g.save_pct ? g.save_pct.value : null;
  return chartCard("Save % by shot zone", "Where the shots they faced came from (net at the top). Each zone shows save %, then saves / shots; red = below their overall save %.", (c) => zoneMap(c, zones, (x) => {
    const data = x || { shots: 0, saves: 0 };
    const fill = data.shots ? saveColor(savePct(data), overall) : css("--surface-2");
    return {
      fill,
      ink: data.shots ? inkOn(fill) : css("--text-muted"),
      lines: data.shots ? [pct(savePct(data), 0), `${data.saves} / ${data.shots}`] : ["—"],
      tip: [{ value: `${data.shots}`, name: "shots" }, { value: `${data.saves}`, name: "saves" }, { value: `${data.shots - data.saves}`, name: "goals" }],
    };
  }), (c) => saveTable(c, zones, shotZoneName));
}

function reboundCard(g) {
  const r = g.rebounds;
  const saves = r ? r.uncontrolled + r.controlled + r.frozen_after_rebound + r.frozen_immediately : 0;
  if (!saves) return null;
  const rows = [
    { label: "Froze it straight away", value: r.frozen_immediately, color: css("--good") },
    { label: "Controlled the rebound", value: r.controlled, color: css("--good") },
    { label: "Froze it after a rebound", value: r.frozen_after_rebound, color: css("--deemphasis") },
    { label: "Loose rebound", value: r.uncontrolled, color: css("--critical") },
  ];
  return chartCard("What happened after each save", "Share of saves by what the goalie did with the puck. Loose rebounds give the other team a second chance.", (c) => hBarChart(c, rows.map((x) => ({ ...x, value: (100 * x.value) / saves, note: `${x.value} of ${saves} saves` })), { min: 0, max: 100, valueFormat: (v) => pct(v, 0), labelWidth: 170, valueName: "of saves" }), (c) => dataTable(c, [
    { key: "label", label: "", left: true }, { key: "value", label: "Saves" }, { key: "share", label: "Share", value: (x) => (100 * x.value) / saves, format: (v) => pct(v, 0) },
  ], rows));
}

function goalieTrendCard(g) {
  const points = g.trend.map((p, i) => ({ ...p, i }));
  return chartCard("Save % by game", "Hollow dots come from InStat's recent-games table.", (c) => lineChart(c, [{ name: "Save %", color: css("--series-1"), points: points.map((p) => ({ x: p.i, y: p.save_pct, hollow: !p.loaded, label: `${p.saves}/${p.shots_against} vs ${p.opponent}` })) }], { xFormat: (i) => points[i]?.date.slice(5) || "", yFormat: (v) => pct(v, 0) }), (c) => dataTable(c, [
    { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true }, { key: "shots_against", label: "SA" }, { key: "saves", label: "Saves" }, { key: "save_pct", label: "Sv%", format: (v) => pct(v, 1) },
  ], g.trend), { source: historyChip() });
}

function goalieNumbers(g) {
  const card = el("div", { class: "card" }, [cardTitle("Every InStat goalie number (latest game in scope)"), el("div")]);
  dataTable(card.lastChild, [{ key: "label", label: "Stat", left: true }, { key: "v", label: "Value", value: (r) => r.value.text }], g.focus_details);
  return card;
}

function goalieTab(g, tab) {
  const grid = (cards, empty) => (cards.some(Boolean) ? el("div", { class: "grid two" }, cards.filter(Boolean)) : emptyNote(empty));
  switch (tab) {
    case "net":
      return grid([goalieNetCard(g), saveSplitCard(g, "body_area", "Save % by where the puck met them", "InStat's body areas (the goalie's own left and right). Red = below their overall save %.")], "No net or body-area breakdown in the games in scope.");
    case "location":
      return grid([goalieZoneCard(g), saveSplitCard(g, "distance", "Save % by shot distance", "Closer shots are harder to stop; compare each band with the overall save %.")], "No shot-location breakdown in the games in scope.");
    case "types":
      return grid([
        saveSplitCard(g, "shot_type", "Save % by shot type", "InStat only types some shots, so these may not add up to every shot."),
        saveSplitCard(g, "situation", "Screened, clear view and 1-on-1", "Screened and clear-view shots cover every shot; 1-on-1 chances are counted separately."),
        saveSplitCard(g, "goalie_state", "Save % by the goalie's state", "InStat's own categories for what the goalie was doing as the shot came."),
      ], "No shot-type breakdown in the games in scope.");
    case "numbers":
      return goalieNumbers(g);
    default:
      return grid([goalieTrendCard(g), reboundCard(g)], "");
  }
}

function viewGoalies() {
  const a = state.analysis;
  if (!a.goalies.length) return page("Goalies", "No goalie pages in scope (they come from the Player report).");
  const [tabs, current] = pageTabs("goalieTab", GOALIE_TABS);
  const r = (g) => g.rebounds;
  const controlled = (g) => {
    const x = r(g);
    const saves = x ? x.uncontrolled + x.controlled + x.frozen_after_rebound + x.frozen_immediately : 0;
    return saves ? (100 * (saves - x.uncontrolled)) / saves : null;
  };
  return page("Goalies", "Save percentage with 95% ranges; small numbers of shots give wide ranges.", tabs, ...a.goalies.map((g) => el("div", { class: "section" }, [
    el("h2", { text: g.player.name }),
    tiles([
      { label: "Save %", value: g.save_pct ? pct(g.save_pct.value, 1) : "—", note: g.save_pct ? `95%: ${fmt(g.save_pct.low, 1)}–${fmt(g.save_pct.high, 1)}` : "" },
      { label: "Even strength", value: g.even_strength_save_pct ? pct(g.even_strength_save_pct.value, 1) : "—" },
      { label: "Short-handed", value: g.short_handed_save_pct ? pct(g.short_handed_save_pct.value, 1) : "—" },
      { label: "Goals against avg", value: fmt(g.goals_against_average, 2), note: `${g.goals_against} GA in ${minutes(g.toi)} min` },
      { label: "Shots faced", value: String(g.shots_against), note: `${g.games} GP` },
      { label: "Rebounds controlled", value: pct(controlled(g), 0), note: "saves without a loose rebound" },
    ]),
    goalieTab(g, current),
  ])));
}

// ---------- Team ----------

function periodTable(c) {
  dataTable(c, [
    { key: "period", label: "Period", left: true, format: (v) => (v <= 3 ? `${v}` : "OT") },
    { key: "shots_on_goal_for", label: "SF" }, { key: "shots_on_goal_against", label: "SA" },
    { key: "attempts_for", label: "Attempts for" }, { key: "attempts_against", label: "Attempts against" },
    { key: "goals", label: "Goals", value: (p) => `${p.goals_for}–${p.goals_against}` },
    { key: "possession_pct", label: "Poss%", format: (v) => pct(v, 0) },
  ], state.analysis.team.periods);
}

function viewTeam() {
  const a = state.analysis;
  const t = a.team;
  const periods = t.periods.map((p) => ({ label: p.period <= 3 ? `Period ${p.period}` : "OT", values: [p.shots_on_goal_for, p.shots_on_goal_against] }));
  const goalsByPeriod = t.periods.map((p) => ({ label: p.period <= 3 ? `Period ${p.period}` : "OT", values: [p.goals_for, p.goals_against] }));
  const series = [{ name: "Us", color: css("--series-1") }, { name: "Them", color: css("--series-2") }];
  const grid = el("div", { class: "grid two" }, [
    chartCard("Shots on goal by period", "Watch for fades late in games. The table also lists all shot attempts.", (c) => groupedColumns(c, periods, series), periodTable),
    chartCard("Goals by period", "", (c) => groupedColumns(c, goalsByPeriod, series), null),
    chartCard("Faceoffs won by zone", "", (c) => hBarChart(c, t.faceoffs.map((z) => ({ label: z.zone, value: z.pct, note: `${z.won} won, ${z.lost} lost` })), { min: 0, max: 100, reference: 50, valueFormat: (v) => pct(v, 0) }), (c) => dataTable(c, [{ key: "zone", label: "Zone", left: true }, { key: "won", label: "Won" }, { key: "lost", label: "Lost" }, { key: "pct", label: "Win %", format: (v) => pct(v, 0) }], t.faceoffs)),
    chartCard("Time by score", "How long we spent leading, tied and trailing.", (c) => hBarChart(c, t.score_states.map((s) => ({ label: s.state, value: s.time / 60 })), { valueFormat: (v) => `${fmt(v, 0)} min` }), null),
    chartCard("Goals by strength", "", (c) => groupedColumns(c, t.strength_goals.map((s) => ({ label: { Even: "Even", PowerPlay: "Power play", ShortHanded: "Short-handed" }[s.strength], values: [s.goals_for, s.goals_against] })), series), null),
    t.cumulative.length >= 2 ? chartCard("Standings points over the season", "2 per win, 1 per overtime loss.", (c) => lineChart(c, [{ name: "Points", color: css("--series-1"), points: t.cumulative.map((p, i) => ({ x: i, y: p.points, label: p.date })) }], { xFormat: (i) => t.cumulative[i]?.date.slice(5) || "", yFormat: (v) => fmt(v, 0), integer: true }), null) : null,
  ].filter(Boolean));
  const logBody = el("div");
  const logCard = more("Game log", logBody);
  dataTable(logBody, [
    { key: "date", label: "Date", left: true }, { key: "opponent", label: "Opponent", left: true },
    { key: "score", label: "Score", value: (g) => `${g.goals_for}-${g.goals_against}` },
    { key: "outcome", label: "", format: (v) => ({ Win: "W", Loss: "L", OvertimeLoss: "OTL" })[v] },
    { key: "shots_on_goal_for", label: "SF" }, { key: "shots_on_goal_against", label: "SA" },
    { key: "attempts", label: "Attempts", value: (g) => `${g.attempts_for}-${g.attempts_against}` },
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

function shotMapCard(title, description, zones) {
  return chartCard(title, description, (c) => shotMap(c, zones), (c) => dataTable(c, [
    { key: "zone", label: "Zone", left: true, format: shotZoneName },
    { key: "shots", label: "Shots" },
    { key: "on_goal", label: "On goal" },
    { key: "pct", label: "On goal %", value: (z) => (z.shots ? (100 * z.on_goal) / z.shots : null), format: (v) => pct(v, 0) },
  ], zones));
}

/** Green when the comparison favours us, red when it favours them, grey near even. */
function comparisonColor(item) {
  if (item.share === null) return css("--deemphasis");
  const edge = item.better === "Higher" ? item.share - 50 : 50 - item.share;
  if (Math.abs(edge) < 3) return css("--deemphasis");
  return edge > 0 ? css("--good") : css("--critical");
}

function comparisonCard(group) {
  const value = (item, v) => (item.measure === "Seconds" ? clock(v) : fmt(v, 0));
  const notes = group.items.filter((item) => item.note).map((item) => `${item.label.replace(/ \(.*\)$/, "")}: ${item.note}`);
  const draw = (c) => {
    hBarChart(c, group.items.map((item) => ({
      label: item.label,
      value: item.share,
      color: comparisonColor(item),
      note: `us ${value(item, item.ours)}, them ${value(item, item.theirs)}${item.note ? ` · ${item.note}` : ""}`,
    })), { min: 0, max: 100, reference: 50, referenceLabel: "even", valueFormat: (v) => pct(v, 0), labelWidth: 230, valueName: "our share" });
    if (notes.length) c.append(el("ul", { class: "small muted notes" }, notes.map((n) => el("li", { text: n }))));
  };
  return chartCard(group.title, "Our share of the total, us vs them. Green = in our favour, red = in theirs (for icings, offsides and giveaways, fewer is better).", draw, (c) => dataTable(c, [
    { key: "label", label: "", left: true },
    { key: "ours", label: "Us", value: (item) => value(item, item.ours) },
    { key: "theirs", label: "Them", value: (item) => value(item, item.theirs) },
    { key: "share", label: "Our share", format: (v) => pct(v, 0) },
    { key: "note", label: "", left: true, format: (v) => v || "" },
  ], group.items));
}

const BATTLE_COLUMNS = [
  ["OwnSlot", "Our slot"], ["BehindOwnGoal", "Behind our net"], ["OwnCorners", "Our corners"], ["OwnBlueLine", "Our blue line"],
  ["NeutralZone", "Neutral"],
  ["OppBlueLine", "Their blue line"], ["OppCorners", "Their corners"], ["BehindOppGoal", "Behind their net"], ["OppSlot", "Their slot"],
];

function battleMapCard(title, description, areas) {
  return chartCard(title, description, (c) => battleMap(c, areas), (c) => dataTable(c, [
    { key: "area", label: "Area", left: true, format: battleAreaName },
    { key: "won", label: "Won" },
    { key: "battles", label: "Battles" },
    { key: "pct", label: "Won %", value: (x) => (x.battles ? (100 * x.won) / x.battles : null), format: (v) => pct(v, 0), tone: "higher" },
  ], areas));
}

function battlesByPlayer(a) {
  const body = el("div");
  const skaters = a.players.filter((p) => p.player.position !== "Goalie" && p.totals.battle_areas.some((x) => x.battles > 0));
  const area = (p, id) => p.totals.battle_areas.find((x) => x.area === id);
  dataTable(body, [
    { key: "name", label: "Player", left: true, value: (p) => p.player.name },
    { key: "total", label: "Battles", value: (p) => p.totals.battle_areas.reduce((sum, x) => sum + x.battles, 0) },
    ...BATTLE_COLUMNS.map(([id, label]) => ({
      key: id,
      label,
      tone: "higher",
      value: (p) => { const x = area(p, id); return x && x.battles ? (100 * x.won) / x.battles : null; },
      render: (p) => { const x = area(p, id); return x && x.battles ? `${x.won}/${x.battles}` : "—"; },
      title: "won / battles; shaded by win % vs teammates",
    })),
  ], skaters, { sortKey: "total", onRow: (p) => goToPlayer(p.player.id) });
  return more("Puck battles by player and area (won / total)", body);
}

function entriesByPlayer(a) {
  const body = el("div");
  const skaters = a.players.filter((p) => p.totals.entries + p.totals.puck_losses + p.totals.puck_recoveries > 0);
  dataTable(body, [
    { key: "name", label: "Player", left: true, value: (p) => p.player.name },
    { key: "entries", label: "Entries", value: (p) => p.totals.entries },
    { key: "carry", label: "Carried", value: (p) => p.totals.entry_types.carry },
    { key: "pass", label: "Passed", value: (p) => p.totals.entry_types.pass },
    { key: "dump", label: "Dumped", value: (p) => p.totals.entry_types.dump_in },
    { key: "carry_pct", label: "Carry-in %", value: (p) => sharePct(p.totals.entry_types.carry, p.totals.entries), format: (v) => pct(v, 0), tone: "higher" },
    { key: "losses", label: "Puck losses", value: (p) => p.totals.puck_losses },
    { key: "own", label: "In our zone", value: (p) => p.totals.puck_losses_defensive_zone, tone: "lower" },
    { key: "recoveries", label: "Recoveries", value: (p) => p.totals.puck_recoveries },
    { key: "theirs", label: "In their zone", value: (p) => p.totals.puck_recoveries_offensive_zone, tone: "higher" },
  ], skaters, { sortKey: "entries", onRow: (p) => goToPlayer(p.player.id) });
  return more("Zone entries and turnovers by player", body);
}

function viewPlay() {
  const a = state.analysis;
  const t = a.team;
  const note = el("p", { class: "small muted", text: "InStat's PDFs give totals per game, not the play-by-play feed, so sequences between whistles (e.g. every neutral-zone regroup) can't be rebuilt. Positional attacks vs counter-attacks and entry types are the closest categories InStat provides." });
  return page("Possession & shots", "How we attack, enter the zone and manage the puck, compared with our opponents.",
    el("div", { class: "grid two" }, [
      t.charted_shots.length ? chartCard("Every shot we took", "Where InStat's shooting charts drew each shot, over the games in scope. Hover a dot for the shooter.", (c) => shotPlot(c, t.charted_shots, { tip: (x) => shotTip(x, { game: a.games.filter((g) => g.in_scope).length > 1 }) }), (c) => shootersTable(c, t.charted_shots)) : null,
      t.shot_zones.some((z) => z.shots > 0) ? shotMapCard("Our shots", "Where our shots came from (all skaters, all strengths). Hover a zone for details.", t.shot_zones) : null,
      t.shot_zones_against.some((z) => z.shots > 0) ? shotMapCard("Shots against", "Where opponents shot on our net, from their shots table. Same layout: our net at the top.", t.shot_zones_against) : null,
    ].filter(Boolean)),
    t.battle_areas.some((x) => x.battles > 0) ? el("div", { style: "margin-top:16px" }, [battleMapCard("Puck battles by area", "Where on the ice we win and lose one-on-one puck battles. Blue = winning most, red = losing most; hover an area for the counts.", t.battle_areas)]) : null,
    t.battle_areas.some((x) => x.battles > 0) ? battlesByPlayer(a) : null,
    entriesByPlayer(a),
    el("div", { class: "grid two", style: "margin-top:16px" }, a.style.groups.filter((g) => g.items.length).map(comparisonCard)),
    note);
}

const FOCUS_TEXT = { WorkOn: ["bad", "▼", "Work on"], Watch: ["info", "●", "Keep an eye on"], Strength: ["good", "▲", "Strengths"] };

function focusList(items) {
  return el("ul", { class: "takeaways focus" }, items.map((item) => {
    const [tone, icon] = FOCUS_TEXT[item.verdict];
    return el("li", { class: `tone-${tone}` }, [
      el("span", { class: "icon", text: icon, "aria-hidden": "true" }),
      el("div", {}, [
        el("div", { class: "who", text: item.area }),
        el("div", { text: item.evidence }),
        el("div", { class: "small muted", text: `Practice idea: ${item.suggestion}` }),
      ]),
    ]);
  }));
}

function viewFocus() {
  const a = state.analysis;
  const focus = a.style.focus;
  if (!focus.length) return page("Practice focus", "Not enough data in scope yet.");
  const groups = ["WorkOn", "Watch", "Strength"].map((verdict) => [verdict, focus.filter((f) => f.verdict === verdict)]).filter(([, items]) => items.length);
  const inScope = a.games.filter((g) => g.in_scope).length;
  return page("Practice focus", "What the numbers say to work on, ranked from the biggest gap. Each area compares us with our opponents (50% = even; power play vs 20%, penalty kill vs 80%).",
    ...groups.map(([verdict, items]) => el("div", { class: "card", style: "margin-bottom:16px" }, [cardTitle(FOCUS_TEXT[verdict][2]), focusList(items)])),
    el("p", { class: "small muted", text: `Based on ${inScope} game${inScope === 1 ? "" : "s"}. An area needs at least 10 events before it's called a problem or a strength, and 8 points away from its benchmark; with few games, treat these as things to check on video, not conclusions.` }));
}

function practiceSummary(a) {
  const top = a.style.focus.filter((f) => f.verdict === "WorkOn").slice(0, 3);
  if (!top.length) return null;
  return el("div", { class: "card", style: "margin-top:16px" }, [
    el("div", { class: "card-head" }, [
      cardTitle("Practice focus"),
      el("button", { class: "link small", text: "all areas", onclick: () => { state.sub.team = "focus"; setView("team"); } }),
    ]),
    focusList(top),
  ]);
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
      { key: "net_per_60", label: "Net /60", format: (v) => signed(v, per), tone: "higher" },
    ], m.rows, { sortKey: "net_per_60" }));
  });
  return page("Individual impact", "Plus/minus-style numbers are unfair to players stuck on weak lines. These models credit each player only for the difference they make once their linemates are accounted for.", el("div", { class: "grid two" }, cards));
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
    const counts = ["LikelyReal", "Maybe", "CouldBeNoise", "NotEnoughData"].map((v) => [v, rows.filter((t) => t.verdict === v).length]).filter(([, n]) => n);
    const card = el("details", { class: "more family" }, [
      el("summary", {}, [el("span", { class: "family-name", text: heading }), ...counts.map(([v, n]) => el("span", { class: `badge verdict-${v}` }, [el("span", { class: "dot" }), `${n} ${VERDICT_TEXT[v].toLowerCase()}`]))]),
      el("div"),
    ]);
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
  const powerCard = el("details", { class: "more family" }, [el("summary", {}, [el("span", { class: "family-name", text: "How many games until we know? (power analysis)" })]), el("p", { class: "desc", text: "Simulates future games at each unit's current usage, assuming its adjusted shot share is its true level, and finds how many games give an 80% chance of a significant result." }), el("div")]);
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
    ["Shot locations", "Every shot from InStat's shooting chart, placed on a standard rink using the chart's own faceoff circles. Distances are measured to the middle of the net; InStat's drawing is approximate, so treat them as a few feet either way."],
    ["Shot map", "Where shots came from, using InStat's seven zones: slot, high slot, left and right sides, and three spots along the blue line. Each zone shows shots / on goal; darker = more shots."],
    ["Puck battles by area", "InStat splits every one-on-one puck battle by where it happened: in front of each net, behind each net, the corners, along each blue line, and the neutral zone. The map shows the share we won in each area; corners are one area drawn top and bottom."],
    ["Matchups", "InStat's challenge and hits distributions list every one-on-one puck battle and every hit between each of our skaters and each of theirs. Opponents are shown as InStat labels them (number and surname); bars show battles won minus lost, so one battle never looks like a 100% record."],
    ["Goalie breakdowns", "From the goalie's Player-report page: save % by distance, zone, shot type, screened or clear view, where on the net the shot was headed (seen from the shooter), and where the puck met the goalie (the goalie's own left and right). Colours compare each part with the goalie's overall save %. Rebound control splits every save by what happened to the puck next."],
    ["Possession & shots", "InStat's team counts (attacks, zone entries, puck losses, takeaways, icings, offsides) shown as our share of the total against the opponent. Positional attacks are set up in their zone; counter-attacks come off a quick transition."],
    ["Practice focus", "Each area compares us with our opponents (50% = even; power play against 20%, penalty kill against 80%). An area is flagged only when it's 8+ points off and backed by at least 10 events; the drill ideas are starting points, not prescriptions."],
    ["Shift data", "Rebuilt from InStat's time-distribution chart. The reader checks itself: every player's +/- rebuilt from shifts must match InStat's own column, or the game shows a warning."],
    ["Where the numbers come from", "InStat's Match report (team, player, line, shot, challenge and pass tables, the challenge and hits distributions and the shift chart) and Player report (full names, jersey numbers, xG, goalie details and each player's recent-games history). InStat prints wrong jersey numbers in some Match-report tables; the reader uses names and ice time instead."],
  ];
  return page("How to read this", "Short explanations of every number in the report. Anywhere in the app, a stat name with a dotted underline explains itself: hover over it, tap it, or tab to it.", el("dl", { class: "explain" }, terms.flatMap(([t, d]) => [el("dt", { text: t }), el("dd", { text: d })])));
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
  const sections = {
    lines: { title: "Lines & pairs", views: { units: viewLines, chemistry: viewChemistry, passing: viewPassing } },
    team: { title: "Team & goalies", views: { team: viewTeam, play: viewPlay, focus: viewFocus, goalies: viewGoalies } },
    deep: { title: "Deep dive", views: { impact: viewImpact, profiles: viewProfiles, advanced: viewAdvanced } },
  };
  const views = { games: viewGames, summary: viewSummary, rankings: viewRankings, players: viewPlayers, game: viewGame, help: viewHelp };
  if (a.games.length === 0 && !["games", "help"].includes(state.view)) state.view = "games";
  let content;
  if (!snapshot && (!state.team || state.changingTeam)) {
    content = viewSetup();
  } else if (sections[state.view]) {
    const section = sections[state.view];
    const sub = state.sub[state.view] || SUBVIEWS[state.view][0][0];
    insideTabs = true;
    try {
      content = [pageTitle(section.title), subTabs(state.view), ...[].concat(section.views[sub]())];
    } finally {
      insideTabs = false;
    }
  } else {
    content = (views[state.view] || viewSummary)();
  }
  main.replaceChildren(...[updateBanner()].concat(content).filter(Boolean));
}

let resizeTimer = null;
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(render, 200);
});
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", render);

const [initialView, initialPlayer, initialTab] = decodeURIComponent(location.hash.slice(1)).split(":");
if (VIEWS.some((v) => v.id === initialView)) state.view = initialView;
if (initialView === "players" && initialPlayer) state.player = initialPlayer;
if (SUBVIEWS[initialView] && SUBVIEWS[initialView].some(([id]) => id === initialPlayer)) state.sub[initialView] = initialPlayer;
if (initialView === "lines" && UNIT_TABS.some(([id]) => id === initialTab)) state.unitTab = initialTab;

if (!snapshot) {
  // Frequent enough that another open tab keeps the server alive when one tab closes.
  setInterval(() => { api("/api/heartbeat", { method: "POST" }).catch((e) => { if (e instanceof ServerGone) showClosed(); }); }, 5000);
  // A reload also fires this; the server waits a few seconds and stays up if the page returns.
  window.addEventListener("pagehide", () => {
    fetch("/api/closing", { method: "POST", keepalive: true, headers: { "X-Hockey-Token": state.token } }).catch(() => {});
  });
  window.addEventListener("pageshow", (event) => { if (event.persisted) refresh(); });
  watchForUpdate();
}
refresh();
