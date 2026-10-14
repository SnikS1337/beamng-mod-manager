"""Проверка читаемости: для каждого текста на странице берёт фон под ним (снимок с невидимым текстом) и считает контраст WCAG.
Запуск: python3 contrast.py <файл.dc.html> [порог]   (сервер на :8766 в папке out/)"""
import sys, io, json
import numpy as np
from PIL import Image
from playwright.sync_api import sync_playwright

f = sys.argv[1]; limit = float(sys.argv[2]) if len(sys.argv) > 2 else 4.5
JS = """() => {
 const out = []; const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
 while (w.nextNode()) {
  const n = w.currentNode; const t = n.textContent.trim(); if (!t) continue;
  const p = n.parentElement; if (!p || ['SCRIPT','STYLE'].includes(p.tagName)) continue;
  const cs = getComputedStyle(p); if (cs.visibility === 'hidden' || cs.display === 'none') continue;
  let op = 1; for (let e = p; e; e = e.parentElement) op *= parseFloat(getComputedStyle(e).opacity);
  if (op < 0.2) continue;
  const r = document.createRange(); r.selectNodeContents(n); const b = r.getBoundingClientRect();
  if (b.width < 2 || b.height < 2 || b.right < 0 || b.bottom < 0 || b.left > 1440 || b.top > 960) continue;
  out.push({t: t.slice(0, 40), x: b.left, y: b.top, w: b.width, h: b.height, c: cs.color, fs: parseFloat(cs.fontSize), fw: parseInt(cs.fontWeight), op});
 }
 return out; }"""


def lin(c):
    c = c / 255.0
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def lum(rgb):
    l = lin(rgb[..., :3].astype(float))
    return 0.2126 * l[..., 0] + 0.7152 * l[..., 1] + 0.0722 * l[..., 2]


def parse(c):
    v = [float(x) for x in c[c.index('(') + 1:c.index(')')].replace('/', ',').replace(' ', ',').split(',') if x]
    return (v + [1.0])[:4] if len(v) == 3 else v[:4]


with sync_playwright() as p:
    b = p.chromium.launch(executable_path='/opt/pw-browsers/chromium')
    pg = b.new_page(viewport={'width': 1440, 'height': 960})
    pg.goto('http://localhost:8766/' + f); pg.wait_for_timeout(5500)
    import os
    if os.environ.get('STRESS'):   # худший случай: под стеклом белое (светлое фото, белый постер)
        pg.add_style_tag(content='.pic:not(.wall),.ph,.pw{background-image:none!important;background-color:#fff!important}.scrim{display:none!important}')
        pg.wait_for_timeout(300)
    items = pg.evaluate(JS)
    pg.add_style_tag(content='*{color:transparent!important;-webkit-text-fill-color:transparent!important;text-shadow:none!important}input::placeholder{color:transparent!important}')
    pg.wait_for_timeout(300)
    img = np.asarray(Image.open(io.BytesIO(pg.screenshot())).convert('RGB'))
    b.close()

bad = []; n = 0
for it in items:
    x0, y0 = max(0, int(it['x'])), max(0, int(it['y'])); x1, y1 = min(1440, int(it['x'] + it['w']) + 1), min(960, int(it['y'] + it['h']) + 1)
    if x1 - x0 < 2 or y1 - y0 < 2: continue
    n += 1
    reg = img[y0:y1, x0:x1].reshape(-1, 3)
    r, g, bl, a = parse(it['c']); a *= it['op']
    mean = reg.mean(axis=0)
    txt_l = lambda bg: lum(np.array([[r * a + bg[0] * (1 - a), g * a + bg[1] * (1 - a), bl * a + bg[2] * (1 - a)]]))[0]
    Lt = txt_l(mean)
    Lbg = lum(reg)
    light = Lt > 0.4
    worst = np.percentile(Lbg, 92 if light else 8)      # самый неудачный участок фона под буквами
    Lm = Lbg.mean()
    cm = (max(Lt, Lm) + .05) / (min(Lt, Lm) + .05)
    cw = (max(Lt, worst) + .05) / (min(Lt, worst) + .05)
    large = it['fs'] >= 24 or (it['fs'] >= 18.66 and it['fw'] >= 700)
    need = 3.0 if large else limit
    if min(cm, cw) < need:
        bad.append((round(cw, 2), round(cm, 2), it['t'], int(it['x']), int(it['y']), it['fs']))
bad.sort()
print(f'текстов проверено: {n}, ниже порога: {len(bad)}')
for r in bad: print(f'  худший {r[0]:5}  средний {r[1]:5}  «{r[2]}»  x={r[3]} y={r[4]} {r[5]}px')
