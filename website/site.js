// Shared behaviour for every ANGI page: navigation, reveal-on-scroll, copy
// buttons, and the image cards and install guide rendered from
// images.js (with prompts.js for the agent install prompt).
(function () {
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const esc = (t) => String(t).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const icon = (id) => `<svg class="i" aria-hidden="true"><use href="#${id}"/></svg>`;
  const images = window.ANGI_IMAGES || [];
  const releases = window.ANGI_RELEASES || 'https://github.com/danaia/ANGI/releases';

  /* navigation */
  const nav = $('#nav');
  if (nav) {
    const onScroll = () => nav.classList.toggle('s', scrollY > 16);
    onScroll();
    addEventListener('scroll', onScroll, { passive: true });
    const menu = $('.menu', nav);
    menu && menu.addEventListener('click', () => {
      const open = nav.classList.toggle('open');
      menu.setAttribute('aria-expanded', open);
    });
    $$('.links a', nav).forEach((a) => a.addEventListener('click', () => nav.classList.remove('open')));
  }

  /* image cards (home) */
  $$('[data-images]').forEach((el) => {
    el.innerHTML = images.map((img) => `
      <article class="img-card panel reveal">
        <svg class="board" role="img" aria-label="${esc(img.name)}"><use href="#${img.icon}"/></svg>
        <div class="txt">
          <h3>${esc(img.short)}</h3>
          <p>${esc(img.blurb)}</p>
        </div>
        <div class="acts">
          <a class="btn sm" href="download.html#${img.id}">${icon('i-chat')} Install prompt</a>
        </div>
      </article>`).join('');
  });

  $$('[data-releases]').forEach((a) => { a.href = releases; });

  window.ANGI_INSTALL_GUIDE?.({
    guide: $('#guide'), hardware: $('#install-hardware'), shortcuts: $('#install-targets'),
    images, esc, icon, bindCopy,
  });

  const baseLink = $('.base-link');
  const baseDialog = $('#bare-minimum');
  if (baseLink && baseDialog) {
    const openFromHash = () => {
      if (location.hash === baseLink.hash && !baseDialog.open) baseDialog.showModal();
    };
    baseLink.addEventListener('click', (event) => {
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      baseDialog.showModal();
    });
    baseDialog.addEventListener('click', (event) => {
      if (event.target === baseDialog) baseDialog.close();
    });
    baseDialog.addEventListener('close', () => baseLink.focus());
    addEventListener('hashchange', openFromHash);
    openFromHash();
  }

  /* copy buttons */
  function bindCopy(root) {
    $$('[data-copy]', root).forEach((b) => {
      if (b.dataset.bound) return;
      b.dataset.bound = '1';
      const label = b.innerHTML;
      b.addEventListener('click', async () => {
        const text = $('#' + b.dataset.copy).textContent;
        try { await navigator.clipboard.writeText(text); b.textContent = 'Copied'; } catch (e) { b.textContent = 'Select and copy'; }
        setTimeout(() => { b.innerHTML = label; }, 1800);
      });
    });
  }
  bindCopy(document.querySelector('main') || document);

  /* reveal on scroll */
  const io = 'IntersectionObserver' in window && new IntersectionObserver((es) => es.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
  }), { threshold: 0.08 });
  $$('.reveal').forEach((el, i) => {
    if (!io || el.getBoundingClientRect().top < innerHeight) { el.classList.add('in'); return; }
    el.style.transitionDelay = (i % 4) * 60 + 'ms';
    io.observe(el);
  });
})();
