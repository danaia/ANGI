/* @ds-bundle: {"format":4,"namespace":"PebbleLine","components":[{"name":"Button"},{"name":"IconButton"},{"name":"TextField"},{"name":"TextArea"},{"name":"Select"},{"name":"Checkbox"},{"name":"RadioGroup"},{"name":"Switch"},{"name":"Slider"},{"name":"RangeSlider"},{"name":"SegmentedControl"},{"name":"Tabs"},{"name":"Breadcrumbs"},{"name":"Pagination"},{"name":"NavList"},{"name":"NavRail"},{"name":"Card"},{"name":"Divider"},{"name":"BentoGrid"},{"name":"BentoCell"},{"name":"StatTile"},{"name":"SummaryList"},{"name":"Dialog"},{"name":"Menu"},{"name":"Tooltip"},{"name":"Alert"},{"name":"Toast"},{"name":"Badge"},{"name":"Progress"},{"name":"ProgressRing"},{"name":"Avatar"},{"name":"Table"},{"name":"Icon"}]} */
(function () {
  var React = window.React;
  var h = React.createElement, useState = React.useState, useId = React.useId, Fragment = React.Fragment;

  function cx() { return Array.prototype.filter.call(arguments, Boolean).join(' '); }
  function omit(o, keys) { var r = {}; for (var k in o) if (keys.indexOf(k) < 0) r[k] = o[k]; return r; }
  function useControlled(value, initial) {
    var s = useState(initial), isC = value !== undefined;
    return [isC ? value : s[0], function (v) { if (!isC) s[1](v); }];
  }
  function clamp(v, a, b) { return Math.min(b, Math.max(a, v)); }

  /* ---------- Icon: 24px grid, 1.75 stroke, round caps ---------- */
  var ICONS = {
    user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4.5 20a7.5 7.5 0 0 1 15 0',
    lock: 'M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3',
    mail: 'M3.5 6h17v12h-17zM4 7l8 6 8-6',
    search: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4',
    eye: 'M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
    eyeOff: 'M3 3l18 18M10.6 5.6A9.6 9.6 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-2.7 3.4M6.6 6.7C4 8.4 2.5 12 2.5 12S6 18.5 12 18.5a9 9 0 0 0 4.4-1.1M9.9 9.9a3 3 0 0 0 4.2 4.2',
    home: 'M3.5 11 12 4l8.5 7M6 9.5V20h12V9.5',
    doc: 'M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6',
    cart: 'M3 4h2.5l2 11h10l2-8H7M9.5 20a1 1 0 1 0 0-.01M16.5 20a1 1 0 1 0 0-.01',
    chart: 'M5 20V12M10 20V6M15 20v-9M20 20V4',
    dollar: 'M12 3v18M16.5 7.5C16 6 14.3 5 12 5 9.5 5 7.5 6.3 7.5 8.3c0 4.4 9 2.4 9 7 0 2-2 3.7-4.5 3.7-2.4 0-4.2-1.1-4.7-2.8',
    sliders: 'M4 7h10M18 7h2M4 17h4M12 17h8M16 5v4M10 15v4',
    bell: 'M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15zM10 20.5a2 2 0 0 0 4 0',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    x: 'M6 6l12 12M18 6 6 18',
    plus: 'M12 5v14M5 12h14',
    minus: 'M5 12h14',
    chevronDown: 'M6 9l6 6 6-6',
    chevronRight: 'M9 6l6 6-6 6',
    chevronLeft: 'M15 6l-6 6 6 6',
    arrowUp: 'M12 19V5M6 11l6-6 6 6',
    arrowDown: 'M12 5v14M6 13l6 6 6-6',
    info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5.5M12 7.6v.4',
    alert: 'M12 4 21.5 20h-19zM12 10v4.5M12 17.2v.3',
    checkCircle: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8 12.3l2.8 2.7L16 9.5',
    calendar: 'M4 6h16v14H4zM4 10.5h16M8.5 3.5v4M15.5 3.5v4',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
    edit: 'M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4',
    trash: 'M4 7h16M9.5 7V4h5v3M6 7l1 13h10l1-13',
    logout: 'M14 4h5v16h-5M10 8l-4 4 4 4M6 12h10',
    more: 'M6 12h.01M12 12h.01M18 12h.01',
    heart: 'M12 20s-8-4.8-8-11a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 9c0 6.2-8 11-8 11z',
    settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1'
  };
  function Icon(p) {
    var size = p.size || 20;
    return h('svg', {
      className: cx('pl-icon', p.className), width: size, height: size, viewBox: '0 0 24 24', fill: 'none',
      stroke: 'currentColor', strokeWidth: p.strokeWidth || 1.75, strokeLinecap: 'round', strokeLinejoin: 'round',
      'aria-hidden': p.label ? undefined : true, role: p.label ? 'img' : undefined, 'aria-label': p.label
    }, h('path', { d: ICONS[p.name] || '' }));
  }
  Icon.names = Object.keys(ICONS);

  /* ---------- Actions ---------- */
  function Button(p) {
    var size = p.size || 'md', isz = size === 'sm' ? 16 : size === 'lg' ? 20 : 18;
    var rest = omit(p, ['variant', 'size', 'icon', 'iconRight', 'block', 'className', 'children']);
    return h('button', Object.assign({ type: 'button' }, rest, {
      className: cx('pl-btn', 'pl-btn--' + (p.variant || 'secondary'), 'pl-btn--' + size, p.block && 'pl-btn--block', p.className)
    }), p.icon && h(Icon, { name: p.icon, size: isz }), p.children, p.iconRight && h(Icon, { name: p.iconRight, size: isz }));
  }

  function IconButton(p) {
    var size = p.size || 'md';
    var rest = omit(p, ['icon', 'label', 'size', 'shape', 'active', 'className']);
    return h('button', Object.assign({ type: 'button' }, rest, {
      className: cx('pl-iconbtn', 'pl-iconbtn--' + size, p.shape === 'square' && 'pl-iconbtn--square', p.active && 'is-active', p.className),
      'aria-label': p.label, 'aria-pressed': p.active === undefined ? undefined : !!p.active
    }), h(Icon, { name: p.icon, size: size === 'sm' ? 16 : size === 'lg' ? 24 : 20 }));
  }

  /* ---------- Inputs ---------- */
  function Field(p, id, control) {
    var hintId = (p.error || p.hint) ? id + '-hint' : undefined;
    return h('div', { className: cx('pl-field', p.error && 'pl-field--error', p.disabled && 'pl-field--disabled', p.className) },
      p.label && h('label', { className: 'pl-label', htmlFor: id }, p.label),
      control(hintId),
      hintId && h('p', { id: hintId, className: 'pl-hint' }, p.error || p.hint));
  }

  function TextField(p) {
    var autoId = useId(), id = p.id || autoId;
    var s = useState(false), shown = s[0];
    var isPw = p.type === 'password';
    var rest = omit(p, ['label', 'hint', 'error', 'icon', 'prefix', 'className', 'id', 'type']);
    return Field(p, id, function (hintId) {
      return h('div', { className: 'pl-control' },
        p.icon && h(Icon, { name: p.icon, size: 18, className: 'pl-input-icon' }),
        p.prefix && h('span', { className: 'pl-input-prefix', 'aria-hidden': true }, p.prefix),
        h('input', Object.assign({}, rest, {
          id: id, type: isPw && shown ? 'text' : (p.type || 'text'), className: 'pl-input',
          'aria-invalid': p.error ? true : undefined, 'aria-describedby': hintId
        })),
        isPw && h('button', {
          type: 'button', className: 'pl-input-action', disabled: p.disabled,
          'aria-label': shown ? 'Hide password' : 'Show password', onClick: function () { s[1](!shown); }
        }, h(Icon, { name: shown ? 'eyeOff' : 'eye', size: 18 })));
    });
  }

  function TextArea(p) {
    var autoId = useId(), id = p.id || autoId;
    var rest = omit(p, ['label', 'hint', 'error', 'className', 'id']);
    return Field(p, id, function (hintId) {
      return h('div', { className: 'pl-control pl-control--area' },
        h('textarea', Object.assign({ rows: 4 }, rest, {
          id: id, className: 'pl-input', 'aria-invalid': p.error ? true : undefined, 'aria-describedby': hintId
        })));
    });
  }

  function Select(p) {
    var autoId = useId(), id = p.id || autoId;
    var rest = omit(p, ['label', 'hint', 'error', 'className', 'id', 'options', 'placeholder']);
    return Field(p, id, function (hintId) {
      return h('div', { className: 'pl-control' },
        h('select', Object.assign({}, rest, {
          id: id, className: 'pl-input', 'aria-invalid': p.error ? true : undefined, 'aria-describedby': hintId,
          defaultValue: p.value === undefined && p.defaultValue === undefined && p.placeholder ? '' : p.defaultValue
        }),
          p.placeholder && h('option', { value: '', disabled: true }, p.placeholder),
          (p.options || []).map(function (o) {
            o = typeof o === 'string' ? { value: o, label: o } : o;
            return h('option', { key: o.value, value: o.value }, o.label);
          })),
        h(Icon, { name: 'chevronDown', size: 18, className: 'pl-select-chevron' }));
    });
  }

  function Checkbox(p) {
    var rest = omit(p, ['label', 'className']);
    return h('label', { className: cx('pl-check', p.disabled && 'is-disabled', p.className) },
      h('input', Object.assign({ type: 'checkbox' }, rest, { className: 'pl-check-input' })),
      h('span', { className: 'pl-check-box', 'aria-hidden': true }, h(Icon, { name: 'check', size: 14, strokeWidth: 2.5 })),
      p.label && h('span', null, p.label));
  }

  function RadioGroup(p) {
    var autoName = useId(), name = p.name || autoName;
    var st = useControlled(p.value, p.defaultValue);
    return h('fieldset', { className: cx('pl-radio-group', p.className) },
      p.label && h('legend', null, p.label),
      (p.options || []).map(function (o) {
        o = typeof o === 'string' ? { value: o, label: o } : o;
        return h('label', { key: o.value, className: cx('pl-check pl-check--radio', (o.disabled || p.disabled) && 'is-disabled') },
          h('input', {
            type: 'radio', className: 'pl-check-input', name: name, value: o.value, checked: st[0] === o.value,
            disabled: o.disabled || p.disabled,
            onChange: function () { st[1](o.value); p.onChange && p.onChange(o.value); }
          }),
          h('span', { className: 'pl-check-box', 'aria-hidden': true }, st[0] === o.value && h('span', { className: 'pl-radio-dot' })),
          h('span', null, o.label));
      }));
  }

  function Switch(p) {
    var st = useControlled(p.checked, !!p.defaultChecked);
    return h('button', {
      type: 'button', role: 'switch', 'aria-checked': st[0], disabled: p.disabled, id: p.id,
      'aria-label': p.label ? undefined : p['aria-label'], className: cx('pl-switch', p.className),
      onClick: function () { st[1](!st[0]); p.onChange && p.onChange(!st[0]); }
    }, h('span', { className: 'pl-switch-track' }, h('span', { className: 'pl-switch-thumb' })), p.label && h('span', null, p.label));
  }

  function fmt(p, v) { return p.format ? p.format(v) : String(v); }

  function Slider(p) {
    var min = p.min == null ? 0 : p.min, max = p.max == null ? 100 : p.max;
    var st = useControlled(p.value, p.defaultValue == null ? min : p.defaultValue);
    var autoId = useId(), id = p.id || autoId, pct = (st[0] - min) / (max - min) * 100;
    return h('div', { className: cx('pl-slider', p.className) },
      (p.label || p.showValue) && h('div', { className: 'pl-slider-head' },
        h('label', { htmlFor: id }, p.label), p.showValue !== false && h('span', { className: 'pl-slider-value' }, fmt(p, st[0]))),
      h('div', { className: 'pl-slider-track' },
        h('div', { className: 'pl-slider-rail' }, h('div', { className: 'pl-slider-fill', style: { left: 0, width: pct + '%' } })),
        h('input', {
          id: id, type: 'range', className: 'pl-range', min: min, max: max, step: p.step || 1, value: st[0], disabled: p.disabled,
          'aria-label': p.label ? undefined : p['aria-label'], 'aria-valuetext': fmt(p, st[0]),
          onChange: function (e) { var v = Number(e.target.value); st[1](v); p.onChange && p.onChange(v); }
        })));
  }

  function RangeSlider(p) {
    var min = p.min == null ? 0 : p.min, max = p.max == null ? 100 : p.max, step = p.step || 1;
    var st = useControlled(p.value, p.defaultValue || [min, max]);
    var lo = st[0][0], hi = st[0][1];
    var pct = function (v) { return (v - min) / (max - min) * 100; };
    function set(i, v) {
      var next = i === 0 ? [clamp(v, min, hi - step), hi] : [lo, clamp(v, lo + step, max)];
      st[1](next); p.onChange && p.onChange(next);
    }
    var name = p.label || 'Range';
    return h('div', { className: cx('pl-slider', p.className), role: 'group', 'aria-label': name },
      p.label && h('div', { className: 'pl-slider-head' }, h('span', null, p.label)),
      h('div', { className: 'pl-slider-track' },
        h('div', { className: 'pl-slider-rail' }, h('div', { className: 'pl-slider-fill', style: { left: pct(lo) + '%', width: (pct(hi) - pct(lo)) + '%' } })),
        h('input', { type: 'range', className: 'pl-range', min: min, max: max, step: step, value: lo, 'aria-label': (p.minLabel || 'Min'), 'aria-valuetext': fmt(p, lo), onChange: function (e) { set(0, Number(e.target.value)); } }),
        h('input', { type: 'range', className: 'pl-range', min: min, max: max, step: step, value: hi, 'aria-label': (p.maxLabel || 'Max'), 'aria-valuetext': fmt(p, hi), onChange: function (e) { set(1, Number(e.target.value)); } })),
      p.showValues !== false && h('div', { className: 'pl-slider-scale' },
        h('span', null, p.minLabel || 'Min', h('b', null, fmt(p, lo))),
        h('span', { style: { textAlign: 'right' } }, p.maxLabel || 'Max', h('b', null, fmt(p, hi)))));
  }

  function SegmentedControl(p) {
    var opts = (p.options || []).map(function (o) { return typeof o === 'string' ? { value: o, label: o } : o; });
    var st = useControlled(p.value, p.defaultValue == null && opts[0] ? opts[0].value : p.defaultValue);
    return h('div', { role: 'radiogroup', 'aria-label': p['aria-label'] || p.label, className: cx('pl-seg', p.className) },
      opts.map(function (o) {
        var on = st[0] === o.value;
        return h('button', {
          key: o.value, type: 'button', role: 'radio', 'aria-checked': on, tabIndex: on ? 0 : -1, className: 'pl-seg-btn',
          onClick: function () { st[1](o.value); p.onChange && p.onChange(o.value); },
          onKeyDown: function (e) {
            var i = opts.indexOf(o), d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
            if (!d) return; e.preventDefault();
            var n = opts[(i + d + opts.length) % opts.length]; st[1](n.value); p.onChange && p.onChange(n.value);
            var sib = e.currentTarget.parentNode.children[(i + d + opts.length) % opts.length]; sib && sib.focus();
          }
        }, o.icon && h(Icon, { name: o.icon, size: 16 }), o.label);
      }));
  }

  /* ---------- Navigation ---------- */
  function Tabs(p) {
    var items = p.items || [], base = useId();
    var st = useControlled(p.value, p.defaultValue || (items[0] && items[0].id));
    var cur = items.filter(function (t) { return t.id === st[0]; })[0];
    return h('div', { className: p.className },
      h('div', { role: 'tablist', className: 'pl-tabs-list', 'aria-label': p['aria-label'] },
        items.map(function (t, i) {
          var on = t.id === st[0];
          return h('button', {
            key: t.id, type: 'button', role: 'tab', id: base + t.id, 'aria-selected': on, 'aria-controls': base + t.id + '-p',
            tabIndex: on ? 0 : -1, className: 'pl-tab',
            onClick: function () { st[1](t.id); p.onChange && p.onChange(t.id); },
            onKeyDown: function (e) {
              var d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0; if (!d) return; e.preventDefault();
              var j = (i + d + items.length) % items.length; st[1](items[j].id); p.onChange && p.onChange(items[j].id);
              var sib = e.currentTarget.parentNode.children[j]; sib && sib.focus();
            }
          }, t.label);
        })),
      cur && h('div', { role: 'tabpanel', id: base + cur.id + '-p', 'aria-labelledby': base + cur.id, className: 'pl-tab-panel', tabIndex: 0 }, cur.content));
  }

  function Breadcrumbs(p) {
    var items = p.items || [];
    return h('nav', { 'aria-label': 'Breadcrumb', className: p.className },
      h('ol', { className: 'pl-crumbs' }, items.map(function (it, i) {
        var last = i === items.length - 1;
        return h('li', { key: i },
          last ? h('span', { 'aria-current': 'page' }, it.label) : h('a', { className: 'pl-link', href: it.href || '#' }, it.label),
          !last && h(Icon, { name: 'chevronRight', size: 14 }));
      })));
  }

  function pageList(cur, n) {
    if (n <= 7) return Array.from({ length: n }, function (_, i) { return i + 1; });
    var out = [1], a = Math.max(2, cur - 1), b = Math.min(n - 1, cur + 1);
    if (cur <= 3) { a = 2; b = 4; } if (cur >= n - 2) { a = n - 3; b = n - 1; }
    if (a > 2) out.push('…');
    for (var i = a; i <= b; i++) out.push(i);
    if (b < n - 1) out.push('…');
    out.push(n); return out;
  }
  function Pagination(p) {
    var n = p.pageCount || 1, st = useControlled(p.page, p.defaultPage || 1), cur = st[0];
    function go(v) { st[1](v); p.onChange && p.onChange(v); }
    return h('nav', { 'aria-label': 'Pagination', className: cx('pl-pages', p.className) },
      h('button', { type: 'button', className: 'pl-page', 'aria-label': 'Previous page', disabled: cur <= 1, onClick: function () { go(cur - 1); } }, h(Icon, { name: 'chevronLeft', size: 18 })),
      pageList(cur, n).map(function (v, i) {
        return v === '…' ? h('span', { key: 'g' + i, className: 'pl-page-gap', 'aria-hidden': true }, '…')
          : h('button', { key: v, type: 'button', className: 'pl-page', 'aria-current': v === cur ? 'page' : undefined, 'aria-label': 'Page ' + v, onClick: function () { go(v); } }, v);
      }),
      h('button', { type: 'button', className: 'pl-page', 'aria-label': 'Next page', disabled: cur >= n, onClick: function () { go(cur + 1); } }, h(Icon, { name: 'chevronRight', size: 18 })));
  }

  function NavList(p) {
    var st = useControlled(p.value, p.defaultValue);
    return h('nav', { 'aria-label': p['aria-label'] || 'Main', className: cx('pl-nav', p.className) },
      (p.items || []).map(function (it) {
        var on = st[0] === it.id;
        return h('a', {
          key: it.id, href: it.href || '#', className: cx('pl-nav-item', on && 'is-active'), 'aria-current': on ? 'page' : undefined,
          onClick: function (e) { if (!it.href) e.preventDefault(); st[1](it.id); p.onChange && p.onChange(it.id); }
        }, it.icon && h(Icon, { name: it.icon, size: 20 }), it.label, it.badge != null && h('span', { className: 'pl-nav-badge' }, h(Badge, null, it.badge)));
      }));
  }

  function NavRail(p) {
    var st = useControlled(p.value, p.defaultValue);
    return h('nav', { 'aria-label': p['aria-label'] || 'Main', className: cx('pl-rail', p.className) },
      (p.items || []).map(function (it) {
        var on = st[0] === it.id;
        return h(Tooltip, { key: it.id, content: it.label },
          h('button', {
            type: 'button', className: cx('pl-rail-btn', on && 'is-active'), 'aria-label': it.label, 'aria-current': on ? 'page' : undefined,
            onClick: function () { st[1](it.id); p.onChange && p.onChange(it.id); }
          }, h(Icon, { name: it.icon, size: 20 })));
      }));
  }

  /* ---------- Containers ---------- */
  function Card(p) {
    var rest = omit(p, ['variant', 'eyebrow', 'title', 'actions', 'className', 'children', 'as']);
    return h(p.as || 'section', Object.assign({}, rest, { className: cx('pl-card', 'pl-card--' + (p.variant || 'outline'), p.className) }),
      p.eyebrow && h('p', { className: 'pl-card-eyebrow' }, p.eyebrow),
      p.title && h('h3', { className: 'pl-card-title' }, p.title),
      p.children != null && (p.title || p.eyebrow ? h('div', { className: 'pl-card-body' }, p.children) : p.children),
      p.actions && h('div', { className: 'pl-card-actions' }, p.actions));
  }

  function Divider(p) {
    return h('hr', { className: cx('pl-divider', p.variant === 'rule' && 'pl-divider--rule', p.className) });
  }

  function BentoGrid(p) {
    var style = Object.assign({ '--pl-cols': p.columns || 4, '--pl-row': (p.rowHeight || 140) + 'px' }, p.style);
    return h('div', { className: cx('pl-bento', p.className), style: style }, p.children);
  }
  function BentoCell(p) {
    return h('div', { className: cx('pl-bento-cell', p.className), style: { gridColumn: 'span ' + (p.cols || 1), gridRow: 'span ' + (p.rows || 1) } }, p.children);
  }

  function StatTile(p) {
    var up = p.trend === 'up', down = p.trend === 'down';
    return h(Card, { variant: p.variant || 'outline', className: cx('pl-stat', p.className), style: p.style },
      h('div', { className: 'pl-stat-head' },
        h('span', { className: 'pl-stat-label' }, p.label),
        p.icon && h('span', { className: 'pl-stat-well', 'aria-hidden': true }, h(Icon, { name: p.icon, size: 24 }))),
      h('div', { className: 'pl-stat-value' }, p.value),
      p.delta && h('span', { className: cx('pl-stat-delta', up && 'is-up', down && 'is-down') },
        (up || down) && h(Icon, { name: up ? 'arrowUp' : 'arrowDown', size: 14, strokeWidth: 2 }),
        p.delta));
  }

  function SummaryList(p) {
    return h('dl', { className: cx('pl-summary', p.className) },
      (p.rows || []).map(function (r, i) {
        return h(Fragment, { key: i },
          h('dt', { className: r.total ? 'is-total' : undefined }, r.label),
          h('dd', { className: r.total ? 'is-total' : undefined }, r.value));
      }));
  }

  function Dialog(p) {
    var base = useId();
    if (p.open === false) return null;
    return h('div', { className: cx('pl-overlay', p.inline && 'pl-overlay--inline'), onClick: function (e) { if (e.target === e.currentTarget && p.onClose) p.onClose(); } },
      h('div', {
        role: 'dialog', 'aria-modal': p.inline ? undefined : true, 'aria-labelledby': base + 't', className: cx('pl-dialog', p.className),
        onKeyDown: function (e) { if (e.key === 'Escape' && p.onClose) p.onClose(); }
      },
        h('div', { className: 'pl-dialog-head' },
          h('h2', { id: base + 't', className: 'pl-dialog-title' }, p.title),
          p.onClose && h(IconButton, { icon: 'x', label: 'Close', size: 'sm', onClick: p.onClose })),
        p.children && h('div', { className: 'pl-dialog-body' }, p.children),
        p.actions && h('div', { className: 'pl-dialog-actions' }, p.actions)));
  }

  function Menu(p) {
    return h('div', { role: 'menu', 'aria-label': p['aria-label'], className: cx('pl-menu', p.className) },
      (p.items || []).map(function (it, i) {
        if (it.separator) return h('div', { key: 's' + i, role: 'separator', className: 'pl-menu-sep' });
        return h('button', {
          key: i, type: 'button', role: 'menuitem', className: cx('pl-menu-item', it.danger && 'pl-menu-item--danger'),
          onClick: function () { it.onSelect && it.onSelect(); p.onSelect && p.onSelect(it); }
        }, it.icon && h(Icon, { name: it.icon, size: 18 }), it.label, it.shortcut && h('span', { className: 'pl-menu-shortcut' }, it.shortcut));
      }));
  }

  function Tooltip(p) {
    var id = useId(), s = useState(false), open = p.open != null ? p.open : s[0];
    var child = React.Children.only(p.children);
    return h('span', {
      className: 'pl-tip-wrap',
      onMouseEnter: function () { s[1](true); }, onMouseLeave: function () { s[1](false); },
      onFocus: function () { s[1](true); }, onBlur: function () { s[1](false); },
      onKeyDown: function (e) { if (e.key === 'Escape') s[1](false); }
    }, React.cloneElement(child, { 'aria-describedby': id }),
      h('span', { role: 'tooltip', id: id, className: cx('pl-tip', open && 'is-open') }, p.content));
  }

  /* ---------- Feedback ---------- */
  var TONE_ICON = { neutral: 'info', success: 'checkCircle', warning: 'alert', danger: 'alert' };
  function Alert(p) {
    var tone = p.tone || 'neutral';
    return h('div', { role: tone === 'danger' ? 'alert' : 'status', className: cx('pl-alert', 'pl-tone--' + tone, p.className) },
      h('span', { className: 'pl-alert-icon', 'aria-hidden': true }, h(Icon, { name: p.icon || TONE_ICON[tone], size: 18 })),
      h('div', { className: 'pl-alert-main' },
        p.title && h('p', { className: 'pl-alert-title' }, p.title),
        p.children && h('p', { className: 'pl-alert-text' }, p.children)),
      p.onClose && h(IconButton, { icon: 'x', label: 'Dismiss', size: 'sm', onClick: p.onClose }));
  }

  function Toast(p) {
    var tone = p.tone || 'neutral';
    return h('div', { role: 'status', 'aria-live': 'polite', className: cx('pl-toast', 'pl-tone--' + tone, p.className) },
      h('span', { className: 'pl-alert-icon', 'aria-hidden': true }, h(Icon, { name: p.icon || TONE_ICON[tone], size: 16 })),
      h('span', { className: 'pl-toast-msg' }, p.children),
      p.action && h(Button, { size: 'sm', variant: 'ghost', onClick: p.onAction }, p.action),
      p.onClose && h(IconButton, { icon: 'x', label: 'Dismiss', size: 'sm', onClick: p.onClose }));
  }

  function Badge(p) {
    return h('span', { className: cx('pl-badge', p.tone && p.tone !== 'neutral' && 'pl-badge--' + p.tone, p.className) },
      p.dot && h('span', { className: 'pl-badge-dot', 'aria-hidden': true }), p.children);
  }

  function Progress(p) {
    var max = p.max || 100, v = clamp(p.value || 0, 0, max), pct = Math.round(v / max * 100);
    return h('div', { className: cx('pl-progress', p.className) },
      (p.label || p.showValue) && h('div', { className: 'pl-progress-head' }, h('span', null, p.label), p.showValue !== false && h('span', null, pct + '%')),
      h('div', { className: 'pl-progress-track', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': max, 'aria-valuenow': v, 'aria-label': p.label || p['aria-label'] },
        h('div', { className: 'pl-progress-bar', style: { width: pct + '%' } })));
  }

  function ProgressRing(p) {
    var size = p.size || 96, stroke = p.thickness || 8, max = p.max || 100, v = clamp(p.value || 0, 0, max);
    var r = (size - stroke) / 2 - 8, c = 2 * Math.PI * r, pct = Math.round(v / max * 100);
    return h('div', { className: cx('pl-ring', p.className), style: { width: size, height: size }, role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': max, 'aria-valuenow': v, 'aria-label': p['aria-label'] || p.label },
      h('svg', { width: size, height: size, viewBox: '0 0 ' + size + ' ' + size, 'aria-hidden': true },
        h('circle', { className: 'pl-ring-track', cx: size / 2, cy: size / 2, r: r, fill: 'none', strokeWidth: stroke }),
        h('circle', { className: 'pl-ring-bar', cx: size / 2, cy: size / 2, r: r, fill: 'none', strokeWidth: stroke, strokeLinecap: 'round', strokeDasharray: c, strokeDashoffset: c * (1 - v / max) })),
      h('span', { className: 'pl-ring-label' }, p.children != null ? p.children : pct + '%'));
  }

  /* ---------- Data display ---------- */
  function initials(name) { return (name || '').split(/\s+/).filter(Boolean).slice(0, 2).map(function (w) { return w[0].toUpperCase(); }).join(''); }
  function Avatar(p) {
    var size = p.size || 48;
    return h('span', { className: cx('pl-avatar', p.className), style: { width: size, height: size, fontSize: Math.round(size * 0.36) }, role: 'img', 'aria-label': p.name + (p.status ? ' (' + p.status + ')' : '') },
      p.src ? h('img', { src: p.src, alt: '' }) : h('span', { 'aria-hidden': true }, initials(p.name)),
      p.status && h('span', { className: cx('pl-avatar-status', p.status === 'away' && 'is-away', p.status === 'offline' && 'is-offline'), 'aria-hidden': true }));
  }

  function Table(p) {
    var cols = p.columns || [];
    return h('div', { className: cx('pl-table-wrap', p.className), style: p.style },
      h('table', { className: 'pl-table' },
        p.caption && h('caption', { style: { position: 'absolute', width: 1, height: 1, overflow: 'hidden', clip: 'rect(0 0 0 0)' } }, p.caption),
        h('thead', null, h('tr', null, cols.map(function (c) { return h('th', { key: c.key, scope: 'col', className: c.align === 'right' ? 'is-num' : undefined }, c.label); }))),
        h('tbody', null, (p.rows || []).map(function (r, i) {
          return h('tr', { key: r.id || i }, cols.map(function (c) {
            return h('td', { key: c.key, className: c.align === 'right' ? 'is-num' : undefined }, c.render ? c.render(r) : r[c.key]);
          }));
        }))));
  }

  var api = {
    Button: Button, IconButton: IconButton, TextField: TextField, TextArea: TextArea, Select: Select, Checkbox: Checkbox,
    RadioGroup: RadioGroup, Switch: Switch, Slider: Slider, RangeSlider: RangeSlider, SegmentedControl: SegmentedControl,
    Tabs: Tabs, Breadcrumbs: Breadcrumbs, Pagination: Pagination, NavList: NavList, NavRail: NavRail, Card: Card, Divider: Divider,
    BentoGrid: BentoGrid, BentoCell: BentoCell, StatTile: StatTile, SummaryList: SummaryList, Dialog: Dialog, Menu: Menu,
    Tooltip: Tooltip, Alert: Alert, Toast: Toast, Badge: Badge, Progress: Progress, ProgressRing: ProgressRing, Avatar: Avatar,
    Table: Table, Icon: Icon
  };
  window.PebbleLine = Object.assign(window.PebbleLine || {}, api);
})();
