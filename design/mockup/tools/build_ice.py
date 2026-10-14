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
    return _surface(glass, name, blur + 3, scale)   # изморозь рассеивает сильнее


U.rim = rim
U.surface = surface

EXTRA = '''
.glass{background:rgba(52,82,124,.26) !important}
.glass.insp{background:rgba(26,42,68,.52) !important}
.ice{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-size:100% 100%;opacity:.82}
.gl{position:absolute;opacity:0;pointer-events:none;background:
 linear-gradient(90deg,transparent 0,rgba(235,246,255,.95) 50%,transparent 100%) center/100% 1.5px no-repeat,
 linear-gradient(0deg,transparent 0,rgba(235,246,255,.95) 50%,transparent 100%) center/1.5px 100% no-repeat,
 radial-gradient(circle,rgba(255,255,255,.95) 0,rgba(210,232,255,.0) 55%);animation:glint var(--beat) ease-in-out infinite}
@keyframes glint{0%,70%,100%{opacity:0;transform:scale(.4) rotate(0)}82%{opacity:1;transform:scale(1) rotate(12deg)}92%{opacity:.25;transform:scale(.7) rotate(20deg)}}
.plate{box-shadow:0 12px 32px rgba(6,10,22,.38), inset 0 0 0 1px rgba(205,230,255,.20), inset 0 1px 0 rgba(235,246,255,.34), inset 0 0 26px rgba(170,205,255,.07) !important}
@media (prefers-reduced-motion:reduce){.gl{animation:none;opacity:.5}}
'''


def build():
    html = U.garage(True)
    html = html.replace('<title>Гараж</title>', '<title>Гараж: ледяное стекло</title>')
    return html.replace('</style>', EXTRA + '</style>', 1)


if __name__ == '__main__':
    open(f'{U.OUT}/GarageIce.dc.html', 'w').write(build())
    print('ok')
