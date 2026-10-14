"""Доска «Гараж: ледяное стекло» — вариант главного экрана, где к жидкому стеклу добавлен иней по краям,
холодный оттенок и искры льда. Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_ice.py"""
import random
import build_ui as U

_rim = U.rim
_surface = U.surface
SIZES = {'rail': (84, 928), 'gear': (56, 56), 'bar': (964, 52), 'insp': (328, 928)}


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
    base = _rim(glass, 'railt' if name == 'rail' else name)
    if glass and name in SIZES:
        n = {'rail': 14, 'gear': 2, 'bar': 8, 'insp': 12}[name]
        return base + f'<div class="ice" style="background-image:url({U.A("ice_" + name + ".png")})">{glints(name, n, hash(name) % 97)}</div>'
    return base


def surface(glass, name, blur, scale=None):
    # настоящее Liquid Glass почти не размывает: видно картинку, а не пятно; смысл даёт преломление
    return _surface(glass, name, {'Insp': 5, 'Bar': 3}.get(name, 2), scale)


_lens = U.lens
def lens(fid, name, w, h, scale=46):
    if fid == 'lgRail':
        name, h = 'railt', 928
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


W_SEG = [52, 112, 148, 152]            # ширины пунктов (фиксированы, чтобы капля знала, куда ехать)
LABELS = [('Все', None), ('Включённые', None), ('Нужно обновить', '3'), ('Не проверяются', '12')]
OVER, TRK = 6, 3                        # капля выступает за пункт на 6 px по бокам; поля подложки 3 px


def drop_filter():
    """Линза капли для любой ширины: карта растягивается по элементу (filterUnits=objectBoundingBox)."""
    def ch(res, k, keep):
        m = {'r': '1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0', 'g': '0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0', 'b': '0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0'}[keep]
        return (f'<feDisplacementMap in="SourceGraphic" in2="map" scale="{44 * k:.0f}" xChannelSelector="R" yChannelSelector="G" result="{res}d"/>'
                f'<feColorMatrix in="{res}d" type="matrix" values="{m}" result="{res}"/>')
    flex = (f'<filter id="dpf" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB">'
            f'<feImage href="{U.A("drop.png")}" preserveAspectRatio="none" result="map"/>'
            f'{ch("r", 1.0, "r")}{ch("g", 1.12, "g")}{ch("b", 1.26, "b")}'
            f'<feComposite in="r" in2="g" operator="arithmetic" k1="0" k2="1" k3="1" k4="0" result="rg"/>'
            f'<feComposite in="rg" in2="b" operator="arithmetic" k1="0" k2="1" k3="1" k4="0"/></filter>')
    return flex + U.lens('dpn', 'dropsq', 70, 58, 44)


def segmented():
    lefts, x = [], TRK
    for w in W_SEG:
        lefts.append(x - OVER); x += w
    labs = ''.join(
        f'<label><input type="radio" name="f" id="f{i}" {"checked" if i == 0 else ""}><span style="width:{W_SEG[i]}px">{t}'
        f'{(" <b style=\"opacity:.7;font-weight:600;margin-left:5px\">" + c + "</b>") if c else ""}</span></label>' for i, (t, c) in enumerate(LABELS))
    rules = ''.join(f'.sg:has(#f{i}:checked) .drop{{left:{lefts[i]}px;width:{W_SEG[i] + 2 * OVER}px}}\n' for i in range(4))
    return f'<div class="sg" role="radiogroup" aria-label="Фильтр">{labs}<i class="drop"></i></div>', rules


DROP_CSS = '''
.sg{position:relative;display:flex;padding:3px;border-radius:21px;background:rgba(8,14,28,.34);box-shadow:inset 0 1px 4px rgba(0,0,0,.42),inset 0 -1px 0 rgba(255,255,255,.10)}
.sg label{position:relative;display:block;cursor:pointer}
.sg input{position:absolute;opacity:0;pointer-events:none}
.sg label span{position:relative;z-index:4;display:flex;align-items:center;justify-content:center;height:30px;font-size:13px;font-weight:550;color:rgba(255,255,255,.78);transition:color var(--fast)}
.sg label:hover span{color:#fff}
.sg input:checked+span{color:#fff;font-weight:650}
.sg input:focus-visible+span{outline:3px solid #fff;outline-offset:2px;border-radius:15px}
.drop,.nv.on{border-radius:20px}
.drop{position:absolute;top:-2px;height:40px;left:-3px;width:64px;z-index:3;pointer-events:none;
 -webkit-backdrop-filter:url(#dpf) saturate(1.5) brightness(1.12);backdrop-filter:url(#dpf) saturate(1.5) brightness(1.12);
 background:linear-gradient(160deg,rgba(255,255,255,.24),rgba(255,255,255,.05) 52%,rgba(255,255,255,.13));
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.30),inset 2px 2px 4px -1px rgba(255,255,255,.85),inset -2px -2px 4px -1px rgba(255,255,255,.45),inset 0 0 16px rgba(255,255,255,.10),0 10px 20px rgba(4,8,20,.50),0 2px 4px rgba(4,8,20,.35);
 transition:left .55s cubic-bezier(.3,1.4,.45,1),width .55s cubic-bezier(.3,1.4,.45,1)}
.drop::before{content:"";position:absolute;inset:0;border-radius:inherit;padding:1.5px;background:conic-gradient(from 210deg,rgba(255,255,255,0),rgba(110,200,255,.9) 14%,rgba(255,255,255,0) 30%,rgba(255,255,255,0) 58%,rgba(255,110,200,.85) 72%,rgba(255,255,255,0) 90%);
 -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.nv{height:58px;box-sizing:border-box;position:relative}
.nv[style*="0.20"]{background:linear-gradient(160deg,rgba(255,255,255,.24),rgba(255,255,255,.05) 52%,rgba(255,255,255,.13)) !important;border-radius:24px;
 -webkit-backdrop-filter:url(#lgdpn) saturate(1.5) brightness(1.12);backdrop-filter:url(#lgdpn) saturate(1.5) brightness(1.12);
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.30),inset 2px 2px 4px -1px rgba(255,255,255,.85),inset -2px -2px 4px -1px rgba(255,255,255,.45),inset 0 0 16px rgba(255,255,255,.10),0 10px 20px rgba(4,8,20,.50),0 2px 4px rgba(4,8,20,.35) !important}
'''


NAV = [('grid', 'Гараж', 16, None), ('upd', 'Обновления', 80, '3'), ('search', 'Поиск', 144, None), ('gear', 'Настройки', 854, None)]
RECENT = [('dacia', True, True, 'Dacia Sandero Stepway'), ('mustang', False, False, 'Ford Mustang Shelby GT500'), ('lambo', False, False, 'Lamborghini Huracán EVO'),
          ('tractor', False, False, 'New Holland T7'), ('truck', False, True, 'Kenworth T610'), ('coupe', False, False, 'Falcon RS Coupe'), ('lebedka', False, False, 'Lebedka Tow')]


def chrome(glass, active):
    """Левая панель на всю высоту: разделы (подвижная капля), «Недавние» фото, «Играть», настройки внутри."""
    items = ''
    for i, (k, label, top, badge) in enumerate(NAV):
        bd = (f'<span class="pulse" style="position:absolute;top:3px;right:6px;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;border-radius:9px;background:#fff;color:#111;'
              f'font-size:11px;font-weight:700;display:flex;align-items:center;justify-content:center">{badge}</span>') if badge else ''
        items += (f'<label class="rn" style="top:{top}px"><input type="radio" name="nv" id="n{i}" {"checked" if i == 0 else ""} aria-label="{label}">'
                  f'{U.icon(k, 22, 1.7)}<span>{label}</span>{bd}</label>')
    rules = ''.join(f'.lft:has(#n{i}:checked) .ndrop{{top:{t}px}}\n' for i, (_, _, t, _) in enumerate(NAV))
    thumbs = ''.join(
        f'<a class="th p-{img}{" cur" if cur else ""}" href="#" title="{name}" aria-label="{name}" style="top:{258 + i * 64}px">'
        f'{"<i class=\"pulse\" style=\"position:absolute;top:-1px;right:-1px;width:12px;height:12px;border-radius:50%;background:#fff;box-shadow:0 0 0 2px rgba(20,30,52,.6)\"></i>" if upd else ""}</a>'
        for i, (img, cur, upd, name) in enumerate(RECENT))
    play = ('<button type="button" class="play" aria-label="Играть"><svg width="26" height="26" viewBox="0 0 24 24" fill="#fff" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg></button>'
            '<span class="plabel">Играть</span>')
    html = (f'<nav class="glass lft" aria-label="Разделы" style="left:16px;top:16px;width:84px;height:928px;border-radius:36px;{U.surface(glass, "Rail", 7)}">'
            f'{U.rim(glass, "rail")}<div style="position:absolute;inset:0"><i class="ndrop"></i>{items}'
            f'<i class="hr" style="top:222px"></i><span class="cap" style="top:232px">Недавние</span>{thumbs}{play}</div></nav>\n')
    return html + f'<style>{rules}</style>'


U.chrome = chrome

RAIL_CSS = '''
.ndrop{position:absolute;left:7px;top:16px;width:70px;height:58px;z-index:2;pointer-events:none;border-radius:24px;
 background:linear-gradient(160deg,rgba(255,255,255,.24),rgba(255,255,255,.05) 52%,rgba(255,255,255,.13));
 -webkit-backdrop-filter:url(#lgdpn) saturate(1.5) brightness(1.12);backdrop-filter:url(#lgdpn) saturate(1.5) brightness(1.12);
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.30),inset 2px 2px 4px -1px rgba(255,255,255,.85),inset -2px -2px 4px -1px rgba(255,255,255,.45),inset 0 0 16px rgba(255,255,255,.10),0 10px 20px rgba(4,8,20,.50),0 2px 4px rgba(4,8,20,.35);
 transition:top .65s cubic-bezier(.3,1.25,.45,1)}
.ndrop::before{content:"";position:absolute;inset:0;border-radius:inherit;padding:1.5px;background:conic-gradient(from 210deg,rgba(255,255,255,0),rgba(110,200,255,.9) 14%,rgba(255,255,255,0) 30%,rgba(255,255,255,0) 58%,rgba(255,110,200,.85) 72%,rgba(255,255,255,0) 90%);
 -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
.rn{position:absolute;left:7px;width:70px;height:58px;z-index:3;box-sizing:border-box;padding-top:9px;display:flex;flex-direction:column;align-items:center;gap:4px;color:rgba(255,255,255,.74);font-size:11px;font-weight:500;cursor:pointer;transition:color var(--fast)}
.rn input{position:absolute;opacity:0;pointer-events:none}
.rn:hover{color:#fff}.rn:has(input:checked){color:#fff;font-weight:600}
.rn:has(input:focus-visible){outline:3px solid #fff;outline-offset:2px;border-radius:24px}
.hr{position:absolute;left:22px;width:40px;height:1px;background:rgba(255,255,255,.24)}
.cap{position:absolute;left:0;right:0;text-align:center;font-size:10.5px;letter-spacing:.02em;color:rgba(255,255,255,.62)}
.th{position:absolute;left:16px;width:52px;height:52px;border-radius:50%;background-size:cover;background-position:center;z-index:3;
 box-shadow:0 0 0 1.5px rgba(255,255,255,.50),0 8px 14px rgba(4,8,20,.45);transition:transform var(--mid) var(--ease),box-shadow var(--mid) var(--ease)}
.th:hover{transform:translateY(-2px) scale(1.07);box-shadow:0 0 0 2px rgba(255,255,255,.85),0 0 22px rgba(170,205,255,.45),0 12px 18px rgba(4,8,20,.5)}
.th.cur{box-shadow:0 0 0 2.5px #fff,0 0 24px rgba(170,205,255,.55),0 8px 14px rgba(4,8,20,.45)}
.play{position:absolute;left:14px;top:766px;width:56px;height:56px;border:0;border-radius:50%;z-index:3;cursor:pointer;display:flex;align-items:center;justify-content:center;padding-left:3px;
 background:linear-gradient(160deg,rgba(150,205,255,.60),rgba(80,130,225,.30));
 -webkit-backdrop-filter:blur(2px) saturate(1.6);backdrop-filter:blur(2px) saturate(1.6);
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.4),inset 2px 2px 4px -1px rgba(255,255,255,.9),inset -2px -2px 4px -1px rgba(255,255,255,.4),0 0 26px rgba(130,190,255,.55),0 10px 20px rgba(4,8,20,.5);
 transition:transform var(--fast) var(--ease)}
.play:hover{transform:scale(1.06)}.play:active{transform:scale(.96)}
.plabel{position:absolute;left:0;right:0;top:826px;text-align:center;font-size:11px;font-weight:500;color:rgba(255,255,255,.8)}
'''


def build():
    html = U.garage(True)
    html = html.replace('<title>Гараж</title>', '<title>Гараж: ледяное стекло</title>')
    import re
    seg, rules = segmented()
    html, n = re.subn(r'<div role="radiogroup" aria-label="Фильтр".*?</div>(?=<label style)', lambda m: seg, html, count=1, flags=re.S)
    assert n == 1, 'полоса фильтров не найдена'
    html = html.replace('</defs></svg>', drop_filter().replace('id="dpn"', 'id="lgdpn"') + '</defs></svg>', 1)
    return html.replace('</style>', EXTRA + DROP_CSS + RAIL_CSS + rules + '</style>', 1)


if __name__ == '__main__':
    open(f'{U.OUT}/GarageIce.dc.html', 'w').write(build())
    print('ok')
