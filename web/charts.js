(() => {
"use strict";

// Small SVG chart kit. Every label is inserted with textContent (names come from PDFs).

const SVG_NS = "http://www.w3.org/2000/svg";

function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else node.setAttribute(key, value === true ? "" : value);
  }
  for (const child of [].concat(children)) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function svg(tag, attrs = {}, children = []) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null) continue;
    if (key === "text") node.textContent = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else node.setAttribute(key, value);
  }
  for (const child of [].concat(children)) {
    if (child) node.append(child);
  }
  return node;
}

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const SERIES = ["--series-1", "--series-2", "--series-3"];

function fmt(value, digits = 1, suffix = "") {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  return `${Number(value).toFixed(digits)}${suffix}`;
}
const pct = (v, d = 1) => fmt(v, d, "%");
const signed = (v, d = 1) => (v === null || v === undefined || Number.isNaN(v) ? "—" : `${v > 0 ? "+" : ""}${Number(v).toFixed(d)}`);
function clock(seconds) {
  if (seconds === null || seconds === undefined) return "—";
  const s = Math.round(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
const minutes = (seconds) => fmt(seconds / 60, 1);
function gameClock(t) {
  const period = Math.min(Math.floor(t / 1200), 3) + 1;
  const within = t - (period - 1) * 1200;
  return `${period <= 3 ? `P${period}` : "OT"} ${clock(within)}`;
}

function niceTicks(min, max, count = 5) {
  if (min === max) { min -= 1; max += 1; }
  const span = max - min;
  const step0 = span / count;
  const magnitude = 10 ** Math.floor(Math.log10(step0));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * magnitude).find((s) => span / s <= count) || magnitude * 10;
  const start = Math.floor(min / step) * step;
  const ticks = [];
  for (let v = start; v <= max + step * 1e-9; v += step) ticks.push(Number(v.toFixed(10)));
  if (ticks[0] > min) ticks.unshift(Number((ticks[0] - step).toFixed(10)));
  if (ticks[ticks.length - 1] < max) ticks.push(Number((ticks[ticks.length - 1] + step).toFixed(10)));
  return ticks;
}

// Tooltip: value leads, label follows; never the only way to a number (tables exist).
const Tooltip = {
  node: null,
  show(event, title, rows) {
    if (!this.node) this.node = document.getElementById("tooltip");
    const node = this.node;
    node.replaceChildren();
    if (title) node.append(el("div", { class: "tt-title", text: title }));
    for (const row of rows) {
      const line = el("div", { class: "tt-row" });
      if (row.color) line.append(el("span", { class: "tt-key", style: `background:${row.color}` }));
      line.append(el("span", { class: "tt-value", text: row.value }));
      if (row.name) line.append(el("span", { class: "tt-name", text: row.name }));
      node.append(line);
    }
    node.style.display = "block";
    this.move(event);
  },
  move(event) {
    if (!this.node || this.node.style.display === "none") return;
    const pad = 14;
    const rect = this.node.getBoundingClientRect();
    let x = event.clientX + pad;
    let y = event.clientY + pad;
    if (x + rect.width > window.innerWidth - 8) x = event.clientX - rect.width - pad;
    if (y + rect.height > window.innerHeight - 8) y = event.clientY - rect.height - pad;
    this.node.style.left = `${x}px`;
    this.node.style.top = `${y}px`;
  },
  hide() {
    if (this.node) this.node.style.display = "none";
  },
};

// Stat definitions, registered by the app; labels with a definition get a dotted underline.
let glossary = {};

function setGlossary(map) {
  glossary = map;
}

function definition(label) {
  if (label === null || label === undefined) return null;
  const key = String(label).trim();
  return glossary[key] || glossary[key.replace(/\s*\(.*\)$/, "")] || glossary[key.replace(/\s+\d+$/, "")] || null;
}

let pinnedTerm = null;

function explain(node, label) {
  const text = definition(label);
  if (!text) return;
  const show = (event) => {
    Tooltip.show(event, label, []);
    Tooltip.node.append(el("div", { class: "tt-explain", text }));
    Tooltip.move(event);
  };
  node.addEventListener("pointerenter", (e) => { if (!pinnedTerm) show(e); });
  node.addEventListener("pointermove", (e) => { if (!pinnedTerm) Tooltip.move(e); });
  node.addEventListener("pointerleave", () => { if (!pinnedTerm) Tooltip.hide(); });
  node.addEventListener("click", (e) => {
    e.stopPropagation();
    if (pinnedTerm === node) { pinnedTerm = null; Tooltip.hide(); return; }
    pinnedTerm = node;
    show(e);
  });
  node.addEventListener("focus", () => {
    const r = node.getBoundingClientRect();
    show({ clientX: r.left, clientY: r.bottom });
  });
  node.addEventListener("blur", () => { if (pinnedTerm === node) pinnedTerm = null; Tooltip.hide(); });
  node.setAttribute("tabindex", "0");
}

document.addEventListener("click", () => {
  if (pinnedTerm) { pinnedTerm = null; Tooltip.hide(); }
});

/** A label that explains itself on hover, tap or keyboard focus. */
function term(label, className = "") {
  if (!definition(label)) return document.createTextNode(String(label));
  const span = el("span", { class: `term ${className}`, text: label });
  explain(span, label);
  return span;
}

function attachTooltip(node, title, rows) {
  const show = (e) => Tooltip.show(e, title, typeof rows === "function" ? rows() : rows);
  node.addEventListener("pointerenter", show);
  node.addEventListener("pointermove", (e) => Tooltip.move(e));
  node.addEventListener("pointerleave", () => Tooltip.hide());
  node.setAttribute("tabindex", "0");
  node.addEventListener("focus", (e) => {
    const r = node.getBoundingClientRect();
    Tooltip.show({ clientX: r.right, clientY: r.top }, title, typeof rows === "function" ? rows() : rows);
    void e;
  });
  node.addEventListener("blur", () => Tooltip.hide());
}

function measureWidth(container, fallback = 640) {
  return Math.max(280, Math.floor(container.clientWidth || fallback));
}

// A roundedEnd bar path: square at the baseline, 4px radius at the data end.
function barPath(x0, x1, y, h, radius = 4) {
  const left = Math.min(x0, x1);
  const right = Math.max(x0, x1);
  const r = Math.min(radius, (right - left) / 2, h / 2);
  if (x1 >= x0) {
    return `M${left},${y}H${right - r}Q${right},${y} ${right},${y + r}V${y + h - r}Q${right},${y + h} ${right - r},${y + h}H${left}Z`;
  }
  return `M${right},${y}H${left + r}Q${left},${y} ${left},${y + r}V${y + h - r}Q${left},${y + h} ${left + r},${y + h}H${right}Z`;
}

function columnPath(x, y0, y1, w, radius = 4) {
  const top = Math.min(y0, y1);
  const bottom = Math.max(y0, y1);
  const r = Math.min(radius, (bottom - top) / 2, w / 2);
  if (y1 <= y0) {
    return `M${x},${bottom}V${top + r}Q${x},${top} ${x + r},${top}H${x + w - r}Q${x + w},${top} ${x + w},${top + r}V${bottom}Z`;
  }
  return `M${x},${top}V${bottom - r}Q${x},${bottom} ${x + r},${bottom}H${x + w - r}Q${x + w},${bottom} ${x + w},${bottom - r}V${top}Z`;
}

/**
 * Horizontal bars, optionally diverging around zero and with interval whiskers.
 * rows: [{label, value, low?, high?, color?, note?, emphasis?}]
 */
function hBarChart(container, rows, options = {}) {
  const width = measureWidth(container);
  const labelWidth = options.labelWidth ?? Math.min(200, Math.max(90, width * 0.28));
  const rowHeight = options.rowHeight ?? 26;
  const barHeight = Math.min(18, rowHeight - 8);
  const margin = { top: 8, right: 56, bottom: 24, left: labelWidth };
  const height = margin.top + margin.bottom + rows.length * rowHeight;
  const values = rows.flatMap((r) => [r.value, r.low, r.high]).filter((v) => v !== null && v !== undefined && !Number.isNaN(v));
  let min = options.min ?? Math.min(0, ...values);
  let max = options.max ?? Math.max(0, ...values);
  if (options.reference !== undefined) { min = Math.min(min, options.reference); max = Math.max(max, options.reference); }
  if (min === max) max = min + 1;
  // Leave room left of the lowest bar for its value label.
  if (options.min === undefined && options.showValues !== false && !options.dots && min < (options.baseline ?? 0)) {
    min -= (max - min) * 0.18;
  }
  const ticks = niceTicks(min, max, Math.max(3, Math.floor((width - labelWidth) / 90))).filter((t) => !options.integer || Number.isInteger(t));
  min = Math.min(min, ticks[0]);
  max = Math.max(max, ticks[ticks.length - 1]);
  const x = (v) => margin.left + ((v - min) / (max - min)) * (width - margin.left - margin.right);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": options.title || "bar chart" });
  for (const t of ticks) {
    root.append(svg("line", { class: "grid-line", x1: x(t), x2: x(t), y1: margin.top, y2: height - margin.bottom }));
    root.append(svg("text", { x: x(t), y: height - 6, "text-anchor": "middle", class: "axis-label", text: options.tickFormat ? options.tickFormat(t) : String(t) }));
  }
  const zero = x(options.baseline ?? 0);
  root.append(svg("line", { class: "baseline", x1: zero, x2: zero, y1: margin.top, y2: height - margin.bottom }));
  if (options.reference !== undefined) {
    const rx = x(options.reference);
    root.append(svg("line", { x1: rx, x2: rx, y1: margin.top - 4, y2: height - margin.bottom, stroke: css("--text-muted"), "stroke-width": 1 }));
    if (options.referenceLabel) root.append(svg("text", { x: rx + 4, y: margin.top + 2, class: "axis-label", text: options.referenceLabel }));
  }
  rows.forEach((row, i) => {
    const y = margin.top + i * rowHeight + (rowHeight - barHeight) / 2;
    const label = svg("text", { x: margin.left - 8, y: y + barHeight / 2 + 4, "text-anchor": "end", text: row.label });
    if (row.emphasis) label.setAttribute("class", "value-label");
    if (definition(row.label)) {
      label.setAttribute("class", "term-svg");
      explain(label, row.label);
    }
    root.append(label);
    const g = svg("g", { class: "mark" });
    const color = row.color || css(options.color || "--series-1");
    if (row.value !== null && row.value !== undefined && !Number.isNaN(row.value)) {
      if (options.dots) {
        g.append(svg("circle", { cx: x(row.value), cy: y + barHeight / 2, r: 5, fill: color, stroke: css("--surface-1"), "stroke-width": 2 }));
      } else {
        g.append(svg("path", { d: barPath(zero, x(row.value), y, barHeight), fill: color }));
      }
      if (row.low !== undefined && row.low !== null && row.high !== undefined && row.high !== null) {
        const cy = y + barHeight / 2;
        const ink = css("--text-secondary");
        g.append(svg("line", { x1: x(row.low), x2: x(row.high), y1: cy, y2: cy, stroke: ink, "stroke-width": 1.5 }));
        g.append(svg("line", { x1: x(row.low), x2: x(row.low), y1: cy - 4, y2: cy + 4, stroke: ink, "stroke-width": 1.5 }));
        g.append(svg("line", { x1: x(row.high), x2: x(row.high), y1: cy - 4, y2: cy + 4, stroke: ink, "stroke-width": 1.5 }));
      }
      const end = Math.max(x(row.value), row.high !== undefined && row.high !== null ? x(row.high) : 0);
      const valueText = options.valueFormat ? options.valueFormat(row.value, row) : fmt(row.value);
      if (options.showValues !== false) {
        const negative = row.value < (options.baseline ?? 0) && !options.dots;
        g.append(svg("text", {
          x: negative ? Math.min(x(row.value), row.low !== undefined ? x(row.low) : x(row.value)) - 6 : end + 6,
          y: y + barHeight / 2 + 4,
          "text-anchor": negative ? "end" : "start",
          class: "value-label",
          text: valueText,
        }));
      }
    }
    const hit = svg("rect", { class: "hit", x: 0, y: margin.top + i * rowHeight, width, height: rowHeight });
    g.append(hit);
    attachTooltip(g, row.label, () => [
      { value: options.valueFormat ? options.valueFormat(row.value, row) : fmt(row.value), name: options.valueName || "", color },
      ...(row.low !== undefined && row.low !== null ? [{ value: `${fmt(row.low)} – ${fmt(row.high)}`, name: options.intervalName || "interval" }] : []),
      ...(row.note ? [{ value: row.note, name: "" }] : []),
    ]);
    root.append(g);
  });
  container.replaceChildren(root);
  return root;
}

/**
 * Lines over an ordered x (dates or indices).
 * series: [{name, color, points: [{x, y, label?, hollow?}]}]
 */
function lineChart(container, series, options = {}) {
  const width = measureWidth(container);
  const height = options.height ?? 240;
  const margin = { top: 12, right: 70, bottom: 30, left: 44 };
  const all = series.flatMap((s) => s.points).filter((p) => p.y !== null && p.y !== undefined);
  if (!all.length) {
    container.replaceChildren(el("div", { class: "empty small", text: "No data yet." }));
    return null;
  }
  const xs = [...new Set(all.map((p) => p.x))].sort((a, b) => a - b);
  const xMin = xs[0];
  const xMax = xs[xs.length - 1] === xMin ? xMin + 1 : xs[xs.length - 1];
  let yMin = options.yMin ?? Math.min(...all.map((p) => p.y));
  let yMax = options.yMax ?? Math.max(...all.map((p) => p.y));
  if (options.reference !== undefined) { yMin = Math.min(yMin, options.reference); yMax = Math.max(yMax, options.reference); }
  if (options.integer) { yMin = Math.floor(yMin); yMax = Math.max(Math.ceil(yMax), yMin + 1); }
  let ticks = niceTicks(yMin, yMax, 4);
  if (options.integer) ticks = ticks.filter((t) => Number.isInteger(t));
  yMin = Math.min(yMin, ticks[0]);
  yMax = Math.max(yMax, ticks[ticks.length - 1]);
  const x = (v) => margin.left + ((v - xMin) / (xMax - xMin)) * (width - margin.left - margin.right);
  const y = (v) => height - margin.bottom - ((v - yMin) / (yMax - yMin)) * (height - margin.top - margin.bottom);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": options.title || "line chart" });
  for (const t of ticks) {
    root.append(svg("line", { class: "grid-line", x1: margin.left, x2: width - margin.right, y1: y(t), y2: y(t) }));
    root.append(svg("text", { x: margin.left - 6, y: y(t) + 4, "text-anchor": "end", class: "axis-label", text: options.yFormat ? options.yFormat(t) : String(t) }));
  }
  if (options.reference !== undefined) {
    root.append(svg("line", { x1: margin.left, x2: width - margin.right, y1: y(options.reference), y2: y(options.reference), stroke: css("--axis"), "stroke-width": 1 }));
  }
  const labelEvery = Math.max(1, Math.ceil(xs.length / Math.max(2, Math.floor((width - 120) / 70))));
  xs.forEach((v, i) => {
    if (i % labelEvery !== 0 && i !== xs.length - 1) return;
    root.append(svg("text", { x: x(v), y: height - 10, "text-anchor": "middle", class: "axis-label", text: options.xFormat ? options.xFormat(v) : String(v) }));
  });
  for (const s of series) {
    const pts = s.points.filter((p) => p.y !== null && p.y !== undefined);
    if (!pts.length) continue;
    const color = s.color || css("--series-1");
    if (pts.length > 1) {
      root.append(svg("path", {
        d: pts.map((p, i) => `${i ? "L" : "M"}${x(p.x)},${y(p.y)}`).join(""),
        fill: "none", stroke: color, "stroke-width": 2, "stroke-linejoin": "round", "stroke-linecap": "round",
      }));
    }
    for (const p of pts) {
      root.append(svg("circle", {
        cx: x(p.x), cy: y(p.y), r: 4, fill: p.hollow ? css("--surface-1") : color,
        stroke: p.hollow ? color : css("--surface-1"), "stroke-width": 2,
      }));
    }
    const last = pts[pts.length - 1];
    if (series.length <= 4 && options.endLabels !== false) {
      root.append(svg("text", { x: x(last.x) + 8, y: y(last.y) + 4, class: "value-label", text: options.yFormat ? options.yFormat(last.y) : fmt(last.y) }));
    }
  }
  const cross = svg("line", { y1: margin.top, y2: height - margin.bottom, stroke: css("--text-muted"), "stroke-width": 1, visibility: "hidden" });
  root.append(cross);
  const overlay = svg("rect", { x: margin.left, y: margin.top, width: width - margin.left - margin.right, height: height - margin.top - margin.bottom, fill: "transparent" });
  overlay.addEventListener("pointermove", (event) => {
    const rect = root.getBoundingClientRect();
    const px = ((event.clientX - rect.left) / rect.width) * width;
    let nearest = xs[0];
    for (const v of xs) if (Math.abs(x(v) - px) < Math.abs(x(nearest) - px)) nearest = v;
    cross.setAttribute("x1", x(nearest));
    cross.setAttribute("x2", x(nearest));
    cross.setAttribute("visibility", "visible");
    const rows = series.flatMap((s) => {
      const p = s.points.find((q) => q.x === nearest && q.y !== null && q.y !== undefined);
      return p ? [{ value: options.yFormat ? options.yFormat(p.y) : fmt(p.y, 1), name: s.name + (p.label ? ` · ${p.label}` : ""), color: s.color || css("--series-1") }] : [];
    });
    Tooltip.show(event, options.xFormat ? options.xFormat(nearest) : String(nearest), rows);
  });
  overlay.addEventListener("pointerleave", () => {
    cross.setAttribute("visibility", "hidden");
    Tooltip.hide();
  });
  root.append(overlay);
  container.replaceChildren(root);
  if (series.length >= 2) {
    container.append(el("div", { class: "legend" }, series.map((s) => el("span", {}, [el("span", { class: "key line", style: `background:${s.color || css("--series-1")}` }), s.name]))));
  }
  return root;
}

/**
 * Vertical grouped columns. groups: [{label, values: [v per series]}], series: [{name, color}]
 */
function groupedColumns(container, groups, series, options = {}) {
  const width = measureWidth(container);
  const height = options.height ?? 220;
  const margin = { top: 16, right: 12, bottom: 30, left: 40 };
  const values = groups.flatMap((g) => g.values).filter((v) => v !== null && v !== undefined);
  const ticks = niceTicks(0, Math.max(1, ...values), 4);
  const yMax = ticks[ticks.length - 1];
  const y = (v) => height - margin.bottom - (v / yMax) * (height - margin.top - margin.bottom);
  const band = (width - margin.left - margin.right) / groups.length;
  const barWidth = Math.min(24, (band * 0.7) / series.length);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": options.title || "column chart" });
  for (const t of ticks) {
    root.append(svg("line", { class: "grid-line", x1: margin.left, x2: width - margin.right, y1: y(t), y2: y(t) }));
    root.append(svg("text", { x: margin.left - 6, y: y(t) + 4, "text-anchor": "end", class: "axis-label", text: String(t) }));
  }
  root.append(svg("line", { class: "baseline", x1: margin.left, x2: width - margin.right, y1: y(0), y2: y(0) }));
  groups.forEach((group, gi) => {
    const center = margin.left + band * gi + band / 2;
    const totalWidth = series.length * barWidth + (series.length - 1) * 2;
    root.append(svg("text", { x: center, y: height - 10, "text-anchor": "middle", class: "axis-label", text: group.label }));
    group.values.forEach((value, si) => {
      if (value === null || value === undefined) return;
      const bx = center - totalWidth / 2 + si * (barWidth + 2);
      const color = series[si].color;
      const g = svg("g", { class: "mark" });
      g.append(svg("path", { d: columnPath(bx, y(0), y(value), barWidth), fill: color }));
      g.append(svg("text", { x: bx + barWidth / 2, y: y(value) - 4, "text-anchor": "middle", class: "value-label", text: options.valueFormat ? options.valueFormat(value) : String(value) }));
      g.append(svg("rect", { class: "hit", x: bx - 2, y: margin.top, width: barWidth + 4, height: height - margin.top - margin.bottom }));
      attachTooltip(g, group.label, [{ value: options.valueFormat ? options.valueFormat(value) : String(value), name: series[si].name, color }]);
      root.append(g);
    });
  });
  container.replaceChildren(root);
  if (series.length >= 2) {
    container.append(el("div", { class: "legend" }, series.map((s) => el("span", {}, [el("span", { class: "key", style: `background:${s.color}` }), s.name]))));
  }
}

function mix(hexA, hexB, t) {
  const parse = (h) => {
    const s = h.replace("#", "");
    return [0, 2, 4].map((i) => parseInt(s.slice(i, i + 2), 16));
  };
  const a = parse(hexA);
  const b = parse(hexB);
  const c = a.map((v, i) => Math.round(v + (b[i] - v) * Math.max(0, Math.min(1, t))));
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function sequentialColor(t) {
  return mix(css("--seq-low"), css("--seq-high"), t);
}

function divergingColor(t) {
  // t in [-1, 1]; gray midpoint.
  return t < 0 ? mix(css("--div-mid"), css("--div-neg"), -t) : mix(css("--div-mid"), css("--div-pos"), t);
}

function inkOn(hex) {
  const s = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(s.slice(i, i + 2), 16) / 255);
  const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return lum > 0.55 ? "#0b0b0b" : "#ffffff";
}

/**
 * Matrix heatmap. labels: names shared by rows and columns, or {rows, columns} when they
 * differ; cell(i, j) -> {value, text?, tip?} or null.
 * scale: {kind: "sequential"|"diverging", min, max, center}
 */
function heatmap(container, labels, cell, scale, options = {}) {
  const rowLabels = Array.isArray(labels) ? labels : labels.rows;
  const columnLabels = Array.isArray(labels) ? labels : labels.columns;
  const width = measureWidth(container);
  const labelWidth = options.labelWidth ?? 130;
  const size = Math.max(14, Math.min(34, (width - labelWidth - 10) / columnLabels.length));
  const top = options.columnLabels === false ? 8 : labelWidth * 0.8;
  const height = top + rowLabels.length * size + 8;
  // Room on the right for the last rotated column label.
  const svgWidth = labelWidth + columnLabels.length * size + (options.columnLabels === false ? 10 : 70);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${svgWidth} ${height}`, width: svgWidth, height, role: "img", "aria-label": options.title || "heatmap" });
  rowLabels.forEach((name, i) => {
    const rowLabel = svg("text", { x: labelWidth - 6, y: top + i * size + size / 2 + 4, "text-anchor": "end", text: name });
    if (definition(name)) {
      rowLabel.setAttribute("class", "term-svg");
      explain(rowLabel, name);
    }
    root.append(rowLabel);
  });
  if (options.columnLabels !== false) {
    columnLabels.forEach((name, j) => {
      const cx = labelWidth + j * size + size / 2;
      root.append(svg("text", { x: 0, y: 0, transform: `translate(${cx + 4},${top - 6}) rotate(-55)`, text: name }));
    });
  }
  for (let i = 0; i < rowLabels.length; i += 1) {
    for (let j = 0; j < columnLabels.length; j += 1) {
      const c = cell(i, j);
      const x = labelWidth + j * size;
      const y = top + i * size;
      if (!c || c.value === null || c.value === undefined) {
        root.append(svg("rect", { x: x + 1, y: y + 1, width: size - 2, height: size - 2, rx: 3, fill: css("--surface-2") }));
        continue;
      }
      let fill;
      if (scale.kind === "diverging") {
        const span = Math.max(Math.abs(scale.max - scale.center), Math.abs(scale.min - scale.center)) || 1;
        fill = divergingColor((c.value - scale.center) / span);
      } else {
        fill = sequentialColor((c.value - scale.min) / ((scale.max - scale.min) || 1));
      }
      const g = svg("g", { class: "mark" });
      g.append(svg("rect", { x: x + 1, y: y + 1, width: size - 2, height: size - 2, rx: 3, fill }));
      if (c.text && size >= 24) {
        g.append(svg("text", { x: x + size / 2, y: y + size / 2 + 4, "text-anchor": "middle", style: `fill:${inkOn(fill)};font-size:10px`, text: c.text }));
      }
      attachTooltip(g, c.title || `${rowLabels[i]} × ${columnLabels[j]}`, c.tip || [{ value: fmt(c.value), name: options.valueName || "" }]);
      if (c.onClick) g.addEventListener("click", c.onClick);
      root.append(g);
    }
  }
  const wrap = el("div", { style: "overflow-x:auto" }, [root]);
  container.replaceChildren(wrap);
  container.append(scaleLegend(scale, options));
}

function scaleLegend(scale, options = {}) {
  const width = 220;
  const root = svg("svg", { width: width + 80, height: 34, class: "chart" });
  const id = `grad${Math.random().toString(36).slice(2)}`;
  const gradient = svg("linearGradient", { id });
  for (let i = 0; i <= 10; i += 1) {
    const t = i / 10;
    const color = scale.kind === "diverging" ? divergingColor(t * 2 - 1) : sequentialColor(t);
    gradient.append(svg("stop", { offset: `${t * 100}%`, "stop-color": color }));
  }
  root.append(svg("defs", {}, [gradient]));
  root.append(svg("rect", { x: 40, y: 4, width, height: 10, rx: 3, fill: `url(#${id})` }));
  const f = options.scaleFormat || ((v) => fmt(v, 0));
  const lo = scale.kind === "diverging" ? scale.center - Math.max(Math.abs(scale.max - scale.center), Math.abs(scale.min - scale.center)) : scale.min;
  const hi = scale.kind === "diverging" ? scale.center + Math.max(Math.abs(scale.max - scale.center), Math.abs(scale.min - scale.center)) : scale.max;
  root.append(svg("text", { x: 40, y: 28, class: "axis-label", text: f(lo) }));
  root.append(svg("text", { x: 40 + width, y: 28, "text-anchor": "end", class: "axis-label", text: f(hi) }));
  if (scale.kind === "diverging") root.append(svg("text", { x: 40 + width / 2, y: 28, "text-anchor": "middle", class: "axis-label", text: f(scale.center) }));
  if (options.scaleName) root.append(svg("text", { x: 0, y: 13, class: "axis-label", text: options.scaleName }));
  return el("div", {}, [root]);
}

/**
 * Shift chart: one row per player, bars per shift, PP/SH bands and goal lines.
 */
function shiftChart(container, timeline, options = {}) {
  const width = Math.max(measureWidth(container), 720);
  const labelWidth = 150;
  const rowHeight = 18;
  const top = 28;
  const players = timeline.players;
  const height = top + players.length * rowHeight + 30;
  const length = timeline.length;
  const x = (t) => labelWidth + (t / length) * (width - labelWidth - 12);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": "shift chart" });
  for (const [start, end, team] of timeline.advantages) {
    const band = svg("rect", { x: x(start), y: top - 4, width: Math.max(1, x(end) - x(start)), height: players.length * rowHeight + 8, fill: team === "Us" ? css("--accent-wash") : "color-mix(in srgb, var(--div-neg) 12%, transparent)" });
    attachTooltip(band, team === "Us" ? "Our power play" : "We were short-handed", [{ value: `${gameClock(start)} – ${gameClock(end)}`, name: "" }]);
    root.append(band);
  }
  for (let p = 1; p * 1200 < length; p += 1) {
    root.append(svg("line", { class: "baseline", x1: x(p * 1200), x2: x(p * 1200), y1: top - 8, y2: height - 26 }));
  }
  for (let p = 0; p * 1200 < length; p += 1) {
    root.append(svg("text", { x: x(p * 1200 + 600), y: height - 8, "text-anchor": "middle", class: "axis-label", text: p < 3 ? `Period ${p + 1}` : "Overtime" }));
  }
  const highlight = options.highlight ? new Set(options.highlight) : null;
  players.forEach((row, i) => {
    const y = top + i * rowHeight;
    const dim = highlight && !highlight.has(row.player.id);
    root.append(svg("text", { x: labelWidth - 8, y: y + rowHeight / 2 + 4, "text-anchor": "end", class: dim ? "axis-label" : "", text: `${row.player.jersey ?? ""} ${row.player.name}` }));
    for (const [start, end] of row.shifts) {
      const g = svg("g", { class: "mark" });
      g.append(svg("rect", { x: x(start), y: y + 3, width: Math.max(1.5, x(end) - x(start) - 1), height: rowHeight - 6, rx: 2, fill: dim ? css("--deemphasis") : css("--series-1") }));
      attachTooltip(g, row.player.name, [{ value: `${gameClock(start)} – ${gameClock(end)}`, name: `${clock(end - start)} shift` }]);
      root.append(g);
    }
  });
  for (const goal of timeline.goals) {
    const gx = x(goal.time);
    const ours = goal.scored_by === "Us";
    const color = ours ? css("--good") : css("--critical");
    const g = svg("g", { class: "mark", style: "cursor:pointer" });
    g.append(svg("line", { x1: gx, x2: gx, y1: top - 10, y2: top + players.length * rowHeight, stroke: color, "stroke-width": 1.5 }));
    g.append(svg("circle", { cx: gx, cy: top - 14, r: 6, fill: color, stroke: css("--surface-1"), "stroke-width": 2 }));
    g.append(svg("rect", { class: "hit", x: gx - 8, y: 0, width: 16, height: top + players.length * rowHeight }));
    const names = goal.on_ice.map((id) => players.find((p) => p.player.id === id)?.player.name).filter(Boolean);
    attachTooltip(g, `${ours ? "Goal for" : "Goal against"} · ${goal.score[0]}-${goal.score[1]}`, [
      { value: gameClock(goal.time), name: goal.strength === "Even" ? "even strength" : goal.strength === "PowerPlay" ? "our power play" : "we were short-handed" },
      { value: names.join(", ") || "—", name: "on ice" },
    ]);
    if (options.onGoal) g.addEventListener("click", () => options.onGoal(goal));
    root.append(g);
  }
  const wrap = el("div", { style: "overflow-x:auto" }, [root]);
  container.replaceChildren(wrap);
  container.append(el("div", { class: "legend" }, [
    el("span", {}, [el("span", { class: "key", style: `background:${css("--series-1")}` }), "Shift"]),
    el("span", {}, [el("span", { class: "key", style: `background:${css("--good")}` }), "Goal for"]),
    el("span", {}, [el("span", { class: "key", style: `background:${css("--critical")}` }), "Goal against"]),
    el("span", {}, [el("span", { class: "key", style: `background:${css("--accent-wash")};border:1px solid var(--border)` }), "Our power play"]),
    el("span", {}, [el("span", { class: "key", style: "background:color-mix(in srgb, var(--div-neg) 12%, transparent);border:1px solid var(--border)" }), "Short-handed"]),
  ]));
}

/**
 * Passing network on a circle. nodes: [{id, label, size}], edges: [{from, to, value, lift}]
 */
function networkChart(container, nodes, edges, options = {}) {
  const width = Math.min(measureWidth(container), 900);
  const height = Math.min(width * 0.8, 680);
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(width, height) / 2 - 90;
  const position = new Map(nodes.map((n, i) => {
    const angle = (i / nodes.length) * Math.PI * 2 - Math.PI / 2;
    return [n.id, { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle), angle }];
  }));
  const maxEdge = Math.max(1, ...edges.map((e) => e.value));
  const maxNode = Math.max(1, ...nodes.map((n) => n.size));
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": "passing network" });
  const edgeLayer = svg("g");
  const nodeLayer = svg("g");
  root.append(edgeLayer, nodeLayer);
  const edgeNodes = [];
  for (const e of edges) {
    const a = position.get(e.from);
    const b = position.get(e.to);
    if (!a || !b || e.value <= 0) continue;
    const mx = (a.x + b.x) / 2 + (cy - (a.y + b.y) / 2) * 0.15;
    const my = (a.y + b.y) / 2 - (cx - (a.x + b.x) / 2) * 0.15;
    const strong = e.lift !== undefined && e.lift >= 1.5 && e.value >= 3;
    const path = svg("path", {
      d: `M${a.x},${a.y}Q${mx},${my} ${b.x},${b.y}`,
      fill: "none",
      stroke: strong ? css("--series-2") : css("--series-1"),
      "stroke-opacity": 0.25 + 0.6 * (e.value / maxEdge),
      "stroke-width": 1 + 5 * (e.value / maxEdge),
      "stroke-linecap": "round",
    });
    const hit = svg("path", { d: path.getAttribute("d"), fill: "none", stroke: "transparent", "stroke-width": 12 });
    const g = svg("g", { class: "mark" }, [path, hit]);
    attachTooltip(g, `${e.fromLabel} → ${e.toLabel}`, [
      { value: String(e.value), name: "passes" },
      ...(e.lift !== undefined ? [{ value: `${fmt(e.lift, 2)}×`, name: "vs expected" }] : []),
    ]);
    edgeNodes.push({ g, e });
    edgeLayer.append(g);
  }
  for (const n of nodes) {
    const p = position.get(n.id);
    const r = 6 + 10 * Math.sqrt(n.size / maxNode);
    const g = svg("g", { class: "mark" });
    g.append(svg("circle", { cx: p.x, cy: p.y, r, fill: css("--series-1"), stroke: css("--surface-1"), "stroke-width": 2 }));
    const out = Math.cos(p.angle) >= 0;
    const anchor = Math.abs(Math.cos(p.angle)) < 0.05 ? "middle" : out ? "start" : "end";
    g.append(svg("text", { x: p.x + (r + 6) * Math.cos(p.angle), y: p.y + (r + 6) * Math.sin(p.angle) + 4 + (Math.sin(p.angle) > 0.95 ? 8 : 0), "text-anchor": anchor, text: n.label }));
    g.append(svg("circle", { class: "hit", cx: p.x, cy: p.y, r: Math.max(12, r + 4) }));
    attachTooltip(g, n.label, [{ value: String(n.size), name: "passes made" }]);
    g.addEventListener("pointerenter", () => {
      for (const { g: eg, e } of edgeNodes) eg.style.opacity = e.from === n.id || e.to === n.id ? "1" : "0.08";
    });
    g.addEventListener("pointerleave", () => {
      for (const { g: eg } of edgeNodes) eg.style.opacity = "1";
    });
    nodeLayer.append(g);
  }
  container.replaceChildren(root);
  container.append(el("div", { class: "legend" }, [
    el("span", {}, [el("span", { class: "key line", style: `background:${css("--series-1")}` }), "Passes (thicker = more)"]),
    el("span", {}, [el("span", { class: "key line", style: `background:${css("--series-2")}` }), "Connection 1.5× or more above expected"]),
  ]));
}

/** Scatter with direct labels. points: [{x, y, label, group}] groups: [{name, color}] */
function scatterChart(container, points, groups, options = {}) {
  const width = measureWidth(container);
  const height = options.height ?? 360;
  const margin = { top: 28, right: 90, bottom: 36, left: 44 };
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const xt = niceTicks(Math.min(...xs), Math.max(...xs), 5);
  const yt = niceTicks(Math.min(...ys), Math.max(...ys), 5);
  const x = (v) => margin.left + ((v - xt[0]) / (xt[xt.length - 1] - xt[0])) * (width - margin.left - margin.right);
  const y = (v) => height - margin.bottom - ((v - yt[0]) / (yt[yt.length - 1] - yt[0])) * (height - margin.top - margin.bottom);
  const root = svg("svg", { class: "chart", viewBox: `0 0 ${width} ${height}`, width, height, role: "img", "aria-label": options.title || "scatter" });
  for (const t of yt) {
    root.append(svg("line", { class: "grid-line", x1: margin.left, x2: width - margin.right, y1: y(t), y2: y(t) }));
    root.append(svg("text", { x: margin.left - 6, y: y(t) + 4, "text-anchor": "end", class: "axis-label", text: fmt(t, 1) }));
  }
  for (const t of xt) {
    root.append(svg("text", { x: x(t), y: height - 18, "text-anchor": "middle", class: "axis-label", text: fmt(t, 1) }));
  }
  if (options.xLabel) root.append(svg("text", { x: width / 2, y: height - 2, "text-anchor": "middle", class: "axis-label", text: options.xLabel }));
  if (options.yLabel) root.append(svg("text", { x: margin.left - 30, y: 12, class: "axis-label", text: options.yLabel }));
  // Greedy label placement: try right, left, above, below; skip if all collide (tooltip and table still carry the name).
  const placed = [];
  const approxWidth = (text) => text.length * 6.2;
  const overlaps = (box) => placed.some((b) => box.x0 < b.x1 && box.x1 > b.x0 && box.y0 < b.y1 && box.y1 > b.y0);
  for (const p of points) placed.push({ x0: x(p.x) - 6, x1: x(p.x) + 6, y0: y(p.y) - 6, y1: y(p.y) + 6 });
  for (const p of points) {
    const color = groups[p.group]?.color || css("--series-1");
    const g = svg("g", { class: "mark" });
    const cx = x(p.x);
    const cy = y(p.y);
    g.append(svg("circle", { cx, cy, r: 5, fill: color, stroke: css("--surface-1"), "stroke-width": 2 }));
    const w = approxWidth(p.label);
    const candidates = [
      { x: cx + 8, y: cy + 4, anchor: "start", box: { x0: cx + 8, x1: cx + 8 + w, y0: cy - 6, y1: cy + 6 } },
      { x: cx - 8, y: cy + 4, anchor: "end", box: { x0: cx - 8 - w, x1: cx - 8, y0: cy - 6, y1: cy + 6 } },
      { x: cx, y: cy - 9, anchor: "middle", box: { x0: cx - w / 2, x1: cx + w / 2, y0: cy - 20, y1: cy - 8 } },
      { x: cx, y: cy + 17, anchor: "middle", box: { x0: cx - w / 2, x1: cx + w / 2, y0: cy + 7, y1: cy + 19 } },
    ];
    const spot = candidates.find((c) => !overlaps(c.box) && c.box.x0 >= 0 && c.box.x1 <= width);
    if (spot) {
      placed.push(spot.box);
      g.append(svg("text", { x: spot.x, y: spot.y, "text-anchor": spot.anchor, text: p.label }));
    }
    g.append(svg("circle", { class: "hit", cx, cy, r: 12 }));
    attachTooltip(g, p.label, [{ value: groups[p.group]?.name || "", name: "style group", color }, ...(p.tip || [])]);
    root.append(g);
  }
  container.replaceChildren(root);
  if (groups.length >= 2) {
    container.append(el("div", { class: "legend" }, groups.map((g) => el("span", {}, [el("span", { class: "key", style: `background:${g.color}` }), g.name]))));
  }
}

/** Percentile bars 0–100 (sequential by value), like a scouting card. */
function percentileBars(container, rows) {
  hBarChart(container, rows.map((r) => ({ ...r, color: divergingColor((r.value - 50) / 50) })), {
    min: 0, max: 100, reference: 50, referenceLabel: "team middle", valueFormat: (v) => `${Math.round(v)}`, valueName: "percentile", labelWidth: 130,
  });
}

// Offensive half-rink in a 200×170 box, net at the top, blue line at the bottom, laid out
// like InStat's "shots by zones" diagram (its left is our left on the page).
const SHOT_ZONE_SHAPES = {
  Slot: [[82, 26], [118, 26], [110, 62], [90, 62]],
  Center: [[90, 62], [110, 62], [134, 112], [66, 112]],
  LeftFlank: [[0, 26], [82, 26], [90, 62], [66, 112], [0, 112]],
  RightFlank: [[118, 26], [200, 26], [200, 112], [134, 112], [110, 62]],
  BlueLineLeft: [[0, 112], [66, 112], [66, 170], [0, 170]],
  BlueLineCenter: [[66, 112], [134, 112], [134, 170], [66, 170]],
  BlueLineRight: [[134, 112], [200, 112], [200, 170], [134, 170]],
};
const SHOT_ZONE_NAMES = {
  Slot: "Slot (in front of the net)",
  Center: "High slot",
  LeftFlank: "Left side",
  RightFlank: "Right side",
  BlueLineLeft: "Left point",
  BlueLineCenter: "Centre point",
  BlueLineRight: "Right point",
};

/**
 * Half-rink diagram of InStat's seven shot zones. zones: [{zone, ...}];
 * style(data) -> {fill, ink, lines: [text], tip: rows}, where data is undefined for a zone not
 * listed. options.legend: nodes shown under the rink.
 */
function zoneMap(container, zones, style, options = {}) {
  const width = Math.min(measureWidth(container), options.maxWidth ?? 420);
  const scale = width / 200;
  const height = Math.round(170 * scale);
  const id = `rink${Math.random().toString(36).slice(2)}`;
  const root = svg("svg", { class: "chart", viewBox: "0 0 200 170", width, height, role: "img", "aria-label": options.title || "zone map" });
  const outline = "M0,170V28Q0,0 28,0H172Q200,0 200,28V170Z";
  const clip = svg("clipPath", { id });
  clip.append(svg("path", { d: outline }));
  root.append(clip);
  const layer = svg("g", { "clip-path": `url(#${id})` });
  root.append(layer);
  const byZone = new Map(zones.map((z) => [z.zone, z]));
  for (const [zone, points] of Object.entries(SHOT_ZONE_SHAPES)) {
    const look = style(byZone.get(zone));
    const shape = svg("polygon", { points: points.map((p) => p.join(",")).join(" "), fill: look.fill, stroke: css("--surface-1"), "stroke-width": 2 / scale, "stroke-linejoin": "round" });
    layer.append(shape);
    attachTooltip(shape, SHOT_ZONE_NAMES[zone], () => look.tip);
    const cx = points.reduce((sum, p) => sum + p[0], 0) / points.length;
    const cy = points.reduce((sum, p) => sum + p[1], 0) / points.length;
    look.lines.forEach((line, i) => {
      const first = i === 0;
      const label = svg("text", { x: cx, y: cy + 4 + (i - (look.lines.length - 1) / 2) * 15, "text-anchor": "middle", "pointer-events": "none", style: `fill:${look.ink};font-size:${(first ? 13 : 10) / scale}px;font-weight:${first ? 600 : 400}` });
      label.textContent = line;
      layer.append(label);
    });
  }
  const lineInk = css("--text-muted");
  root.append(
    svg("line", { x1: 0, x2: 200, y1: 26, y2: 26, stroke: css("--div-neg"), "stroke-width": 0.8, opacity: 0.7, "pointer-events": "none" }),
    svg("path", { d: "M90,26A10,10 0 0 0 110,26", fill: "none", stroke: lineInk, "stroke-width": 0.8, "pointer-events": "none" }),
    svg("rect", { x: 94, y: 19, width: 12, height: 7, fill: "none", stroke: lineInk, "stroke-width": 1.2, rx: 1.5, "pointer-events": "none" }),
    svg("path", { d: outline, fill: "none", stroke: css("--axis"), "stroke-width": 1.2, "pointer-events": "none" }),
    svg("line", { x1: 0, x2: 200, y1: 169, y2: 169, stroke: css("--series-1"), "stroke-width": 2, "pointer-events": "none" }),
  );
  container.replaceChildren(root);
  if (options.legend) container.append(el("div", { class: "legend" }, options.legend));
}

/**
 * Half-rink shot map. zones: [{zone, shots, on_goal}]; shading = share of shots.
 * The number in each zone is shots / on goal.
 */
function shotMap(container, zones, options = {}) {
  const total = zones.reduce((sum, z) => sum + z.shots, 0);
  const most = Math.max(1, ...zones.map((z) => z.shots));
  zoneMap(container, zones, (z) => {
    const data = z || { shots: 0, on_goal: 0 };
    const fill = data.shots ? sequentialColor(0.15 + 0.85 * (data.shots / most)) : css("--surface-2");
    return {
      fill,
      ink: data.shots ? inkOn(fill) : css("--text-muted"),
      lines: [`${data.shots} / ${data.on_goal}`],
      tip: [
        { value: `${data.shots}`, name: "shots" },
        { value: `${data.on_goal}`, name: "on goal" },
        { value: total ? `${Math.round((100 * data.shots) / total)}%` : "—", name: "of all shots" },
      ],
    };
  }, {
    ...options,
    legend: [
      el("span", {}, [el("span", { class: "key", style: `background:${sequentialColor(0.15)}` }), "few shots"]),
      el("span", {}, [el("span", { class: "key", style: `background:${sequentialColor(1)}` }), "most shots"]),
      el("span", { class: "muted", text: "numbers = shots / on goal · blue line at the bottom" }),
    ],
  });
}

const NET_AREA_ROWS = [["TopLeft", "TopCenter", "TopRight"], ["MiddleLeft", "Middle", "MiddleRight"], ["BottomLeft", "BottomCenter", "BottomRight"]];
const NET_AREA_NAMES = {
  TopLeft: "Top left", TopCenter: "Top middle", TopRight: "Top right",
  MiddleLeft: "Middle left", Middle: "Middle", MiddleRight: "Middle right",
  BottomLeft: "Bottom left", BottomCenter: "Bottom middle", BottomRight: "Bottom right",
};

/**
 * The net seen from the shooter's side, split 3×3 like InStat's net diagrams.
 * cells: [{area, ...}]; style(data) -> {fill, ink, lines: [text], tip: rows}.
 */
function netMap(container, cells, style, options = {}) {
  const width = Math.min(measureWidth(container), options.maxWidth ?? 360);
  const scale = width / 240;
  const height = Math.round(160 * scale);
  const root = svg("svg", { class: "chart", viewBox: "0 0 240 160", width, height, role: "img", "aria-label": options.title || "net map" });
  const byArea = new Map(cells.map((c) => [c.area, c]));
  const [left, top, cellW, cellH] = [14, 14, 212 / 3, 144 / 3];
  NET_AREA_ROWS.forEach((row, r) => row.forEach((area, c) => {
    const look = style(byArea.get(area));
    const x = left + c * cellW;
    const y = top + r * cellH;
    const shape = svg("rect", { x: x + 1, y: y + 1, width: cellW - 2, height: cellH - 2, rx: 3, fill: look.fill });
    root.append(shape);
    attachTooltip(shape, NET_AREA_NAMES[area], () => look.tip);
    look.lines.forEach((line, i) => {
      const first = i === 0;
      const label = svg("text", { x: x + cellW / 2, y: y + cellH / 2 + 4 + (i - (look.lines.length - 1) / 2) * 15, "text-anchor": "middle", "pointer-events": "none", style: `fill:${look.ink};font-size:${(first ? 13 : 10) / scale}px;font-weight:${first ? 600 : 400}` });
      label.textContent = line;
      root.append(label);
    });
  }));
  root.append(svg("path", { d: `M${left - 6},160V${top - 6}H${240 - left + 6}V160`, fill: "none", stroke: css("--critical"), "stroke-width": 6, "stroke-linejoin": "round", "pointer-events": "none" }));
  container.replaceChildren(root);
  if (options.legend) container.append(el("div", { class: "legend" }, options.legend));
}

function netAreaName(area) {
  return NET_AREA_NAMES[area] || area;
}

/** Feet from the middle of the opponent's goal line (89 ft from centre ice) to a rink point. */
function shotDistance(at) {
  return Math.hypot(at.across, 89 - at.along);
}

/**
 * Every shot attempt where InStat's shooting chart drew it, on a half rink in feet (net at the top,
 * blue line at the bottom). shots: [{at: {along, across}, goal, ...}];
 * options.tip(shot) -> {title, rows} describes a dot on hover; options.goalColor is a colour
 * token for goals (default --series-2).
 */
function shotPlot(container, shots, options = {}) {
  const width = Math.min(measureWidth(container), options.maxWidth ?? 420);
  const [x0, y0, w, h] = [-44, -13, 88, 79];
  const height = Math.round((width * h) / w);
  const root = svg("svg", { class: "chart", viewBox: `${x0} ${y0} ${w} ${h}`, width, height, role: "img", "aria-label": options.title || "shot locations" });
  const id = `rink${Math.random().toString(36).slice(2)}`;
  const outline = "M-42.5,64V17A28,28 0 0 1 -14.5,-11H14.5A28,28 0 0 1 42.5,17V64Z";
  const clip = svg("clipPath", { id });
  clip.append(svg("path", { d: outline }));
  root.append(clip, svg("path", { d: outline, fill: css("--surface-2"), stroke: "none" }));
  const lines = svg("g", { "clip-path": `url(#${id})`, "pointer-events": "none" });
  const redLine = css("--div-neg");
  const muted = css("--text-muted");
  lines.append(
    svg("line", { x1: -42.5, x2: 42.5, y1: 0, y2: 0, stroke: redLine, "stroke-width": 0.3, opacity: 0.7 }),
    svg("path", { d: "M-6,0A6,6 0 0 0 6,0Z", fill: css("--accent-wash"), stroke: redLine, "stroke-width": 0.25 }),
    svg("rect", { x: -3, y: -3.3, width: 6, height: 3.3, fill: "none", stroke: muted, "stroke-width": 0.4 }),
    ...[-22, 22].flatMap((cx) => [
      svg("circle", { cx, cy: 20, r: 15, fill: "none", stroke: redLine, "stroke-width": 0.3, opacity: 0.6 }),
      svg("circle", { cx, cy: 20, r: 1, fill: redLine, opacity: 0.6 }),
    ]),
    svg("line", { x1: -42.5, x2: 42.5, y1: 63, y2: 63, stroke: css("--series-1"), "stroke-width": 2, opacity: 0.6 }),
  );
  root.append(lines, svg("path", { d: outline, fill: "none", stroke: css("--axis"), "stroke-width": 0.5, "pointer-events": "none" }));
  const ordered = [...shots].sort((a, b) => Number(a.goal) - Number(b.goal));
  for (const shot of ordered) {
    const dot = svg("circle", {
      cx: shot.at.across,
      cy: 89 - shot.at.along,
      r: shot.goal ? 2.3 : 1.6,
      fill: css(shot.goal ? options.goalColor ?? "--series-2" : "--series-1"),
      "fill-opacity": shot.goal ? 1 : 0.7,
      stroke: css("--surface-1"),
      "stroke-width": 0.4,
    });
    const tip = options.tip ? options.tip(shot) : { title: shot.goal ? "Goal" : "Shot", rows: [] };
    attachTooltip(dot, tip.title, () => tip.rows);
    root.append(dot);
  }
  container.replaceChildren(root);
  const goals = shots.filter((x) => x.goal).length;
  container.append(el("div", { class: "legend" }, [
    el("span", {}, [el("span", { class: "key", style: `background:${css("--series-1")}` }), `attempt, no goal (${shots.length - goals})`]),
    el("span", {}, [el("span", { class: "key", style: `background:${css(options.goalColor ?? "--series-2")}` }), `goal (${goals})`]),
    el("span", { class: "muted", text: "net at the top · blue line at the bottom" }),
  ]));
}

/** Our zone, the neutral zone or theirs, by where a rink point lies against the blue lines. */
function rinkZone(at) {
  if (at.along < -25) return "ours";
  return at.along > 25 ? "theirs" : "neutral";
}

/** An empty full rink in feet (x −100…100 along, y −42.5…42.5 across), our net on the left. */
function fullRink(container, options = {}) {
  const width = Math.min(measureWidth(container), options.maxWidth ?? 640);
  const height = Math.round((width * 89) / 204);
  const root = svg("svg", { class: "chart", viewBox: "-102 -44.5 204 89", width, height, role: "img", "aria-label": options.title || "rink map" });
  const outline = "M-72,-42.5H72A28,28 0 0 1 100,-14.5V14.5A28,28 0 0 1 72,42.5H-72A28,28 0 0 1 -100,14.5V-14.5A28,28 0 0 1 -72,-42.5Z";
  const id = `rink${Math.random().toString(36).slice(2)}`;
  const clip = svg("clipPath", { id });
  clip.append(svg("path", { d: outline }));
  root.append(clip, svg("path", { d: outline, fill: css("--surface-2"), stroke: "none" }));
  const red = css("--div-neg");
  const blue = css("--series-1");
  const lines = svg("g", { "clip-path": `url(#${id})`, "pointer-events": "none" });
  const vertical = (x, color, w, opacity = 0.7) => svg("line", { x1: x, x2: x, y1: -42.5, y2: 42.5, stroke: color, "stroke-width": w, opacity });
  lines.append(
    vertical(-89, red, 0.4), vertical(89, red, 0.4), vertical(0, red, 1, 0.5), vertical(-25, blue, 1.6, 0.6), vertical(25, blue, 1.6, 0.6),
    svg("circle", { cx: 0, cy: 0, r: 15, fill: "none", stroke: blue, "stroke-width": 0.3, opacity: 0.6 }),
    ...[-69, 69].flatMap((cx) => [-22, 22].map((cy) => svg("circle", { cx, cy, r: 15, fill: "none", stroke: red, "stroke-width": 0.3, opacity: 0.5 }))),
    ...[-69, -20, 20, 69].flatMap((cx) => [-22, 22].map((cy) => svg("circle", { cx, cy, r: 0.9, fill: red, opacity: 0.5 }))),
    ...[-1, 1].map((side) => svg("rect", { x: side < 0 ? -92.3 : 89, y: -3, width: 3.3, height: 6, fill: "none", stroke: css("--text-muted"), "stroke-width": 0.5 })),
  );
  root.append(lines, svg("path", { d: outline, fill: "none", stroke: css("--axis"), "stroke-width": 0.6, "pointer-events": "none" }));
  return { root, width };
}

/** Where each faceoff dot sits in rink feet; "Left" is the top of the drawing (our left
 * when facing their net). */
const FACEOFF_SPOTS = {
  OurZoneLeft: [-69, -22, "Our zone, left dot"], OurZoneRight: [-69, 22, "Our zone, right dot"],
  NeutralOurSideLeft: [-20, -22, "Neutral zone, our side, left"], NeutralOurSideRight: [-20, 22, "Neutral zone, our side, right"],
  CenterIce: [0, 0, "Centre ice"],
  NeutralTheirSideLeft: [20, -22, "Neutral zone, their side, left"], NeutralTheirSideRight: [20, 22, "Neutral zone, their side, right"],
  TheirZoneLeft: [69, -22, "Their zone, left dot"], TheirZoneRight: [69, 22, "Their zone, right dot"],
};

function faceoffSpotName(spot) {
  return FACEOFF_SPOTS[spot]?.[2] ?? spot;
}

/**
 * Our faceoff win % at every dot, on a full rink with our net on the left.
 * spots: [{spot, won, lost}]; blue = we win most there, red = we lose most.
 */
function faceoffMap(container, spots, options = {}) {
  const { root } = fullRink(container, options);
  for (const { spot, won, lost } of spots) {
    const [x, y] = FACEOFF_SPOTS[spot] || [];
    if (x === undefined) continue;
    const total = won + lost;
    const rate = total ? (100 * won) / total : null;
    const fill = rate === null ? css("--surface-1") : divergingColor(Math.max(-1, Math.min(1, (rate - 50) / 30)));
    const ink = rate === null ? css("--text-muted") : inkOn(fill);
    const g = svg("g", { class: "mark" }, [
      svg("circle", { cx: x, cy: y, r: 11, fill, stroke: css("--surface-1"), "stroke-width": 0.8 }),
      svg("text", { x, y: y - 0.5, "text-anchor": "middle", style: `fill:${ink};font-size:5.8px;font-weight:650`, text: rate === null ? "—" : `${Math.round(rate)}%` }),
      svg("text", { x, y: y + 6, "text-anchor": "middle", style: `fill:${ink};font-size:4.6px`, text: `${won}–${lost}` }),
    ]);
    attachTooltip(g, faceoffSpotName(spot), () => [
      { value: String(won), name: "won" }, { value: String(lost), name: "lost" }, { value: rate === null ? "—" : `${Math.round(rate)}%`, name: "win rate" },
    ]);
    root.append(g);
  }
  container.replaceChildren(root);
  const zone = (names) => {
    const [won, lost] = spots.filter((s) => names.includes(s.spot)).reduce(([w, l], s) => [w + s.won, l + s.lost], [0, 0]);
    return won + lost ? `${Math.round((100 * won) / (won + lost))}% (${won}–${lost})` : "—";
  };
  container.append(el("div", { class: "legend" }, [
    el("span", {}, [el("span", { class: "key", style: `background:${divergingColor(1)}` }), "we win most"]),
    el("span", {}, [el("span", { class: "key", style: `background:${divergingColor(-1)}` }), "we lose most"]),
    el("span", { class: "muted", text: `our zone ${zone(["OurZoneLeft", "OurZoneRight"])} · neutral ${zone(["NeutralOurSideLeft", "NeutralOurSideRight", "CenterIce", "NeutralTheirSideLeft", "NeutralTheirSideRight"])} · their zone ${zone(["TheirZoneLeft", "TheirZoneRight"])} · our net on the left` }),
  ]));
}

/**
 * Events on a full rink in feet, our net on the left. events: [{kind, at: {along, across}}];
 * styles: {kind: {label, color, mark: "dot" | "ring" | "cross"}} (kinds without a style are
 * skipped). The legend counts each kind by zone.
 */
function rinkPlot(container, events, styles, options = {}) {
  const { root } = fullRink(container, options);
  for (const event of events) {
    const style = styles[event.kind];
    if (!style) continue;
    const [x, y] = [event.at.along, event.at.across];
    const r = 2;
    let mark;
    if (style.mark === "cross") {
      mark = svg("path", { d: `M${x - r},${y - r}L${x + r},${y + r}M${x - r},${y + r}L${x + r},${y - r}`, stroke: style.color, "stroke-width": 0.9, "stroke-linecap": "round" });
    } else {
      mark = svg("circle", { cx: x, cy: y, r, fill: style.mark === "ring" ? css("--surface-1") : style.color, stroke: style.mark === "ring" ? style.color : css("--surface-1"), "stroke-width": style.mark === "ring" ? 0.8 : 0.4 });
    }
    const hit = svg("g", { class: "mark" }, [svg("circle", { cx: x, cy: y, r: r + 1.5, fill: "transparent" }), mark]);
    attachTooltip(hit, style.label, () => [{ value: { ours: "our zone", neutral: "neutral zone", theirs: "their zone" }[rinkZone(event.at)], name: "" }]);
    root.append(hit);
  }
  container.replaceChildren(root);
  const counts = (kind) => ["ours", "neutral", "theirs"].map((zone) => events.filter((e) => e.kind === kind && rinkZone(e.at) === zone).length);
  container.append(el("div", { class: "legend" }, [
    ...Object.entries(styles).map(([kind, style]) => {
      const [ours, neutral, theirs] = counts(kind);
      const keys = {
        dot: () => el("span", { class: "key", style: `background:${style.color};border-radius:50%` }),
        ring: () => el("span", { class: "key", style: `border:1.5px solid ${style.color};border-radius:50%;background:transparent` }),
        cross: () => el("span", { style: `color:${style.color};font-weight:700;margin-right:4px`, text: "×" }),
      };
      const key = keys[style.mark]();
      return el("span", {}, [key, `${style.label}: ${ours} · ${neutral} · ${theirs}`]);
    }),
    el("span", { class: "muted", text: "counts: our zone · neutral · their zone · our net on the left" }),
  ]));
}

// Full rink in a 300×130 box, our net on the left; corners are one area drawn top and bottom.
const BATTLE_AREA_SHAPES = {
  BehindOwnGoal: [[[0, 0], [22, 0], [22, 130], [0, 130]]],
  OwnCorners: [[[22, 0], [80, 0], [80, 40], [22, 40]], [[22, 90], [80, 90], [80, 130], [22, 130]]],
  OwnSlot: [[[22, 40], [80, 40], [80, 90], [22, 90]]],
  OwnBlueLine: [[[80, 0], [105, 0], [105, 130], [80, 130]]],
  NeutralZone: [[[105, 0], [195, 0], [195, 130], [105, 130]]],
  OppBlueLine: [[[195, 0], [220, 0], [220, 130], [195, 130]]],
  OppSlot: [[[220, 40], [278, 40], [278, 90], [220, 90]]],
  OppCorners: [[[220, 0], [278, 0], [278, 40], [220, 40]], [[220, 90], [278, 90], [278, 130], [220, 130]]],
  BehindOppGoal: [[[278, 0], [300, 0], [300, 130], [278, 130]]],
};
const BATTLE_AREA_NAMES = {
  OwnSlot: "In front of our net",
  BehindOwnGoal: "Behind our net",
  OwnCorners: "Our corners",
  OwnBlueLine: "Our blue line",
  NeutralZone: "Neutral zone",
  OppBlueLine: "Their blue line",
  OppCorners: "Their corners",
  BehindOppGoal: "Behind their net",
  OppSlot: "In front of their net",
};

/**
 * Full-rink puck-battle map. areas: [{area, battles, won}]; colour = win % (blue above 50,
 * red below), labelled won / battles.
 */
function battleMap(container, areas, options = {}) {
  const width = Math.min(measureWidth(container), options.maxWidth ?? 760);
  const scale = width / 300;
  const height = Math.round(130 * scale);
  const id = `rink${Math.random().toString(36).slice(2)}`;
  const root = svg("svg", { class: "chart", viewBox: "0 0 300 130", width, height, role: "img", "aria-label": options.title || "puck battles by area" });
  const outline = "M24,0H276Q300,0 300,24V106Q300,130 276,130H24Q0,130 0,106V24Q0,0 24,0Z";
  const clip = svg("clipPath", { id });
  clip.append(svg("path", { d: outline }));
  root.append(clip);
  const layer = svg("g", { "clip-path": `url(#${id})` });
  root.append(layer);
  const byArea = new Map(areas.map((a) => [a.area, a]));
  const text = (x, y, content, ink, size, weight = 600) => {
    const node = svg("text", { x, y, "text-anchor": "middle", "pointer-events": "none", style: `fill:${ink};font-size:${size / scale}px;font-weight:${weight}` });
    node.textContent = content;
    layer.append(node);
  };
  for (const [area, polygons] of Object.entries(BATTLE_AREA_SHAPES)) {
    const data = byArea.get(area) || { battles: 0, won: 0 };
    const rate = data.battles ? (100 * data.won) / data.battles : null;
    const fill = rate === null ? css("--surface-2") : divergingColor(Math.max(-1, Math.min(1, (rate - 50) / 25)));
    const ink = rate === null ? css("--text-muted") : inkOn(fill);
    polygons.forEach((points, i) => {
      const shape = svg("polygon", { points: points.map((p) => p.join(",")).join(" "), fill, stroke: css("--surface-1"), "stroke-width": 2 / scale });
      layer.append(shape);
      attachTooltip(shape, BATTLE_AREA_NAMES[area], () => [
        { value: `${data.won} of ${data.battles}`, name: "battles won" },
        { value: rate === null ? "—" : `${Math.round(rate)}%`, name: area.endsWith("Corners") ? "win rate (both corners)" : "win rate" },
      ]);
      const cx = points.reduce((s, p) => s + p[0], 0) / points.length;
      const behindNet = area.startsWith("Behind");
      const cy = points.reduce((s, p) => s + p[1], 0) / points.length - (behindNet ? 28 : 0);
      const narrow = behindNet || area.endsWith("BlueLine");
      if (i === 0) {
        text(cx, cy - 2, rate === null ? "—" : `${Math.round(rate)}%`, ink, narrow ? 11 : 13);
        text(cx, cy + 10 / scale + 1, `${data.won}/${data.battles}`, ink, 10, 400);
      } else {
        text(cx, cy + 3, "same area", ink, 9, 400);
      }
    });
  }
  const line = (x, color, w) => svg("line", { x1: x, x2: x, y1: 0, y2: 130, stroke: color, "stroke-width": w / scale, "pointer-events": "none", opacity: 0.8 });
  root.append(
    line(22, css("--div-neg"), 1), line(278, css("--div-neg"), 1), line(150, css("--div-neg"), 1.5),
    line(105, css("--series-1"), 3), line(195, css("--series-1"), 3),
    svg("rect", { x: 17, y: 60, width: 5, height: 10, fill: "none", stroke: css("--text-muted"), "stroke-width": 1.5 / scale, "pointer-events": "none" }),
    svg("rect", { x: 278, y: 60, width: 5, height: 10, fill: "none", stroke: css("--text-muted"), "stroke-width": 1.5 / scale, "pointer-events": "none" }),
    svg("path", { d: outline, fill: "none", stroke: css("--axis"), "stroke-width": 1.5 / scale, "pointer-events": "none" }),
  );
  container.replaceChildren(root);
  container.append(el("div", { class: "legend" }, [
    el("span", {}, [el("span", { class: "key", style: `background:${divergingColor(-1)}` }), "losing most battles"]),
    el("span", {}, [el("span", { class: "key", style: `background:${divergingColor(0)}` }), "even"]),
    el("span", {}, [el("span", { class: "key", style: `background:${divergingColor(1)}` }), "winning most"]),
    el("span", { class: "muted", text: "our net on the left · % won, then won/total" }),
  ]));
}

function battleAreaName(area) {
  return BATTLE_AREA_NAMES[area] || area;
}

function shotZoneName(zone) {
  return SHOT_ZONE_NAMES[zone] || zone;
}

/**
 * Sortable data table. columns: [{key, label, format?, value?(row), left?, title?}]
 * Values shown with textContent; returns the table element.
 */
function dataTable(container, columns, rows, options = {}) {
  let sortKey = options.sortKey ?? null;
  let descending = options.descending ?? true;
  const valueOf = (col, row) => (col.value ? col.value(row) : row[col.key]);
  const wrap = el("div", { class: "table-wrap" });
  // Columns with `tone: "higher" | "lower"` tint each cell by how far it sits from the
  // column average (green = better, red = worse).
  const toneStats = new Map(columns.filter((c) => c.tone).map((col) => {
    const values = rows.map((r) => valueOf(col, r)).filter((v) => typeof v === "number" && !Number.isNaN(v));
    const avg = values.reduce((sum, v) => sum + v, 0) / Math.max(1, values.length);
    const sd = Math.sqrt(values.reduce((sum, v) => sum + (v - avg) ** 2, 0) / Math.max(1, values.length - 1));
    return [col.key, { avg, sd }];
  }));
  const toneStyle = (col, value) => {
    const stats = toneStats.get(col.key);
    if (!stats || typeof value !== "number" || Number.isNaN(value) || !(stats.sd > 0)) return "";
    const z = ((value - stats.avg) / stats.sd) * (col.tone === "lower" ? -1 : 1);
    if (Math.abs(z) < 0.35) return "";
    const color = z > 0 ? css("--good") : css("--critical");
    const strength = Math.round(Math.min(1, Math.abs(z) / 2) * 30);
    return `background:color-mix(in srgb, ${color} ${strength}%, transparent)`;
  };
  const render = () => {
    const sorted = [...rows];
    if (sortKey !== null) {
      const col = columns.find((c) => c.key === sortKey);
      sorted.sort((a, b) => {
        const va = valueOf(col, a);
        const vb = valueOf(col, b);
        const na = va === null || va === undefined || Number.isNaN(va);
        const nb = vb === null || vb === undefined || Number.isNaN(vb);
        if (na && nb) return 0;
        if (na) return 1;
        if (nb) return -1;
        const cmp = typeof va === "string" ? va.localeCompare(vb) : va - vb;
        return descending ? -cmp : cmp;
      });
    }
    const head = el("tr", {}, columns.map((col) => {
      // Clicking a defined term explains it; clicking elsewhere on the header sorts.
      const th = el("th", { class: col.left ? "left" : "", title: col.title || "" }, [term(col.label)]);
      if (sortKey === col.key) th.append(el("span", { class: "arrow", text: descending ? "▼" : "▲" }));
      th.addEventListener("click", () => {
        if (sortKey === col.key) descending = !descending;
        else { sortKey = col.key; descending = !col.left; }
        render();
      });
      return th;
    }));
    const body = sorted.map((row) => {
      const tr = el("tr", { class: `${options.onRow ? "clickable" : ""} ${options.dim && options.dim(row) ? "dim" : ""}` });
      for (const col of columns) {
        const raw = valueOf(col, row);
        const shown = col.render ? col.render(row) : col.format ? col.format(raw, row) : raw ?? "—";
        tr.append(el("td", { class: `${col.left ? "left" : ""} ${col.wrap ? "wrap" : ""}`, style: col.tone ? toneStyle(col, raw) : undefined }, [shown instanceof Node ? shown : String(shown)]));
      }
      if (options.onRow) tr.addEventListener("click", () => options.onRow(row));
      return tr;
    });
    wrap.replaceChildren(el("table", {}, [el("thead", {}, [head]), el("tbody", {}, body)]));
    if (toneStats.size) {
      wrap.append(el("div", { class: "tone-legend" }, [
        el("span", { class: "swatch good" }), "better than the rest of the team",
        el("span", { class: "swatch bad" }), "worse (stronger colour = further from average)",
      ]));
    }
  };
  render();
  container.replaceChildren(wrap);
  return wrap;
}

/** Card with title, description and a chart/table toggle. */
function chartCard(title, description, drawChart, drawTable, options = {}) {
  const body = el("div");
  let showing = "chart";
  const toggle = drawTable ? el("button", { class: "small", text: "Table" }) : null;
  const draw = () => {
    Tooltip.hide();
    if (showing === "chart") drawChart(body);
    else drawTable(body);
    if (toggle) toggle.textContent = showing === "chart" ? "Table" : "Chart";
  };
  if (toggle) toggle.addEventListener("click", () => { showing = showing === "chart" ? "table" : "chart"; draw(); });
  const card = el("div", { class: `card ${options.class || ""}` }, [
    el("div", { class: "card-head" }, [
      el("div", {}, [el("h3", {}, [title, ...[].concat(options.source || [])]), description ? el("p", { class: "desc", text: description }) : null]),
      el("div", { class: "actions" }, [toggle]),
    ]),
    body,
  ]);
  requestAnimationFrame(draw);
  card.redraw = draw;
  return card;
}

window.Charts = {
  el, svg, css, fmt, pct, signed, clock, minutes, gameClock, SERIES, Tooltip, attachTooltip, setGlossary, definition, term, explain,
  hBarChart, lineChart, groupedColumns, heatmap, shiftChart, networkChart, scatterChart, percentileBars,
  zoneMap, shotMap, shotZoneName, shotPlot, shotDistance, rinkPlot, faceoffMap, faceoffSpotName, netMap, netAreaName, battleMap, battleAreaName, dataTable, chartCard, sequentialColor, divergingColor, inkOn,
};
})();
