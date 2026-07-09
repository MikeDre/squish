'use strict';

const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/* ---------- the squishable wordmark ---------- */

const wordmark = document.querySelector('.wordmark');

const squish = () => wordmark.classList.add('is-squished');
const unsquish = () => wordmark.classList.remove('is-squished');

wordmark.addEventListener('pointerdown', squish);
wordmark.addEventListener('pointerup', unsquish);
wordmark.addEventListener('pointerleave', unsquish);
wordmark.addEventListener('pointercancel', unsquish);
wordmark.addEventListener('keydown', (e) => {
  if (e.key === ' ' || e.key === 'Enter') squish();
});
wordmark.addEventListener('keyup', (e) => {
  if (e.key === ' ' || e.key === 'Enter') unsquish();
});
wordmark.addEventListener('blur', unsquish);

/* ---------- terminal playback ---------- */

const termBody = document.getElementById('demo-terminal');
const termLines = [...termBody.querySelectorAll('.term-line')];
const typedTarget = termBody.querySelector('.t-typed');
const commandText = typedTarget.textContent;
let playToken = 0;

const playDemo = () => {
  if (prefersReducedMotion) return;
  const token = ++playToken;
  const stillMe = () => token === playToken;

  termLines.forEach((line) => {
    line.classList.add('is-hidden');
    line.classList.remove('is-revealed');
  });
  termLines[0].classList.remove('is-hidden');
  typedTarget.textContent = '';
  termBody.classList.add('is-typing');

  let i = 0;
  const typeNext = () => {
    if (!stillMe()) return;
    typedTarget.textContent = commandText.slice(0, ++i);
    if (i < commandText.length) {
      setTimeout(typeNext, 40 + Math.random() * 45);
    } else {
      setTimeout(revealOutput, 450);
    }
  };

  const revealOutput = () => {
    if (!stillMe()) return;
    termBody.classList.remove('is-typing');
    termLines.slice(1).forEach((line, idx) => {
      const isSummary = line.classList.contains('t-summary');
      setTimeout(() => {
        if (!stillMe()) return;
        line.classList.remove('is-hidden');
        line.classList.add('is-revealed');
      }, idx * 130 + (isSummary ? 320 : 0));
    });
  };

  setTimeout(typeNext, 350);
};

const terminalObserver = new IntersectionObserver(
  (entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        playDemo();
        terminalObserver.unobserve(entry.target);
      }
    });
  },
  { threshold: 0.4 },
);

terminalObserver.observe(termBody);
document.querySelector('.term-replay').addEventListener('click', playDemo);

/* ---------- results chart ---------- */

const chart = document.querySelector('.chart');

if (!prefersReducedMotion) {
  chart.classList.add('is-primed');

  const chartObserver = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          requestAnimationFrame(() =>
            requestAnimationFrame(() => chart.classList.add('is-squishing')),
          );
          chartObserver.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.35 },
  );

  chartObserver.observe(chart);
}

/* ---------- install tabs ---------- */

const tabs = [...document.querySelectorAll('.tab')];

const selectTab = (tab) => {
  tabs.forEach((t) => {
    const active = t === tab;
    t.classList.toggle('is-active', active);
    t.setAttribute('aria-selected', String(active));
    t.tabIndex = active ? 0 : -1;
    document.getElementById(t.getAttribute('aria-controls')).hidden = !active;
  });
  tab.focus();
};

tabs.forEach((tab, idx) => {
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowRight') selectTab(tabs[(idx + 1) % tabs.length]);
    if (e.key === 'ArrowLeft') selectTab(tabs[(idx - 1 + tabs.length) % tabs.length]);
  });
});
