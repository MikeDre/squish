'use strict';

/* ---------- sidebar scrollspy ---------- */

const tocLinks = [...document.querySelectorAll('.docs-sidebar .toc a')];
const sections = tocLinks
  .map((link) => document.querySelector(link.getAttribute('href')))
  .filter(Boolean);

const setActive = (id) => {
  tocLinks.forEach((link) => {
    link.classList.toggle('is-active', link.getAttribute('href') === `#${id}`);
  });
};

const spyObserver = new IntersectionObserver(
  (entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) setActive(entry.target.id);
    });
  },
  { rootMargin: '-15% 0px -75% 0px' },
);

sections.forEach((section) => spyObserver.observe(section));

/* ---------- mobile TOC: collapse after a jump ---------- */

const mobileToc = document.querySelector('.docs-toc-mobile');
if (mobileToc) {
  mobileToc.querySelectorAll('a').forEach((link) => {
    link.addEventListener('click', () => mobileToc.removeAttribute('open'));
  });
}
