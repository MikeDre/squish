"use strict";
const CFG = window.__SQUISH__;
const $ = (id) => document.getElementById(id);
const img = $("preview"), stage = $("stage"), box = $("sel"), shade = $("shade");

let sel = { ...CFG.seed };
let lock = CFG.lock ? CFG.lock[0] / CFG.lock[1] : null; // width / height, or null
let finished = false;

img.src = "/preview" + location.search;
$("file").textContent = CFG.file_name;
$("source").textContent =
  `${CFG.source_w} × ${CFG.source_h} · ${fmtBytes(CFG.source_bytes)}`;

function fmtBytes(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + " MB";
  if (n >= 1e3) return Math.round(n / 1e3) + " KB";
  return n + " B";
}

/** Source px -> CSS px, using the preview's rendered size. */
function scale() { return img.clientWidth / CFG.source_w; }
function s2p(v) { return v * scale(); }
function p2s(v) { return v / scale(); }

/**
 * The image's box in the stage's *content* space, in CSS px — scroll included.
 *
 * The selection and shade are absolutely positioned inside the stage, so they
 * scroll with the image. Positioning them in content space is what keeps them
 * glued to the same image region while panning, with no re-render on scroll.
 */
function frame() {
  const i = img.getBoundingClientRect(), s = stage.getBoundingClientRect();
  return {
    left: i.left - s.left + stage.scrollLeft,
    top: i.top - s.top + stage.scrollTop,
    w: i.width,
    h: i.height,
  };
}

function clampSel() {
  sel.w = Math.max(1, Math.min(Math.round(sel.w), CFG.source_w));
  sel.h = Math.max(1, Math.min(Math.round(sel.h), CFG.source_h));
  sel.x = Math.max(0, Math.min(Math.round(sel.x), CFG.source_w - sel.w));
  sel.y = Math.max(0, Math.min(Math.round(sel.y), CFG.source_h - sel.h));
}

function gcd(a, b) { return b ? gcd(b, a % b) : a; }
function ratioLabel(w, h) {
  const g = gcd(w, h), rw = w / g, rh = h / g;
  if (rw <= 40 && rh <= 40) return `${rw}:${rh}`;
  return (w / h).toFixed(2) + ":1";
}

/**
 * The ratio in force right now: the transient one while shift is held during a
 * new drag, otherwise the sticky one from the presets or `--crop`. Shift is
 * deliberately transient — a modifier that silently rewrote the sticky lock
 * would leave the "Free" preset lit while the box refused to go free.
 */
function activeLock() {
  if (drag && drag.mode === "new" && drag.shift) return drag.baseRatio;
  return lock;
}

/**
 * Force the selection to the active ratio.
 *
 * The anchor matters: while a corner is being dragged out, that corner has to
 * stay put, or the box crawls away from where the drag began. Only outside a
 * drag is the centre the right thing to hold.
 */
function applyLock() {
  const ratio = activeLock();
  if (!ratio) return;

  // Which axis leads. An edge handle must follow the edge being dragged; a
  // corner or a fresh drag follows whichever way the pointer has gone further,
  // so the box tracks the pointer instead of collapsing on the other axis.
  const drive = drag ? drag.drive : "dominant";
  let w = sel.w, h = sel.h;
  if (drive === "w") h = w / ratio;
  else if (drive === "h") w = h * ratio;
  else if (h <= 0 || w / h > ratio) h = w / ratio;
  else w = h * ratio;

  if (h > CFG.source_h) { h = CFG.source_h; w = h * ratio; }
  if (w > CFG.source_w) { w = CFG.source_w; h = w / ratio; }
  w = Math.max(1, Math.round(w));
  h = Math.max(1, Math.round(h));

  // Hold still whatever the user is not moving: the opposite edge or corner
  // during a drag, the centre otherwise.
  if (drag && drag.keep) {
    sel.x = axisPlace(drag.keep.x, drag.fix.x, w);
    sel.y = axisPlace(drag.keep.y, drag.fix.y, h);
  } else {
    sel.x = Math.round(sel.x + sel.w / 2 - w / 2);
    sel.y = Math.round(sel.y + sel.h / 2 - h / 2);
  }
  sel.w = w;
  sel.h = h;
}

/** Place one axis so its pinned edge — or its centre — stays put. */
function axisPlace(mode, fixed, size) {
  if (mode === "min") return fixed;
  if (mode === "max") return fixed - size;
  return Math.round(fixed - size / 2);
}

function render() {
  applyLock();
  clampSel();
  const f = frame();
  box.style.left = f.left + s2p(sel.x) + "px";
  box.style.top = f.top + s2p(sel.y) + "px";
  box.style.width = s2p(sel.w) + "px";
  box.style.height = s2p(sel.h) + "px";

  shade.style.left = f.left + "px";
  shade.style.top = f.top + "px";
  shade.style.width = f.w + "px";
  shade.style.height = f.h + "px";
  const x0 = s2p(sel.x), y0 = s2p(sel.y), x1 = x0 + s2p(sel.w), y1 = y0 + s2p(sel.h);
  shade.style.clipPath =
    `polygon(0 0, ${f.w}px 0, ${f.w}px ${f.h}px, 0 ${f.h}px, 0 0,` +
    ` ${x0}px ${y0}px, ${x0}px ${y1}px, ${x1}px ${y1}px, ${x1}px ${y0}px, ${x0}px ${y0}px)`;

  $("size").textContent = `${sel.w} × ${sel.h} px`;
  $("meta").textContent = `+${sel.x}+${sel.y} · ${ratioLabel(sel.w, sel.h)}`;
  placeReadout(f);
  onSelectionRendered();
}

/**
 * Keep the readout on screen. It hangs below the box by default, but the stage
 * clips its overflow — so a selection flush with the bottom or right edge would
 * hide the one number the user came here for. Tuck it inside the box instead.
 */
function placeReadout(f) {
  const r = $("readout");
  // Visible position, not content position: when zoomed in, what matters is
  // whether the readout lands inside the scrolled viewport.
  const left = f.left + s2p(sel.x) - stage.scrollLeft;
  const bottom = f.top + s2p(sel.y) + s2p(sel.h) - stage.scrollTop;
  const below = bottom + 6 + r.offsetHeight <= stage.clientHeight;
  r.style.top = below ? "100%" : "auto";
  r.style.bottom = below ? "auto" : "6px";
  r.style.marginTop = below ? "6px" : "0";
  // Align its right edge with the stage's when it would otherwise run off.
  r.style.left = Math.min(0, stage.clientWidth - (left + r.offsetWidth)) + "px";
}

/** Extension point used by later tasks (live estimate). */
function onSelectionRendered() {}

/** Pointer position in source px, clamped to the image. */
function pointerToSource(ev) {
  const i = img.getBoundingClientRect();
  return {
    x: Math.max(0, Math.min(CFG.source_w, p2s(ev.clientX - i.left))),
    y: Math.max(0, Math.min(CFG.source_h, p2s(ev.clientY - i.top))),
  };
}

let drag = null;
/** null = fit to the window; a number = CSS px per preview px. */
let zoom = null;
let spaceHeld = false;
let panning = null;

/** The drag a grab on `handle` starts: the opposite edge or corner is pinned. */
function resizeDrag(name) {
  const f = { x0: sel.x, y0: sel.y, x1: sel.x + sel.w, y1: sel.y + sel.h };
  const keep = {
    x: name.includes("w") ? "max" : name.includes("e") ? "min" : "center",
    y: name.includes("n") ? "max" : name.includes("s") ? "min" : "center",
  };
  return {
    mode: "resize",
    h: name,
    fixed: f,
    keep,
    fix: {
      x: keep.x === "max" ? f.x1 : keep.x === "min" ? f.x0 : (f.x0 + f.x1) / 2,
      y: keep.y === "max" ? f.y1 : keep.y === "min" ? f.y0 : (f.y0 + f.y1) / 2,
    },
    // A single-letter handle is an edge: the ratio must follow that edge.
    drive: name.length > 1 ? "dominant" : name === "n" || name === "s" ? "h" : "w",
  };
}

stage.addEventListener("pointerdown", (ev) => {
  if (finished || ev.button !== 0) return;
  if (spaceHeld) {
    // Panning wins over selecting. Handled here rather than in a capture-phase
    // listener: when the stage is itself the target, capture and bubble
    // listeners on it fire in registration order, so a separate listener
    // could not reliably pre-empt this one.
    panning = { x: ev.clientX, y: ev.clientY, l: stage.scrollLeft, t: stage.scrollTop };
    drag = null;
    stage.classList.add("grabbing");
    stage.setPointerCapture(ev.pointerId);
    ev.preventDefault();
    return;
  }
  const handle = ev.target.closest && ev.target.closest(".h");
  const p = pointerToSource(ev);
  if (handle) {
    drag = resizeDrag(handle.dataset.h);
  } else {
    const inside =
      p.x >= sel.x && p.x <= sel.x + sel.w && p.y >= sel.y && p.y <= sel.y + sel.h;
    drag = inside
      ? { mode: "move", ox: p.x - sel.x, oy: p.y - sel.y }
      : {
          mode: "new",
          ax: p.x,
          ay: p.y,
          keep: { x: "min", y: "min" },
          fix: { x: p.x, y: p.y },
          drive: "dominant",
          shift: ev.shiftKey,
          // The ratio a shift-drag constrains to: whatever the box is now.
          baseRatio: sel.h ? sel.w / sel.h : 1,
        };
  }
  stage.setPointerCapture(ev.pointerId);
  ev.preventDefault();
});

stage.addEventListener("pointermove", (ev) => {
  if (panning) {
    stage.scrollLeft = panning.l - (ev.clientX - panning.x);
    stage.scrollTop = panning.t - (ev.clientY - panning.y);
    return;
  }
  if (!drag) return;
  const p = pointerToSource(ev);
  if (drag.mode === "resize") {
    const f = drag.fixed;
    let { x0, y0, x1, y1 } = f;
    if (drag.h.includes("n")) y0 = p.y;
    if (drag.h.includes("s")) y1 = p.y;
    if (drag.h.includes("w")) x0 = p.x;
    if (drag.h.includes("e")) x1 = p.x;
    sel.x = Math.min(x0, x1);
    sel.y = Math.min(y0, y1);
    sel.w = Math.abs(x1 - x0);
    sel.h = Math.abs(y1 - y0);
  } else if (drag.mode === "move") {
    sel.x = p.x - drag.ox;
    sel.y = p.y - drag.oy;
  } else {
    sel.x = Math.min(drag.ax, p.x);
    sel.y = Math.min(drag.ay, p.y);
    sel.w = Math.abs(p.x - drag.ax);
    sel.h = Math.abs(p.y - drag.ay);
    // Pin whichever corner the box is being pulled away from.
    drag.keep.x = p.x < drag.ax ? "max" : "min";
    drag.keep.y = p.y < drag.ay ? "max" : "min";
    drag.shift = ev.shiftKey;
  }
  render();
});

stage.addEventListener("pointerup", (ev) => {
  if (panning) {
    panning = null;
    stage.classList.remove("grabbing");
    stage.releasePointerCapture(ev.pointerId);
    return;
  }
  if (!drag) return;
  drag = null;
  stage.releasePointerCapture(ev.pointerId);
  settle();
});

/** Called whenever the selection stops changing. */
function settle() { render(); }

async function send(path) {
  if (finished) return;
  finished = true;
  document.body.classList.add("done");
  await fetch(path + location.search, {
    method: "POST",
    body: JSON.stringify({ x: sel.x, y: sel.y, w: sel.w, h: sel.h }),
  });
  $("hint").firstChild.textContent = " done — back to your terminal ";
}

$("crop").addEventListener("click", () => send("/crop"));
$("cancel").addEventListener("click", () => send("/cancel"));

let nudgeTimer = null;
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Enter") { send("/crop"); ev.preventDefault(); return; }
  if (ev.key === "Escape") { send("/cancel"); ev.preventDefault(); return; }

  const step = ev.shiftKey ? 10 : 1;
  const resize = ev.altKey;              // alt+arrows resize instead of move
  let dx = 0, dy = 0;
  if (ev.key === "ArrowLeft") dx = -step;
  else if (ev.key === "ArrowRight") dx = step;
  else if (ev.key === "ArrowUp") dy = -step;
  else if (ev.key === "ArrowDown") dy = step;
  else return;

  if (resize) { sel.w += dx; sel.h += dy; }
  else { sel.x += dx; sel.y += dy; }
  render();
  clearTimeout(nudgeTimer);
  nudgeTimer = setTimeout(settle, 300);
  ev.preventDefault();
});

// Closing the tab must not leave the CLI waiting for the 10-minute timeout.
window.addEventListener("pagehide", () => {
  if (finished) return;
  finished = true;
  navigator.sendBeacon("/cancel" + location.search, "{}");
});

/**
 * Switch the sticky ratio lock. A spec that matches no button (a `--crop 3:2`
 * seed, say) leaves every preset unpressed, which is honest: the lock is on,
 * but it isn't one of the six.
 */
function setRatio(spec) {
  if (spec === "free") lock = null;
  else if (spec === "orig") lock = CFG.source_w / CFG.source_h;
  else {
    const [w, h] = spec.split(":").map(Number);
    lock = w / h;
  }
  for (const b of document.querySelectorAll("#ratios button"))
    b.setAttribute("aria-pressed", String(b.dataset.r === spec));
  render();
  settle();
}
for (const b of document.querySelectorAll("#ratios button"))
  b.addEventListener("click", () => setRatio(b.dataset.r));
setRatio(CFG.lock ? `${CFG.lock[0]}:${CFG.lock[1]}` : "free");

function applyZoom() {
  if (zoom === null) {
    stage.classList.remove("zoomed");
    img.style.width = "";
  } else {
    stage.classList.add("zoomed");
    img.style.width = CFG.preview_w * zoom + "px";
  }
  render();
}

/**
 * Zoom while keeping the source pixel under (clientX, clientY) under it. Zoom
 * that drifts is useless for the thing zoom is for: placing an edge exactly.
 */
function zoomTo(next, clientX, clientY) {
  const at = pointerToSource({ clientX, clientY });
  zoom = next === null ? null : Math.min(4, Math.max(0.05, next));
  applyZoom();
  if (zoom === null) return;
  const i = img.getBoundingClientRect();
  stage.scrollLeft += i.left + s2p(at.x) - clientX;
  stage.scrollTop += i.top + s2p(at.y) - clientY;
  render();
}

for (const b of document.querySelectorAll("#zoom button")) {
  b.addEventListener("click", () => {
    for (const o of document.querySelectorAll("#zoom button"))
      o.setAttribute("aria-pressed", String(o === b));
    // Zoom on the stage's centre when it comes from a button, not the cursor.
    const s = stage.getBoundingClientRect();
    zoomTo(b.dataset.z === "fit" ? null : Number(b.dataset.z),
           s.left + stage.clientWidth / 2, s.top + stage.clientHeight / 2);
  });
}

// The page opens fitted, so say so — the ratio row shows its state too.
for (const b of document.querySelectorAll("#zoom button"))
  b.setAttribute("aria-pressed", String(b.dataset.z === "fit"));

stage.addEventListener("wheel", (ev) => {
  // Only ctrl/⌘+wheel zooms — a plain wheel has to keep scrolling a zoomed
  // image, or there is no way to reach its edges.
  if (!ev.ctrlKey && !ev.metaKey) return;
  const base = zoom === null ? img.clientWidth / CFG.preview_w : zoom;
  zoomTo(base * (ev.deltaY < 0 ? 1.1 : 1 / 1.1), ev.clientX, ev.clientY);
  ev.preventDefault();
}, { passive: false });

// Space is a modifier here, not a key press: hold to pan, release to select.
document.addEventListener("keydown", (ev) => {
  if (ev.code === "Space" && !spaceHeld) {
    spaceHeld = true;
    stage.classList.add("panning");
    ev.preventDefault();
  }
});
document.addEventListener("keyup", (ev) => {
  if (ev.code === "Space") {
    spaceHeld = false;
    panning = null;
    stage.classList.remove("panning", "grabbing");
  }
});

img.addEventListener("load", render);
window.addEventListener("resize", render);
