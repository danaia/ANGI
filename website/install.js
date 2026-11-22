// Source installations share the same profiles on every published branch.
window.ANGI_INSTALL_GUIDE = function ({guide, hardware, shortcuts, images, esc, icon, bindCopy}) {
  if (!guide || !hardware || !shortcuts) return;

  const profiles = window.ANGI_INSTALL_PROFILES;
  const piTargets = ['rpizero2w', 'rpi4', 'rpi5'];
  const targets = [...piTargets, ...Object.keys(profiles).filter((id) => !piTargets.includes(id))];
  const imageFor = (profile) => images.find((image) => image.id === profile.board);
  const idForHash = () => {
    const id = location.hash.slice(1).replace(/^dl-/, '');
    if (id === 'x86-desktop') return 'intel';
    return targets.includes(id) ? id : null;
  };

  function select(row) {
    const id = row.dataset.id;
    history.replaceState(null, '', '#' + id);
    for (const link of shortcuts.querySelectorAll('a')) {
      if (link.hash === '#' + id) link.setAttribute('aria-current', 'location');
      else link.removeAttribute('aria-current');
    }
    if (guide.parentElement === row) return;
    const profile = profiles[id];
    const modes = Object.entries(profile.modes);
    const image = imageFor(profile);
    guide.innerHTML = `
      ${profile.status === 'experimental' ? '<p class="bring">Experimental target. Your agent will ask before building.</p>' : ''}
      ${modes.length === 1 ? `<p class="bring">${esc(modes[0][1].label)}</p>` : `
        <p class="bring"><label for="install-medium">Install using</label>
        <select id="install-medium">${modes.map(([method, mode]) =>
          `<option value="${method}">${esc(mode.label)}</option>`).join('')}</select></p>`}
      <p class="bring" id="install-method-status" role="status" hidden></p>
      <div class="prompt panel">
        <div class="prompt-bar">
          <p>Paste this prompt into your coding agent.</p>
          <button class="btn primary" type="button" data-copy="prompt-text">${icon('i-copy')} Copy prompt</button>
        </div>
        <pre id="prompt-text" tabindex="0"></pre>
        <button class="more" type="button" aria-expanded="false" aria-controls="prompt-text">Show the whole prompt</button>
      </div>
      ${image?.status === 'available' ? `
        <div class="release-image">
          <h3>Published image</h3>
          <p>${esc(image.file)}${image.size ? ` · ${esc(image.size)}` : ''} · ${esc(image.media)}</p>
          <p>SHA-256 ${esc(image.sha256 || 'is listed in the matching checksum file.')}</p>
          <a class="link" href="${esc(image.url || `${window.ANGI_DOWNLOADS}/${image.file}`)}">Download image</a>
        </div>` : ''}`;
    const prompt = guide.querySelector('#prompt-text');
    const medium = guide.querySelector('#install-medium');
    const update = () => {
      const mode = medium?.value || modes[0][0];
      const status = guide.querySelector('#install-method-status');
      status.hidden = !profile.modes[mode].experimental;
      status.textContent = status.hidden ? '' : 'Experimental install method. Hardware verification is pending.';
      prompt.textContent = window.ANGI_PROMPT(id, mode);
    };
    update();
    medium?.addEventListener('change', update);
    const more = guide.querySelector('.more');
    const box = guide.querySelector('.prompt');
    more.addEventListener('click', () => {
      const open = box.classList.toggle('open');
      more.setAttribute('aria-expanded', open);
      more.textContent = open ? 'Collapse' : 'Show the whole prompt';
    });
    bindCopy(guide);
    row.append(guide);
    guide.hidden = false;
  }

  hardware.innerHTML = targets.map((id) => {
    const profile = profiles[id];
    const image = imageFor(profile);
    const media = profile.modes.sd && profile.modes.usb ? 'microSD card or USB storage'
      : profile.modes.sd ? 'microSD card' : 'USB stick or SD card in a USB reader';
    return `<details class="install-row panel" name="install-hardware" id="${id}" data-id="${id}">
      <summary aria-label="Install prompt for ${esc(profile.name)}">
        <svg class="board" aria-hidden="true"><use href="#${esc(image.icon)}"/></svg>
        <span class="install-row-copy">
          <span class="install-row-title">${esc(profile.name)}</span>
          <span>${esc(image.blurb)}</span>
          <span class="install-row-media">Boot media <span>${media}</span></span>
        </span>
        <span class="btn" aria-hidden="true">${icon('i-chat')} <span class="install-row-open">Install prompt</span><span class="install-row-close">Hide prompt</span></span>
      </summary>
    </details>`;
  }).join('');
  shortcuts.innerHTML = targets.map((id) => {
    const profile = profiles[id];
    const symbol = imageFor(profile)?.icon;
    return `<a href="#${id}">
      ${symbol ? `<svg aria-hidden="true"><use href="#${esc(symbol)}"/></svg>` : ''}${esc(profile.short)}
    </a>`;
  }).join('');
  shortcuts.hidden = false;
  const rows = [...hardware.querySelectorAll('details')];
  const scrollToRow = (row) => row.scrollIntoView({block: 'start'});
  rows.forEach((row) => row.addEventListener('toggle', () => {
    if (!row.open) return;
    select(row);
    scrollToRow(row);
  }));
  function openRow(id, moveFocus = false) {
    const row = rows.find((item) => item.dataset.id === id);
    if (!row) return;
    if (moveFocus) row.querySelector('summary').focus({preventScroll: true});
    if (row.open) scrollToRow(row);
    else row.open = true;
  }
  shortcuts.addEventListener('click', (event) => {
    const link = event.target.closest('a');
    if (!link || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    openRow(link.hash.slice(1), true);
  });
  addEventListener('hashchange', () => openRow(idForHash()));
  openRow(idForHash());
};
