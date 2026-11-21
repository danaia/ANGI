// Shared behaviour for every ANGI page: navigation, reveal-on-scroll, copy
// buttons, and the image cards, image list and install guide rendered from
// images.js (with prompts.js for the agent install prompt).
(function () {
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const esc = (t) => String(t).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const icon = (id) => `<svg class="i" aria-hidden="true"><use href="#${id}"/></svg>`;
  const images = window.ANGI_IMAGES || [];
  const releases = window.ANGI_RELEASES || 'https://github.com/danaia/ANGI/releases';
  const published = (img) => img.status === 'available';
  const imageUrl = (img) => img.url || `${window.ANGI_DOWNLOADS}/${img.file}`;

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
          <a class="btn sm" href="get-started.html#${img.id}">${icon('i-chat')} Install prompt</a>
        </div>
      </article>`).join('');
  });

  /* image list (images page) */
  const list = $('#dl-list');
  if (list) {
    list.innerHTML = images.map((img) => `
      <article class="dl-row panel reveal" id="dl-${img.id}">
        <svg class="board" role="img" aria-label="${esc(img.name)}"><use href="#${img.icon}"/></svg>
        <div>
          <h3>${esc(img.name)}</h3>
          <p>${esc(img.blurb)}</p>
          <dl>
            <dt>Written to</dt><dd>${esc(img.media)}</dd>
            ${published(img) ? `<dt>File</dt><dd>${esc(img.file)}${img.size ? ' · ' + esc(img.size) : ''}</dd>
            <dt>SHA-256</dt><dd>${img.sha256 ? esc(img.sha256) : `${esc(img.file)}.sha256, next to the image`}</dd>` : ''}
          </dl>
        </div>
        <div class="acts">
          <a class="btn" href="get-started.html#${img.id}">${icon('i-chat')} Install prompt</a>
          ${published(img) ? `<a class="btn ghost" href="${esc(imageUrl(img))}">${icon('i-download')} Image file</a>` : ''}
        </div>
      </article>`).join('');
  }
  $$('[data-releases]').forEach((a) => { a.href = releases; });

  /* install guide */
  const guide = $('#guide');
  const tabs = $('#tabs');
  const who = (label, items) => `<div class="who"><h4>${label}</h4><ul>${items.map((t) => `<li>${t}</li>`).join('')}</ul></div>`;
  function phases(img) {
    const pi = img.kind === 'pi';
    const m = esc(img.media);
    const one = `
      <div class="phase panel">
        <div class="phase-tag">Phase 1 · ${pi ? 'card' : 'drive'} in your computer</div>
        <div class="scene" aria-hidden="true">
          <span class="chip">${icon('i-laptop')}Your computer</span><span class="wire-s"></span><span class="chip hot">${icon('i-card')}${m}</span>
        </div>
        ${who('Your agent', [
          'Checks your computer and finds the card reader',
          'Finds this computer\'s SSH key, or creates one',
          `Builds ANGI for the ${esc(img.short)} and verifies the image`,
          `Shows you the ${m}: path, size, model, contents`,
          'Writes it, checks every byte, and ejects it',
        ])}
        ${who('You', [
          `Put the ${m} in ${pi ? 'a card reader on your computer' : 'your computer'}`,
          'Type <code>erase /dev/…</code> to approve. Nothing is erased before that.',
        ])}
      </div>`;
    const two = pi ? `
      <div class="phase panel hot">
        <div class="phase-tag">Phase 2 · card in the ${esc(img.short)}</div>
        <div class="scene" aria-hidden="true">
          <span class="chip">${icon('i-laptop')}Your computer</span><span class="wire-s">${img.id === 'rpizero2w' ? 'USB' : 'USB-C'}</span><span class="chip hot"><svg class="b" aria-hidden="true"><use href="#${img.icon}"/></svg>${esc(img.short)}</span>
        </div>
        ${who('You', [
          `Move the card into the ${esc(img.short)}`,
          img.id === 'rpizero2w' ? 'Connect a data cable to the micro-USB port marked USB'
            : 'Connect a USB data cable, or use direct Ethernet and the Pi’s power supply',
          'Create your account and join Wi-Fi in the ANGI terminal dashboard',
        ])}
        ${who('Your agent', [
          'Verifies the Pi’s identity and connects over SSH',
          'Opens the ANGI terminal dashboard and verifies Wi-Fi and internet',
        ])}
      </div>` : `
      <div class="phase panel hot">
        <div class="phase-tag">Phase 2 · drive in the ${esc(img.short)} computer</div>
        <div class="scene" aria-hidden="true">
          <span class="chip">${icon('i-card')}USB drive</span><span class="wire-s"></span><span class="chip hot"><svg class="b" aria-hidden="true"><use href="#${img.icon}"/></svg>${esc(img.short)}</span>
        </div>
        ${who('You', [
          `Move the drive into the ${esc(img.short)} computer`,
          'Start it from the drive using its boot menu',
          'Paste the <b>SSH public key</b> your agent gives you when setup asks',
        ])}
        ${who('Your agent', [
          'Helps you find the boot menu key',
          'Walks you through the setup steps for this image',
          'Pins the computer\'s identity for SSH over Ethernet',
        ])}
      </div>`;
    return `<div class="phases">${one}<div class="move" aria-hidden="true">${icon('i-arrow')}<span>Move the ${pi ? 'card' : 'drive'}</span></div>${two}</div>`;
  }
  function select(id, focusTab) {
    const img = images.find((i) => i.id === id) || images[0];
    if (!img || !guide) return;
    $$('button', tabs).forEach((b) => {
      const on = b.dataset.id === img.id;
      b.setAttribute('aria-selected', on);
      b.tabIndex = on ? 0 : -1;
      if (on && focusTab) b.focus();
    });
    const pi = img.kind === 'pi';
    const status = window.ANGI_BUILD_STATUS[img.id];
    const planned = status === 'planned';
    const notice = planned ? 'Build support is planned. The prompt checks the current repository for updates.'
      : status === 'experimental' ? 'Experimental target. The agent will ask before building.' : '';
    guide.setAttribute('aria-labelledby', 'tab-' + img.id);
    guide.innerHTML = `
      ${notice ? `<p class="bring">${notice}</p>` : ''}
      ${planned ? '' : `<div class="need">
        <div class="panel">${icon('i-download')}<p><b>Your agent does the work</b>Builds the image, checks it and writes your ${esc(img.media)}.</p></div>
        <div class="panel">${icon('i-chat')}<p><b>It tells you what to do</b>Every physical step, the moment you need to take it.</p></div>
        <div class="panel">${icon('i-check')}<p><b>It sees it through</b>Step by step until ANGI is running${pi ? ' and online' : ''}.</p></div>
      </div>
      <p class="bring"><b>You bring</b> a coding agent with access to a Linux build environment, a ${esc(img.media)} you can erase${pi ? ', a data cable' : ''} and your Wi-Fi details.</p>`}
      <div class="prompt panel">
        <div class="prompt-bar">
          <div><h3>Install prompt for ${esc(img.name)}</h3><p>Paste it into your agent's chat.</p></div>
          <button class="btn primary" type="button" data-copy="prompt-text">${icon('i-copy')} Copy prompt</button>
        </div>
        <pre id="prompt-text" tabindex="0"></pre>
        <button class="more" type="button" aria-expanded="false">Show the whole prompt</button>
      </div>
      ${planned ? '' : `<h2 class="phases-title"><span class="grad">What to expect</span></h2>
      <p class="lead" style="margin-bottom:28px">Your agent tells you when to move the ${pi ? 'card' : 'drive'}.</p>
      ${phases(img)}`}`;
    $('#prompt-text').textContent = window.ANGI_PROMPT(img);
    const more = $('.more', guide), box = $('.prompt', guide);
    more.addEventListener('click', () => {
      const open = box.classList.toggle('open');
      more.setAttribute('aria-expanded', open);
      more.textContent = open ? 'Collapse' : 'Show the whole prompt';
    });
    bindCopy(guide);
    if (history.replaceState) history.replaceState(null, '', '#' + img.id);
  }
  if (guide && tabs) {
    tabs.innerHTML = images.map((img) => `
      <button type="button" role="tab" id="tab-${img.id}" data-id="${img.id}" aria-controls="guide" aria-selected="false">
        <svg aria-hidden="true"><use href="#${img.icon}"/></svg>${esc(img.short)}
      </button>`).join('');
    tabs.addEventListener('click', (e) => { const b = e.target.closest('button'); b && select(b.dataset.id); });
    tabs.addEventListener('keydown', (e) => {
      const keys = { ArrowRight: 1, ArrowLeft: -1 };
      if (!(e.key in keys)) return;
      const ids = images.map((i) => i.id);
      const cur = ids.indexOf(document.activeElement.dataset.id);
      select(ids[(cur + keys[e.key] + ids.length) % ids.length], true);
      e.preventDefault();
    });
    const pick = () => select(location.hash.slice(1));
    addEventListener('hashchange', pick);
    pick();
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
