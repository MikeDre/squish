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

/** Selection position relative to the stage, in CSS px. */
function frame() {
  const i = img.getBoundingClientRect(), s = stage.getBoundingClientRect();
  return { left: i.left - s.left, top: i.top - s.top, w: i.width, h: i.height };
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

  // Drive from whichever axis the user has pulled further, so the box follows
  // the pointer instead of collapsing when the drag is mostly vertical.
  let w = sel.w, h = sel.h;
  if (h <= 0 || w / h > ratio) h = w / ratio;
  else w = h * ratio;
  if (h > CFG.source_h) { h = CFG.source_h; w = h * ratio; }
  if (w > CFG.source_w) { w = CFG.source_w; h = w / ratio; }
  w = Math.max(1, Math.round(w));
  h = Math.max(1, Math.round(h));

  if (drag && (drag.mode === "new" || drag.mode === "resize") && drag.anchor) {
    // Pin the anchor corner and grow away from it, in the drag's direction.
    sel.x = drag.dx < 0 ? drag.anchor.x - w : drag.anchor.x;
    sel.y = drag.dy < 0 ? drag.anchor.y - h : drag.anchor.y;
  } else {
    sel.x = Math.round(sel.x + sel.w / 2 - w / 2);
    sel.y = Math.round(sel.y + sel.h / 2 - h / 2);
  }
  sel.w = w;
  sel.h = h;
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
  const left = f.left + s2p(sel.x);
  const bottom = f.top + s2p(sel.y) + s2p(sel.h);
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
stage.addEventListener("pointerdown", (ev) => {
  if (finished || ev.button !== 0) return;
  const p = pointerToSource(ev);
  const inside =
    p.x >= sel.x && p.x <= sel.x + sel.w && p.y >= sel.y && p.y <= sel.y + sel.h;
  drag = inside
    ? { mode: "move", ox: p.x - sel.x, oy: p.y - sel.y }
    : {
        mode: "new",
        ax: p.x,
        ay: p.y,
        anchor: { x: p.x, y: p.y },
        dx: 0,
        dy: 0,
        shift: ev.shiftKey,
        // The ratio a shift-drag constrains to: whatever the box is now.
        baseRatio: sel.h ? sel.w / sel.h : 1,
      };
  stage.setPointerCapture(ev.pointerId);
  ev.preventDefault();
});

stage.addEventListener("pointermove", (ev) => {
  if (!drag) return;
  const p = pointerToSource(ev);
  if (drag.mode === "move") {
    sel.x = p.x - drag.ox;
    sel.y = p.y - drag.oy;
  } else {
    sel.x = Math.min(drag.ax, p.x);
    sel.y = Math.min(drag.ay, p.y);
    sel.w = Math.abs(p.x - drag.ax);
    sel.h = Math.abs(p.y - drag.ay);
    // Which way the box is being pulled, so a locked drag grows the right way.
    drag.dx = p.x - drag.ax;
    drag.dy = p.y - drag.ay;
    drag.shift = ev.shiftKey;
  }
  render();
});

stage.addEventListener("pointerup", (ev) => {
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

document.addEventListener("keydown", (ev) => {
  if (ev.key === "Enter") { send("/crop"); ev.preventDefault(); }
  else if (ev.key === "Escape") { send("/cancel"); ev.preventDefault(); }
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

img.addEventListener("load", render);
window.addEventListener("resize", render);
