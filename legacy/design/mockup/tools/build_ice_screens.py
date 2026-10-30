"""Ледяной стиль для остальных экранов: Обновления, Поиск, Окно загрузки (2), Настройки (3).
Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_ice_screens.py
Сначала подключается build_ice (он подменяет панель слева, поверхности и линзы), потом build_screens."""
import build_ice as I
import build_ui as U
import build_screens as S

CARD_H = 286

# вторичный текст на стекле не бледнее 0,9: перечисляем все встречающиеся оттенки белого
_pats = []
for n in range(50, 88):
    _pats += [f'.glass [style*="color:rgba(255,255,255,.{n})"]', f'.glass [style*="color:rgba(255,255,255,0.{n})"]']
SCR_CSS = ',\n'.join(_pats) + '{color:rgba(255,255,255,.90) !important}\n' + '''
.poster .nm2{font-size:15px;font-weight:620;color:#fff}
.chip.inl{position:static;display:inline-flex;align-self:flex-start;margin-bottom:8px;height:24px}
.pcard{position:relative;border-radius:26px;box-sizing:border-box;overflow:hidden;background:#1d2533;
 box-shadow:0 16px 32px rgba(4,8,20,.50),0 3px 8px rgba(4,8,20,.35);transition:transform var(--mid) var(--ease),box-shadow var(--mid) var(--ease)}
.pcard::before{content:"";position:absolute;inset:0;border-radius:inherit;pointer-events:none;z-index:4;
 box-shadow:inset 0 0 0 1px rgba(255,255,255,.22),inset 0 1.5px 0 rgba(255,255,255,.55),inset 1.5px 0 0 rgba(255,255,255,.18),inset 0 -1px 0 rgba(255,255,255,.12)}
.pcard:hover{transform:translateY(-4px);box-shadow:0 26px 44px rgba(4,8,20,.58),0 0 34px rgba(170,205,255,.20),0 3px 8px rgba(4,8,20,.35)}
.pcard .ph{position:absolute;left:0;top:0;bottom:0;width:560px;background-size:cover;background-position:center}
.pcard .ph::after{display:none}
.fadeR{position:absolute;inset:0;pointer-events:none;background:linear-gradient(90deg,rgba(8,14,28,0) 0,rgba(8,14,28,0) 24%,rgba(8,14,28,.86) 36%,rgba(8,14,28,.94) 46%,rgba(8,14,28,.96) 100%)}
.pcard .body{position:absolute;left:372px;right:22px;top:0;bottom:0;display:flex;flex-direction:column;justify-content:center;z-index:3;text-shadow:0 1px 8px rgba(4,8,20,.75)}
.pcard.gray .ph{filter:grayscale(1) brightness(.8)}
.pcard .btn{text-shadow:none}
.scard .ph{position:absolute;inset:0;background-size:cover;background-position:center}
.scard .ph::after{display:none}
.scard .fade{height:62%}
.scard .info{height:auto;bottom:16px}
.pbtn{position:relative}
'''


def _crop(crop):
    if not crop:
        return ''
    d = dict(p.split(':') for p in crop.split(';'))
    return f'background-size:{d["size"]};background-position:{d["pos"]};'


def chip_dot(color, text):
    return f'<span class="chip dk inl">{I.DOT.format(c=color)}{text}</span>'


# ------------------------------------------------------------- Обновления: окно-фото слева, текст справа на тёмном
def poster(y, img, name, old, new, why, meta, status, crop=''):
    chip = {'update': '', 'maybe': chip_dot('#ffd36b', 'Возможно, обновлён'), 'lost': chip_dot('#ffb340', 'Страница пропала')}[status]
    if new:
        change = (f'<div style="font-size:26px;font-weight:700;letter-spacing:-0.015em;line-height:1.1"><span style="color:rgba(255,255,255,.78);font-weight:500;text-decoration:line-through">{old}</span>'
                  f'<span style="color:rgba(255,255,255,.84);font-weight:500"> → </span>{new}</div>')
    else:
        change = ''
    btns = {'update': S.primary('Обновить') + S.link('Открыть страницу') + S.link('Пропустить эту версию', 'rgba(255,255,255,.88)', w=500),
            'maybe': S.primary('Открыть страницу') + S.link('Версия не изменилась', 'rgba(255,255,255,.88)', w=500),
            'lost': S.primary('Найти замену') + S.link('Не проверять этот мод', 'rgba(255,255,255,.88)', w=500)}[status]
    gray = ' gray' if status == 'lost' else ''
    return (f'<article class="card" style="position:absolute;left:116px;top:{y}px;width:964px;animation-delay:{120 + (y // 200) * 70}ms">'
            f'<div class="pcard{gray}" style="height:196px"><div class="ph pic p-{img}" style="{_crop(crop)}"></div><div class="fadeR"></div>'
            f'<div class="body">{chip}<div style="font-size:15px;color:rgba(255,255,255,.92);font-weight:600;margin-bottom:3px">{name}</div>{change}'
            f'<div style="margin-top:8px;font-size:13.5px;color:#fff">{why}</div><div style="margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.88)">{meta}</div>'
            f'<div style="display:flex;align-items:center;gap:22px;margin-top:14px">{btns}</div></div></div></article>\n')


# ------------------------------------------------------------- Поиск: карточка-окно с кнопками на градиенте
def stile(x, y, pos, img, name, sub, price, action, crop):
    chip = {'free': chip_dot('#30d158', 'Бесплатно'), 'paid': chip_dot('#ffb340', 'Платный'), 'unknown': chip_dot('#c9d3e2', 'Цена не указана')}[price]
    chip = chip.replace('chip dk inl', 'chip dk')
    btn = (S.primary(action, 34, 13.5) if action else
           '<button type="button" class="btn" style="height:34px;padding:0 18px;border:1.5px solid #ffc15e;border-radius:17px;background:rgba(8,14,28,.5);color:#ffc15e;font-size:13.5px;font-weight:650;cursor:pointer;white-space:nowrap;text-shadow:none">Купить у автора</button>')
    return (f'<article class="card" style="position:absolute;left:{x}px;top:{y}px;width:308px;animation-delay:{80 + pos * 55}ms">'
            f'<div class="cd scard" style="height:{CARD_H}px"><div class="pw"><div class="ph pic p-{img}" style="{_crop(crop)}"></div><div class="scrim"></div><div class="fade"></div></div>{chip}'
            f'<div class="info"><div class="nm">{name}</div><div class="sb">{sub}</div>'
            f'<div style="display:flex;align-items:center;gap:18px;margin-top:12px">{btn}{S.link("Страница", w=600)}</div></div></div></article>\n')


# ------------------------------------------------------------- Настройки: внешний вид (стекло, иней, фон, движение по отдельности)
I.SIZES['set_a'] = (1308, 760)
I.GLINTS['set_a'] = 14

APP_CSS = '''
.o{transition:opacity var(--mid) var(--ease)}
.seg input:checked + span{background:rgba(255,255,255,.16) !important;box-shadow:inset 0 0 0 1px rgba(255,255,255,.35),0 0 16px rgba(170,205,255,.22)}
.opt-row{display:flex;align-items:center;gap:16px;padding:12px 16px;border-radius:18px;background:rgba(255,255,255,.09)}
.grp{font-size:13px;font-weight:650;color:rgba(255,255,255,.92);padding:0 4px 8px;letter-spacing:.005em}
.rng{-webkit-appearance:none;appearance:none;width:150px;height:6px;border-radius:3px;background:rgba(255,255,255,.30);outline:0;margin:0;cursor:pointer}
.rng::-webkit-slider-thumb{-webkit-appearance:none;width:22px;height:22px;border-radius:50%;background:#fff;box-shadow:0 2px 8px rgba(6,10,22,.5),0 0 14px rgba(170,205,255,.4)}
.rng:focus-visible{outline:3px solid #fff;outline-offset:6px}
.pset{display:flex;flex-direction:column;gap:2px;padding:9px 14px;border-radius:16px;cursor:pointer;background:rgba(255,255,255,.09);box-shadow:inset 0 0 0 1px rgba(255,255,255,.08)}
.pset b{font-size:14px;font-weight:620}.pset span{font-size:12px;color:rgba(255,255,255,.88)}
.psetin{position:absolute;opacity:0;pointer-events:none}
.psetin:checked + .pset{background:rgba(255,255,255,.2);box-shadow:inset 0 0 0 1.5px rgba(255,255,255,.85),0 0 22px rgba(170,205,255,.3)}
.psetin:focus-visible + .pset{outline:3px solid #fff;outline-offset:2px}
.pvice{position:absolute;inset:0;border-radius:28px;background-size:100% 100%;pointer-events:none;opacity:.5;transition:opacity var(--mid)}
.pvcol{position:absolute;inset:0;background:linear-gradient(160deg,#2b3752,#141b2c);opacity:0;transition:opacity var(--mid)}
/* зависимости: подчинённые пункты тускнеют */
html:has(#ic:not(:checked)) .dep-ic{opacity:.4;pointer-events:none}
html:has(#bg2:checked) .dep-bg{opacity:.4;pointer-events:none}
/* живой отклик самой страницы на переключатели */
html:has(#ic:not(:checked)) .ice,html:has(#ic:not(:checked)) .pvice,html:has(#ic:not(:checked)) .gl{opacity:0 !important}
html:has(#sp:not(:checked)) .gl{display:none}
html:has(#ics0:checked){--k:.5}
html:has(#ics2:checked){--k:1.8}
html:has(#ics0:checked) .pvice{opacity:.3}
html:has(#ics1:checked) .pvice{opacity:.5}
html:has(#ics2:checked) .pvice{opacity:.9}
html:has(#lg:not(:checked)) .glass{-webkit-backdrop-filter:blur(26px) saturate(1.15) !important;backdrop-filter:blur(26px) saturate(1.15) !important}
html:has(#lg:not(:checked)) .pv-on{opacity:0}
html:has(#lg:checked) .pv-off{opacity:0}
html:has(#bg1:checked) .wall{filter:blur(18px) saturate(1.1)}
html:has(#bg2:checked) .wall,html:has(#bg2:checked) .blob,html:has(#bg2:checked) .grade{opacity:0}
html:has(#bg2:checked) .pvwall{opacity:0}
html:has(#bg2:checked) .pvcol{opacity:1}
html:has(#an:not(:checked)) .b1,html:has(#an:not(:checked)) .b2,html:has(#an:not(:checked)) .b3,html:has(#an:not(:checked)) .wall,html:has(#an:not(:checked)) .grade{animation:none !important}
html:has(#fl:not(:checked)) .fly{display:none}
html:has(#mo:not(:checked)) *{animation:none !important;transition:none !important}
'''


def trow(tid, on, title, text, dep='', extra=''):
    return (f'<div class="o {dep}"><input id="{tid}" class="tgl" type="checkbox" {"checked" if on else ""}>'
            f'<label for="{tid}" class="trow" style="gap:16px;padding:12px 16px;border-radius:18px;background:rgba(255,255,255,.09)">'
            f'<span style="flex:1"><span style="display:block;font-size:15px;font-weight:620">{title}</span>'
            f'<span style="display:block;margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.88);line-height:1.35">{text}</span></span>'
            f'<span class="trk"><i></i></span></label></div>')


def segrow(name, label, text, items, checked, dep=''):
    segs = ''.join(f'<label class="seg"><input type="radio" name="{name}" id="{name}{i}" {"checked" if i == checked else ""}><span>{t}</span></label>' for i, t in enumerate(items))
    return (f'<div class="o opt-row {dep}"><span style="flex:1"><span style="display:block;font-size:15px;font-weight:620">{label}</span>'
            f'<span style="display:block;margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.88);line-height:1.35">{text}</span></span>'
            f'<div role="radiogroup" aria-label="{label}" style="display:flex;gap:2px;padding:3px;border-radius:19px;background:rgba(255,255,255,0.10)">{segs}</div></div>')


def group(title, *rows):
    return f'<div><div class="grp">{title}</div><div style="display:flex;flex-direction:column;gap:8px">{"".join(rows)}</div></div>'


def appearance_preview():
    def bar(on):
        f = 'blur(5px) url(#lgPv) saturate(1.4)' if on else 'blur(26px) saturate(1.15)'
        tint = 'rgba(12,20,38,.74)' if on else 'rgba(34,46,68,0.66)'
        sh = 'inset 0 0 0 1px rgba(255,255,255,0.14)' if on else 'inset 0 0 0 1px rgba(255,255,255,0.16), inset 0 1px 0 rgba(255,255,255,0.26)'
        r = f'<div class="rim" style="background-image:url({U.A("pv_rim.png")})"></div>' if on else ''
        return (f'<div style="position:absolute;left:24px;top:22px;width:410px;height:56px;border-radius:28px;background:{tint};box-shadow:{sh};-webkit-backdrop-filter:{f};backdrop-filter:{f};overflow:visible">{r}'
                f'<div class="pvice" style="background-image:url({U.A("ice_pv.png")})"></div>'
                f'<div style="position:relative;display:flex;align-items:center;gap:12px;height:100%;padding:0 22px;font-size:14px"><b style="font-weight:650">Гараж</b>'
                f'<span style="color:rgba(255,255,255,.9);font-size:13px">128 модов</span><span style="flex:1"></span><span style="padding:5px 12px;border-radius:15px;background:rgba(255,255,255,.12);font-size:13px;font-weight:650;color:#fff">Все</span></div></div>')
    return (f'<div><div class="grp">Как это выглядит</div><div style="position:relative;height:172px;border-radius:22px;overflow:hidden;box-shadow:inset 0 0 0 1px rgba(255,255,255,.14)">'
            f'<div class="pic p-wall pvwall" style="position:absolute;inset:0;background-position:center 70%;transition:opacity var(--mid)"></div><div class="pvcol"></div>'
            f'<div class="pic p-mustang" style="position:absolute;left:30px;top:56px;width:250px;height:140px;border-radius:16px"></div>'
            f'<div class="pic p-lambo" style="position:absolute;left:292px;top:56px;width:200px;height:140px;border-radius:16px"></div>'
            f'<div class="pv pv-on" style="inset:0">{bar(True)}</div><div class="pv pv-off" style="inset:0">{bar(False)}</div></div></div>')


def appearance():
    H = 760
    filters = (U.lens('lgRail', 'rail', 84, 212, 70) + U.lens('lgGear', 'gear', 56, 56, 56) + U.lens('lgSet', 'set_a', 1308, H, 96) + U.lens('lgPv', 'pv', 420, 56, 80))
    nav = ''.join(
        f'<a class="nv" href="#" style="display:flex;align-items:center;height:42px;padding:0 16px;border-radius:21px;color:{"#fff" if t == "Внешний вид" else "rgba(255,255,255,.88)"};text-decoration:none;font-size:14.5px;font-weight:{650 if t == "Внешний вид" else 500};'
        f'background:{"rgba(255,255,255,.20);box-shadow:0 0 22px rgba(170,205,255,.3)" if t == "Внешний вид" else "transparent"}">{t}</a>' for t in S.SECTIONS)
    presets = [('Лёд', 'стекло, иней, фон'), ('Стекло', 'без инея'), ('Матовое', 'без преломления'), ('Лёгкое', 'ничего лишнего')]
    pset = ''.join(f'<div><input class="psetin" type="radio" name="ps" id="ps{i}" {"checked" if i == 0 else ""}><label class="pset" for="ps{i}"><b>{t}</b><span>{n}</span></label></div>' for i, (t, n) in enumerate(presets))
    left = (appearance_preview()
            + f'<div><div class="grp">Готовые наборы</div><div style="display:grid;grid-template-columns:1fr 1fr;gap:8px">{pset}</div>'
              f'<div style="margin-top:8px;font-size:12.5px;color:rgba(255,255,255,.88);line-height:1.4;padding:0 4px">Набор выставляет переключатели справа. Дальше каждый можно поменять отдельно.</div></div>'
            + group('Движение',
                    trow('mo', True, 'Анимации интерфейса', 'Капля-выбор, появление и подъём карточек.'),
                    trow('an', True, 'Анимированный фон', 'Фото медленно меняет цвет.', 'dep-bg'),
                    trow('fl', True, 'Огоньки', 'Над фоном плавают светлые точки.', 'dep-bg')))
    right = (group('Стекло',
                   trow('lg', True, 'Жидкое стекло', 'Края панелей преломляют фон. Выключено: обычное матовое стекло.'),
                   trow('ic', True, 'Иней на стекле', 'Морозный узор по краям панелей и на карточках-состояниях.'),
                   segrow('ics', 'Сила инея', 'От лёгкой дымки до густого мороза.', ['Слабо', 'Средне', 'Сильно'], 1, 'dep-ic'),
                   trow('sp', True, 'Искры льда', 'Редкие блики на кромках.', 'dep-ic'))
             + group('Фон',
                     segrow('bg', 'Что за панелями', 'Фото, размытое фото или один цвет.', ['Фото', 'Размытое', 'Цвет'], 0),
                     f'<div class="o opt-row dep-bg"><span style="flex:1"><span style="display:block;font-size:15px;font-weight:620">Затемнение фона</span>'
                     f'<span style="display:block;margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.88)">Темнее фон: проще читать.</span></span><input class="rng" type="range" min="0" max="100" value="35" aria-label="Затемнение фона"></div>')
             + f'<div style="display:flex;align-items:center;justify-content:space-between;padding:6px 4px 0"><span style="font-size:12.5px;color:rgba(255,255,255,.88);max-width:330px;line-height:1.4">Если в Windows включено «Уменьшить анимацию» или «Уменьшить прозрачность», программа учтёт это сама.</span>{S.link("Сбросить", w=650)}</div>')
    body = U.flies() + U.chrome(True, 'settings')
    body += (f'<section class="glass top" aria-label="Настройки" style="left:116px;top:16px;width:1308px;height:{H}px;border-radius:32px;background:rgba(24,34,52,.44);{U.surface(True, "Set", 14)}">{U.rim(True, "set_a")}'
             f'<div style="position:relative;display:flex;height:100%;padding:36px 40px 32px 36px;box-sizing:border-box;gap:36px">'
             f'<div style="flex:0 0 220px"><div style="font-size:28px;font-weight:700;letter-spacing:-0.02em;margin:0 0 24px 16px">Настройки</div><nav style="display:flex;flex-direction:column;gap:4px" aria-label="Разделы настроек">{nav}</nav></div>'
             f'<div style="flex:1;min-width:0"><div style="font-size:22px;font-weight:700;letter-spacing:-0.01em;margin:6px 0 18px">Внешний вид</div>'
             f'<div style="display:flex;gap:28px"><div style="flex:0 0 440px;display:flex;flex-direction:column;gap:16px">{left}</div><div style="flex:1;min-width:0;display:flex;flex-direction:column;gap:18px">{right}</div></div></div></div></section>\n')
    return U.page('Настройки: внешний вид', filters, body, True)


S.poster = poster
S.stile = stile

JOBS = {
    'UpdatesIce': ('Обновления: ледяное стекло', lambda: S.updates(True)),
    'SearchIce': ('Поиск: ледяное стекло', lambda: S.search(True)),
    'DownloadIce': ('Окно загрузки: ледяное стекло', lambda: S.download(False)),
    'DownloadCheckedIce': ('Окно загрузки, архив проверен: ледяное стекло', lambda: S.download(True)),
    'SettingsIce': ('Настройки: внешний вид, ледяное стекло', appearance),
    'SettingsDownloadIce': ('Настройки: загрузка модов, ледяное стекло', lambda: S.settings_page('Загрузка модов')),
    'SettingsSitesIce': ('Настройки: сайты, ледяное стекло', lambda: S.settings_page('Сайты')),
}


def build_all(only=None):
    out = {}
    for name, (title, fn) in JOBS.items():
        if only and name not in only:
            continue
        html = I.finish(fn(), title=title)
        html = html.replace('</style>', SCR_CSS + APP_CSS + '</style>', 1)
        html = html.replace('background:#2f6bff', 'background:#2459e6').replace('rgba(30,40,58,0.26)', 'rgba(12,20,38,.70)').replace('background:rgba(255,255,255,.22);font-size:13px;font-weight:650', 'background:rgba(255,255,255,.12);font-size:13px;font-weight:650;color:#fff')
        out[name] = html
        open(f'{U.OUT}/{name}.dc.html', 'w').write(html)
    return out


if __name__ == '__main__':
    import sys
    r = build_all(sys.argv[1:] or None)
    for n, h in r.items():
        print(n, len(h))
