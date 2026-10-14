"""Доска «Гараж: ледяное стекло» — вариант главного экрана, где к жидкому стеклу добавлен иней по краям,
холодный оттенок и искры льда. Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_ice.py"""
import random
import build_ui as U

_rim = U.rim
_surface = U.surface
SIZES = {'rail': (84, 212), 'gear': (56, 56), 'bar': (964, 52), 'insp': (328, 928)}


def glints(name, n, seed):
    """Искры льда: крестики у краёв панели, мерцают в общем ритме (шаг 1 с внутри удара 6 с)."""
    W, H = SIZES[name]
    r = random.Random(seed)
    out = ''
    for _ in range(n):
        edge = r.choice('tblr')
        x = r.randint(6, W - 6); y = r.randint(6, H - 6)
        if edge == 't': y = r.randint(4, 16)
        elif edge == 'b': y = H - r.randint(4, 16)
        elif edge == 'l': x = r.randint(4, 16)
        else: x = W - r.randint(4, 16)
        s = r.choice([9, 11, 13, 15])
        out += f'<i class="gl" style="left:{x - s // 2}px;top:{y - s // 2}px;width:{s}px;height:{s}px;animation-delay:{-r.randint(0, 5)}s"></i>'
    return out


def rim(glass, name):
    base = _rim(glass, name)
    if glass and name in SIZES:
        n = {'rail': 4, 'gear': 2, 'bar': 8, 'insp': 12}[name]
        return base + f'<div class="ice" style="background-image:url({U.A("ice_" + name + ".png")})">{glints(name, n, hash(name) % 97)}</div>'
    return base


def surface(glass, name, blur, scale=None):
    # настоящее Liquid Glass почти не размывает: видно картинку, а не пятно; смысл даёт преломление
    return _surface(glass, name, {'Insp': 5, 'Bar': 3}.get(name, 2), scale)


_lens = U.lens
def lens(fid, name, w, h, scale=46):
    return _lens(fid, name, w, h, int(scale * 1.5))   # линза заметнее: контент изгибается у краёв
U.lens = lens


U.rim = rim
U.surface = surface

EXTRA = '''
.glass{background:rgba(16,26,48,.20) !important}
.glass.top{background:rgba(16,26,48,.30) !important}
.glass.top,.glass.lft{text-shadow:0 1px 8px rgba(8,14,28,.55)}
.glass.insp{background:rgba(18,30,52,.40) !important}
.ice{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-size:100% 100%;opacity:.55}
.gl{position:absolute;opacity:0;pointer-events:none;background:
 linear-gradient(90deg,transparent 0,rgba(235,246,255,.7) 50%,transparent 100%) center/100% 1.5px no-repeat,
 linear-gradient(0deg,transparent 0,rgba(235,246,255,.7) 50%,transparent 100%) center/1.5px 100% no-repeat,
 radial-gradient(circle,rgba(255,255,255,.95) 0,rgba(210,232,255,.0) 55%);animation:glint var(--beat) ease-in-out infinite}
@keyframes glint{0%,70%,100%{opacity:0;transform:scale(.4) rotate(0)}82%{opacity:1;transform:scale(1) rotate(12deg)}92%{opacity:.25;transform:scale(.7) rotate(20deg)}}
.plate{box-shadow:0 12px 32px rgba(6,10,22,.38), inset 0 0 0 1px rgba(205,230,255,.20), inset 0 1px 0 rgba(235,246,255,.34), inset 0 0 26px rgba(170,205,255,.07) !important}
@media (prefers-reduced-motion:reduce){.gl{animation:none;opacity:.55}}
'''


def build():
    html = U.garage(True)
    html = html.replace('<title>Гараж</title>', '<title>Гараж: ледяное стекло</title>')
    return html.replace('</style>', EXTRA + '</style>', 1)


if __name__ == '__main__':
    open(f'{U.OUT}/GarageIce.dc.html', 'w').write(build())
    print('ok')
