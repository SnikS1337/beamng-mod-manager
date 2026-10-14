"""Доска «Гараж: ледяное стекло» — вариант главного экрана, где к жидкому стеклу добавлен иней по краям,
холодный оттенок и искры льда. Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_ice.py"""
import random
import build_ui as U

_rim = U.rim
_surface = U.surface
SIZES = {'rail': (84, 284), 'gear': (56, 56), 'bar': (964, 52), 'insp': (328, 928), 'chk': (328, 540), 'set': (1308, 640), 'set_d': (1308, 470), 'set_s': (1308, 560), 'dl': (1100, 700)}
GLINTS = {'rail': 6, 'gear': 2, 'bar': 8, 'insp': 12, 'chk': 8, 'set': 12, 'set_d': 10, 'set_s': 10, 'dl': 12}


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
    base = _rim(glass, 'rail4' if name == 'rail' else name)
    if glass and name in SIZES:
        n = GLINTS[name]
        return base + f'<div class="ice" style="background-image:url({U.A("ice_" + name + ".png")})">{glints(name, n, hash(name) % 97)}</div>'
    return base


def surface(glass, name, blur, scale=None):
    # настоящее Liquid Glass почти не размывает: видно картинку, а не пятно; смысл даёт преломление
    css = _surface(glass, name, {'Insp': 5, 'Chk': 5, 'Set': 4, 'Dl': 4, 'Bar': 3}.get(name, 2), scale)
    dim = {'Insp': ' brightness(.70) contrast(.9)', 'Bar': ' brightness(.60) contrast(.9)', 'Rail': ' brightness(.78)', 'Chk': ' brightness(.70)', 'Set': ' brightness(.66)', 'Dl': ' brightness(.66)'}.get(name, '')
    return css.replace('saturate(1.4)', 'saturate(1.4)' + dim)


_lens = U.lens
def lens(fid, name, w, h, scale=46):
    if fid == 'lgRail':
        name, h = 'rail4', 284
    return _lens(fid, name, w, h, int(scale * 1.5))   # линза заметнее: контент изгибается у краёв
U.lens = lens


U.rim = rim
U.surface = surface

EXTRA = '''
.glass{background:rgba(14,24,44,.44) !important}
.glass.top{background:rgba(14,24,44,.44) !important;text-shadow:0 1px 8px rgba(4,8,18,.8),0 0 2px rgba(4,8,18,.6)}
.glass{text-shadow:0 1px 6px rgba(6,12,24,.50)}
.glass .cta{text-shadow:none}
/* читаемость: второстепенный текст на стекле не бледнее 0,84 */
.sb{color:rgba(255,255,255,.88) !important}
.glass [style*="rgba(255,255,255,0.64)"],.glass [style*="rgba(255,255,255,0.66)"],.glass [style*="rgba(255,255,255,0.7)"],.glass [style*="rgba(255,255,255,0.72)"],.glass [style*="rgba(255,255,255,.7)"],.glass [style*="rgba(255,255,255,.6)"]{color:rgba(255,255,255,.90) !important}
.glass.insp{background:rgba(14,24,44,.50) !important;text-shadow:0 1px 8px rgba(4,8,18,.7)}
.glass.insp .ice{opacity:.16}
.glass [style*="background:rgba(255,255,255,0.09)"],.glass [style*="background:rgba(255,255,255,.09)"]{background:rgba(8,14,28,.34) !important;box-shadow:inset 0 0 0 1px rgba(255,255,255,.08),inset 0 1px 0 rgba(255,255,255,.14)}
[aria-label="Настройки"] .ice,[aria-label="Скачивание"] .ice{opacity:.13}
.glass .ice{mix-blend-mode:normal}
.glass.top .ice{opacity:.34}
.hfade{position:absolute;left:104px;top:0;width:988px;height:98px;z-index:0;pointer-events:none;background:linear-gradient(180deg,rgba(8,14,28,.58) 0,rgba(8,14,28,.58) 52%,rgba(8,14,28,0) 100%);-webkit-mask-image:linear-gradient(90deg,transparent,#000 48px,#000 calc(100% - 48px),transparent);mask-image:linear-gradient(90deg,transparent,#000 48px,#000 calc(100% - 48px),transparent)}
.ice{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-size:100% 100%;opacity:.55}
.gl{position:absolute;opacity:0;pointer-events:none;background:
 linear-gradient(90deg,transparent 0,rgba(235,246,255,.7) 50%,transparent 100%) center/100% 1.5px no-repeat,
 linear-gradient(0deg,transparent 0,rgba(235,246,255,.7) 50%,transparent 100%) center/1.5px 100% no-repeat,
 radial-gradient(circle,rgba(255,255,255,.95) 0,rgba(210,232,255,.0) 55%);animation:glint var(--beat) ease-in-out infinite}
@keyframes glint{0%,70%,100%{opacity:0;transform:scale(.4) rotate(0)}82%{opacity:1;transform:scale(1) rotate(12deg)}92%{opacity:.25;transform:scale(.7) rotate(20deg)}}
.plate{box-shadow:0 12px 32px rgba(6,10,22,.38), inset 0 0 0 1px rgba(205,230,255,.20), inset 0 1px 0 rgba(235,246,255,.34), inset 0 0 26px rgba(170,205,255,.07) !important}
@media (prefers-reduced-motion:reduce){.gl{animation:none;opacity:.55}}
'''


W_SEG = [52, 112, 148, 152]            # ширины пунктов главного экрана (запас на жирный шрифт)
LABELS = [('Все', None), ('Включённые', None), ('Нужно обновить', '3'), ('Не проверяются', '12')]
_WCACHE = {}


def measure(labels):
    """Ширины пунктов (px) для капли: меряем настоящий текст жирным шрифтом в браузере."""
    key = tuple(labels)
    if key not in _WCACHE:
        from playwright.sync_api import sync_playwright
        html = '<body style="font-family:%s;font-size:13px;font-weight:650">' % U.FONT + ''.join(
            f'<span id="m{i}" style="display:inline-block;white-space:nowrap;padding:0 14px">{t}{(" <b style=margin-left:5px>" + c + "</b>") if c else ""}</span><br>' for i, (t, c) in enumerate(labels))
        with sync_playwright() as p:
            br = p.chromium.launch(executable_path='/opt/pw-browsers/chromium')
            pg = br.new_page(); pg.set_content(html)
            w = [int(pg.evaluate(f"document.getElementById('m{i}').getBoundingClientRect().width")) + 8 for i in range(len(labels))]
            br.close()
        _WCACHE[key] = w
    return _WCACHE[key]
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


def segmented(labels=None):
    W_SEG_ = W_SEG if labels is None else measure(labels)
    LABELS_ = LABELS if labels is None else labels
    lefts, x = [], TRK
    for w in W_SEG_:
        lefts.append(x - OVER); x += w
    labs = ''.join(
        f'<label><input type="radio" name="f" id="f{i}" {"checked" if i == 0 else ""}><span style="width:{W_SEG_[i]}px">{t}'
        f'{(" <b style=\"opacity:.7;font-weight:600;margin-left:5px\">" + c + "</b>") if c else ""}</span></label>' for i, (t, c) in enumerate(LABELS_))
    rules = ''.join(f'.sg:has(#f{i}:checked) .drop{{left:{lefts[i]}px;width:{W_SEG_[i] + 2 * OVER}px}}\n' for i in range(len(LABELS_)))
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


NAV = [('grid', 'Гараж', 14, None), ('upd', 'Обновления', 76, '3'), ('search', 'Поиск', 138, None), ('gear', 'Настройки', 212, None)]
RAIL_H = 284


ACTIVE = {'garage': 0, 'updates': 1, 'search': 2, 'settings': 3}


def chrome(glass, active):
    """Левая панель: четыре раздела в одной капсуле, одна подвижная капля; настройки отделены тонкой чертой."""
    items = ''
    for i, (k, label, top, badge) in enumerate(NAV):
        bd = (f'<span class="pulse" style="position:absolute;top:3px;right:6px;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;border-radius:9px;background:#fff;color:#111;'
              f'font-size:11px;font-weight:700;display:flex;align-items:center;justify-content:center">{badge}</span>') if badge else ''
        items += (f'<label class="rn" style="top:{top}px"><input type="radio" name="nv" id="n{i}" {"checked" if i == ACTIVE.get(active, 0) else ""} aria-label="{label}">'
                  f'{U.icon(k, 22, 1.7)}<span>{label}</span>{bd}</label>')
    rules = ''.join(f'.lft:has(#n{i}:checked) .ndrop{{top:{t}px}}\n' for i, (_, _, t, _) in enumerate(NAV))
    first = NAV[ACTIVE.get(active, 0)][2]
    rules += f'.ndrop{{top:{first}px}}\n'
    html = (f'<nav class="glass lft" aria-label="Разделы" style="left:16px;top:16px;width:84px;height:{RAIL_H}px;border-radius:36px;{U.surface(glass, "Rail", 7)}">'
            f'{U.rim(glass, "rail")}<div style="position:absolute;inset:0"><i class="ndrop"></i>{items}<i class="hr" style="top:200px"></i></div></nav>\n')
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
'''


# ---------------------------------------------------------------- Карточка мода: фото-окно со стеклянной подписью
SNOW = ('<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">'
        '<path d="M12 2v20M3.3 7l17.4 10M3.3 17L20.7 7"/></svg>')
DOT = '<i style="display:inline-block;width:7px;height:7px;border-radius:50%;background:{c};margin-right:7px;flex:0 0 auto"></i>'


def tile(x, y, w, pos, img, n, sub, status, crop, sel, cap=True):
    extra = ''
    if crop:
        d = dict(p.split(':') for p in crop.split(';'))
        extra = f'background-size:{d["size"]};background-position:{d["pos"]};'
    if not cap:
        status = None if status != 'off' else 'off'
    chip = {
        'update': f'<span class="chip dk up pulse">{DOT.format(c="#8fd0ff")}Есть обновление</span>',
        'lost': f'<span class="chip dk">{DOT.format(c="#ffb340")}Страница пропала</span>',
        'maybe': f'<span class="chip dk">{DOT.format(c="#ffd36b")}Возможно, обновлён</span>',
        'off': f'<span class="chip dk">{SNOW}<span style="margin-left:6px">Выключен</span></span>',
    }.get(status, '')
    off = status == 'off'
    subh = f'<b style="color:#fff;font-weight:650">{sub}</b>' if status == 'update' else sub
    frost = f'<div class="frost" style="background-image:url({U.A("ice_card.png")})"></div>' if off else ''
    info = (f'<div class="info"><div class="nm">{n}</div><div class="sb">{subh}</div></div>') if cap else ''
    return (f'<article class="card" style="position:absolute;left:{x}px;top:{y}px;width:{w}px;animation-delay:{60 + pos * 45}ms">'
            f'<div class="cd{" sel" if sel else ""}{" off" if off else ""}{"" if cap else " under"}" style="height:{U.PLATE_H}px" tabindex="0">'
            f'<div class="pw"><div class="ph pic p-{img}" style="{extra}"></div>{frost}<div class="scrim"></div><div class="fade"></div></div>{chip if cap else ''}{info}</div></article>\n')


U.tile = tile

CARD_CSS = '''
.card{animation:rise .8s cubic-bezier(.2,.8,.2,1) both}
.cd{position:relative;border-radius:26px;box-sizing:border-box;cursor:pointer;
 box-shadow:0 16px 32px rgba(4,8,20,.50),0 3px 8px rgba(4,8,20,.35);transition:transform var(--mid) var(--ease),box-shadow var(--mid) var(--ease)}
.cd::before{content:"";position:absolute;inset:0;border-radius:inherit;pointer-events:none;z-index:4;
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.22),inset 0 1.5px 0 rgba(255,255,255,.55),inset 1.5px 0 0 rgba(255,255,255,.18),inset 0 -1px 0 rgba(255,255,255,.12)}
.pw{position:absolute;inset:0;border-radius:inherit;overflow:hidden;background:#1d2533}
.pw .ph{position:absolute;inset:0;background-size:cover;background-position:center;transition:transform 1.2s var(--ease),filter var(--mid)}
.pw .ph::after{display:none}
.scrim{position:absolute;inset:0;background:linear-gradient(180deg,rgba(8,14,28,.18) 0%,rgba(8,14,28,0) 26%,rgba(8,14,28,0) 52%,rgba(8,14,28,.62) 100%)}
.cd:hover{transform:translateY(-5px);box-shadow:0 26px 44px rgba(4,8,20,.58),0 0 34px rgba(170,205,255,.20),0 3px 8px rgba(4,8,20,.35)}
.cd:hover .ph{transform:scale(1.06)}
.fade{position:absolute;left:0;right:0;bottom:0;height:50%;pointer-events:none;background:linear-gradient(180deg,rgba(8,14,28,0) 0,rgba(8,14,28,.66) 34%,rgba(8,14,28,.90) 100%)}
.info{position:absolute;left:20px;right:20px;bottom:14px;height:50px;box-sizing:border-box;z-index:3;text-shadow:0 1px 8px rgba(4,8,20,.75)}
.nm{font-size:15px;line-height:19px;font-weight:620;letter-spacing:-.005em;color:#fff;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.sb{margin-top:1px;font-size:12.5px;line-height:17px;color:rgba(255,255,255,.74);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.chip{position:absolute;top:12px;right:12px;z-index:3;display:inline-flex;align-items:center;height:26px;padding:0 11px;border-radius:13px;font-size:12px;font-weight:700;letter-spacing:.005em}
.chip.wh{background:rgba(255,255,255,.95);color:#14181f;box-shadow:0 4px 12px rgba(4,8,20,.35)}
.chip.dk.up{box-shadow:inset 0 0 0 1px rgba(143,208,255,.55),0 4px 12px rgba(4,8,20,.3)}
.chip.dk{background:rgba(10,16,30,.80);color:#fff;font-weight:600;-webkit-backdrop-filter:blur(8px);backdrop-filter:blur(8px);box-shadow:inset 0 0 0 1px rgba(255,255,255,.22),inset 0 1px 0 rgba(255,255,255,.4),0 4px 12px rgba(4,8,20,.3)}
.cd.sel{transform:translateY(-3px);animation:selglow var(--beat) ease-in-out infinite}
.cd.sel::after{content:"";position:absolute;inset:-4px;border-radius:30px;padding:2.5px;pointer-events:none;z-index:5;
 background:conic-gradient(from 200deg,rgba(255,255,255,.95),rgba(120,205,255,.95) 16%,rgba(255,255,255,.9) 34%,rgba(255,255,255,.75) 55%,rgba(255,115,205,.9) 72%,rgba(255,255,255,.95) 90%,rgba(255,255,255,.95));
 -webkit-mask:linear-gradient(#000 0 0) content-box,linear-gradient(#000 0 0);-webkit-mask-composite:xor;mask-composite:exclude}
@keyframes selglow{0%,100%{box-shadow:0 18px 34px rgba(4,8,20,.5),0 0 26px rgba(140,195,255,.32)}50%{box-shadow:0 22px 40px rgba(4,8,20,.5),0 0 52px rgba(140,195,255,.62)}}
.cd.off .ph{filter:saturate(.25) brightness(.9) contrast(1.05)}
.cd.off .pw::after{content:"";position:absolute;inset:0;background:linear-gradient(180deg,rgba(150,195,235,.30),rgba(120,165,215,.20))}
.frost{position:absolute;inset:0;background-size:100% 100%;z-index:1;opacity:.95}
.cd.off .scrim,.cd.off .fade{z-index:2}
@media (prefers-reduced-motion:reduce){.cd.sel{animation:none}}
.cd.under{opacity:.5;filter:saturate(.7)}
'''


def finish(html, rules='', title=None):
    """Общая одежда ледяного стиля для любого экрана: капля вместо фильтров, фильтр линзы, стили."""
    import re
    m = re.search(r'<div role="radiogroup" aria-label="Фильтр".*?</div>(?=<label style)', html, flags=re.S)
    if m:
        labels = [(t.strip(), c) for t, c in re.findall(r'<span>([^<]*?)(?: <b[^>]*>([^<]*)</b>)?</span>', m.group(0))]
        labels = [(t, c or None) for t, c in labels]
        seg, r2 = segmented(labels)
        html = html.replace(m.group(0), seg, 1); rules += r2
    html = html.replace('</defs></svg>', drop_filter().replace('id="dpn"', 'id="lgdpn"') + '</defs></svg>', 1)
    if title:
        html = re.sub(r'<title>.*?</title>', f'<title>{title}</title>', html, count=1)
    return html.replace('</style>', EXTRA + DROP_CSS + RAIL_CSS + CARD_CSS + rules + '</style>', 1)


def build():
    import re
    html = U.garage(True)
    html = html.replace('<header class="glass lq top"', '<div class="hfade"></div><header class="glass lq top"', 1)
    assert 'class="hfade"' in html
    html = html.replace('>Машина</span>', '>Состав</span>', 1).replace('font-weight:500">Dacia Sandero Stepway</span>', 'font-weight:500">Машина, 3 цвета</span>', 1)
    html = html.replace('142 МБ, на сайте 151 МБ', '142 МБ <span style="opacity:.82;font-weight:450">(на сайте 151)</span>', 1)
    btn = 'white-space:nowrap;padding:0 6px;height:38px;border-radius:19px;border:0;cursor:pointer;color:#fff;font-size:13px;font-weight:600;background:rgba(255,255,255,.14);box-shadow:inset 0 0 0 1px rgba(255,255,255,.20),inset 0 1px 0 rgba(255,255,255,.28)'
    tail = (f'<div style="margin-top:auto;display:flex;flex-direction:column;gap:12px"><div style="display:flex;gap:10px">'
            f'<button type="button" style="flex:1;{btn}">Открыть страницу</button><button type="button" style="flex:1;{btn}">Выключить</button></div>'
            f'<div style="display:flex;align-items:center;justify-content:space-between;padding:0 6px"><button type="button" style="padding:0;border:0;background:transparent;color:#ffb8b0;font-size:13.5px;font-weight:650;cursor:pointer">Удалить</button>'
            f'<span style="font-size:12.5px;color:rgba(255,255,255,.88)">Можно вернуть позже</span></div></div></div></aside>')
    html, n = re.subn(r'<div style="margin-top:auto;display:flex;flex-wrap.*?</span></div></div></aside>', lambda m: tail, html, count=1, flags=re.S)
    assert n == 1, 'низ инспектора не найден'
    return finish(html, title='Гараж: ледяное стекло')


if __name__ == '__main__':
    open(f'{U.OUT}/GarageIce.dc.html', 'w').write(build())
    print('ok')
