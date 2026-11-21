// Source installations share the same profiles on every published branch.
window.ANGI_INSTALL_GUIDE = function ({guide, tabs, images, esc, icon, bindCopy}) {
  if (!guide || !tabs) return;

  const profiles = window.ANGI_INSTALL_PROFILES;
  const targets = Object.keys(profiles);
  const legacy = {'x86-desktop': 'intel'};
  const art = (profile) => images.find((image) => image.id === profile.board)?.icon;
  const idForHash = () => {
    const id = location.hash.slice(1);
    return targets.includes(id) ? id : legacy[id] || 'rpi5';
  };

  function select(id, focusTab) {
    const profile = profiles[id];
    const modes = Object.entries(profile.modes);
    tabs.querySelectorAll('[role="tab"]').forEach((tab) => {
      const selected = tab.dataset.id === id;
      tab.setAttribute('aria-selected', selected);
      tab.tabIndex = selected ? 0 : -1;
      if (selected && focusTab) tab.focus();
    });
    guide.setAttribute('aria-labelledby', 'tab-' + id);
    guide.innerHTML = `
      ${profile.status === 'experimental' ? '<p class="bring">Experimental target. Your agent will ask before building.</p>' : ''}
      ${modes.length === 1 ? `<p class="bring">${esc(modes[0][1].label)}</p>` : `
        <p class="bring"><label for="install-medium">Install using</label>
        <select id="install-medium">${modes.map(([method, mode]) =>
          `<option value="${method}">${esc(mode.label)}</option>`).join('')}</select></p>`}
      <p class="bring" id="install-method-status" role="status" hidden></p>
      <div class="prompt panel">
        <div class="prompt-bar">
          <div><h3>${esc(profile.name)}</h3><p>Paste this prompt into your coding agent.</p></div>
          <button class="btn primary" type="button" data-copy="prompt-text">${icon('i-copy')} Copy prompt</button>
        </div>
        <pre id="prompt-text" tabindex="0"></pre>
        <button class="more" type="button" aria-expanded="false" aria-controls="prompt-text">Show the whole prompt</button>
      </div>`;
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
    history.replaceState(null, '', '#' + id);
  }

  tabs.innerHTML = targets.map((id) => {
    const profile = profiles[id];
    const symbol = art(profile);
    return `<button type="button" role="tab" id="tab-${id}" data-id="${id}" aria-controls="guide" aria-selected="false">
      ${symbol ? `<svg aria-hidden="true"><use href="#${symbol}"/></svg>` : ''}${esc(profile.short)}
    </button>`;
  }).join('');
  tabs.addEventListener('click', (event) => {
    const tab = event.target.closest('[role="tab"]');
    if (tab) select(tab.dataset.id);
  });
  tabs.addEventListener('keydown', (event) => {
    const current = targets.indexOf(document.activeElement.dataset.id);
    if (current < 0) return;
    let next;
    switch (event.key) {
      case 'ArrowRight': next = (current + 1) % targets.length; break;
      case 'ArrowLeft': next = (current + targets.length - 1) % targets.length; break;
      case 'Home': next = 0; break;
      case 'End': next = targets.length - 1; break;
      default: return;
    }
    event.preventDefault();
    select(targets[next], true);
  });
  addEventListener('hashchange', () => select(idForHash()));
  select(idForHash());
};
