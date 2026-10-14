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


S.poster = poster
S.stile = stile

JOBS = {
    'UpdatesIce': ('Обновления: ледяное стекло', lambda: S.updates(True)),
    'SearchIce': ('Поиск: ледяное стекло', lambda: S.search(True)),
    'DownloadIce': ('Окно загрузки: ледяное стекло', lambda: S.download(False)),
    'DownloadCheckedIce': ('Окно загрузки, архив проверен: ледяное стекло', lambda: S.download(True)),
    'SettingsIce': ('Настройки: внешний вид, ледяное стекло', lambda: S.settings_page('Внешний вид')),
    'SettingsDownloadIce': ('Настройки: загрузка модов, ледяное стекло', lambda: S.settings_page('Загрузка модов')),
    'SettingsSitesIce': ('Настройки: сайты, ледяное стекло', lambda: S.settings_page('Сайты')),
}


def build_all(only=None):
    out = {}
    for name, (title, fn) in JOBS.items():
        if only and name not in only:
            continue
        html = I.finish(fn(), title=title)
        html = html.replace('</style>', SCR_CSS + '</style>', 1)
        html = html.replace('background:#2f6bff', 'background:#2459e6').replace('rgba(30,40,58,0.26)', 'rgba(12,20,38,.70)').replace('background:rgba(255,255,255,.22);font-size:13px;font-weight:650', 'background:rgba(255,255,255,.12);font-size:13px;font-weight:650;color:#fff')
        out[name] = html
        open(f'{U.OUT}/{name}.dc.html', 'w').write(html)
    return out


if __name__ == '__main__':
    import sys
    r = build_all(sys.argv[1:] or None)
    for n, h in r.items():
        print(n, len(h))
