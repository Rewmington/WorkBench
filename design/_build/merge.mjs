import { readFileSync, writeFileSync } from 'node:fs';

const dir = 'D:/ADevelop/Repositories/Mine/workbench/design';
const read = f => readFileSync(`${dir}/${f}`, 'utf8');

function scopeCss(css, prefix) {
  return css.replace(/([^{}]+)\{([^{}]*)\}/g, (m, rawSel, body) => {
    const lead = rawSel.match(/^(\s*(?:\/\*[\s\S]*?\*\/\s*)*)/)[1];
    const s = rawSel.slice(lead.length).trim();
    if (!s || s.startsWith('@')) return m;
    const out = s.split(',').map(part => {
      const t = part.trim();
      if (/^\.(wz|st)\b/.test(t)) return t;
      if (/^[a-z]+$/i.test(t)) return t;
      return `${prefix} ${t}`;
    }).join(',\n  ');
    return `${lead}${out} {${body}}`;
  });
}

function panes(src, from, to) {
  const a = src.indexOf(from), b = src.indexOf(to);
  if (a < 0 || b < 0 || b < a) throw new Error(`marker not found: ${from} / ${to}`);
  return src.slice(a, b).replace(/\s*<\/div>\s*$/, '\n').trim();
}

const d = read('workbench-direction-d.html');
const st = read('workbench-stats.html');

const dCss = scopeCss(d.match(/<style>([\s\S]*?)<\/style>/)[1], '.wz');
const stCss = scopeCss(st.match(/<style>([\s\S]*?)<\/style>/)[1], '.st');
const dPanes = `<div class="wz" id="wz" data-theme="dark">\n${panes(d, '<!-- D1 HOME -->', '<script>')}\n</div>`;
const stPanes = `<div class="st" id="st" data-theme="dark">\n${panes(st, '<section class="st-pane" data-pane="v"', '<script>')}\n</div>`;
const stDataJs = panes(st, '// deterministic PRNG', "show('v');");

const png = readFileSync(`${dir}/refs/yics-knowledge-os-1000.png`).toString('base64');
const img = `<img src="data:image/png;base64,${png}" alt="YCS Knowledge OS 参考截图">`;

const out = read('_build/00_shell.html')
  .replace('<!--{{D_PANES}}-->', dPanes)
  .replace('<!--{{ST_PANES}}-->', stPanes)
  .replace('<!--{{YCS_IMG}}-->', img)
  .replace('<!--{{ST_DATA_JS}}-->', stDataJs)
  .replace('<!--{{EXTRA_CSS}}-->', `<style>\n/* ===== workbench-direction-d.html 的样式，已用 .wz 作用域隔离 ===== */\n${dCss}</style>\n<style>\n/* ===== workbench-stats.html 的样式，已用 .st 作用域隔离 ===== */\n${stCss}</style>`);

if (out.includes('{{')) throw new Error('unresolved placeholder');
writeFileSync(`${dir}/workbench-ui.html`, out);
console.log('workbench-ui.html', (out.length / 1024).toFixed(0) + 'KB');
