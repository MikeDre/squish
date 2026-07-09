'use strict';

/* ---------- theme toggle ---------- */

const root = document.documentElement;
const themeToggle = document.querySelector('.theme-toggle');
const systemDark = window.matchMedia('(prefers-color-scheme: dark)');

const effectiveTheme = () =>
  root.dataset.theme || (systemDark.matches ? 'dark' : 'light');

const syncToggleLabel = () => {
  const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
  themeToggle.setAttribute('aria-label', `Switch to ${next} mode`);
};

const savedTheme = localStorage.getItem('squish-theme');
if (savedTheme === 'light' || savedTheme === 'dark') {
  root.dataset.theme = savedTheme;
}

if (themeToggle) {
  syncToggleLabel();
  themeToggle.addEventListener('click', () => {
    const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
    root.dataset.theme = next;
    localStorage.setItem('squish-theme', next);
    syncToggleLabel();
  });
  systemDark.addEventListener('change', syncToggleLabel);
}

/* ---------- copy buttons ---------- */

const copyText = async (text) => {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const scratch = document.createElement('textarea');
    scratch.value = text;
    document.body.appendChild(scratch);
    scratch.select();
    document.execCommand('copy');
    scratch.remove();
  }
};

document.querySelectorAll('.copy-btn').forEach((btn) => {
  btn.addEventListener('click', async () => {
    await copyText(btn.dataset.copy);
    btn.classList.add('is-copied');
    setTimeout(() => btn.classList.remove('is-copied'), 1400);
  });
});

/* ---------- scroll reveals ---------- */

const revealObserver = new IntersectionObserver(
  (entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.classList.add('in-view');
        revealObserver.unobserve(entry.target);
      }
    });
  },
  { threshold: 0.15 },
);

document.querySelectorAll('[data-reveal]').forEach((el) => revealObserver.observe(el));
