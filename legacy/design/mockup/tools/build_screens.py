"""Остальные экраны: Обновления, Поиск, Окно загрузки (2 состояния), Настройки (3 раздела).
Запуск: python3 gen.py && python3 build_screens.py (после build_ui.py)."""
from build_ui import *  # noqa: F401,F403
import build_ui as U

OUT = U.OUT

CHECK = '<path d="M5 12.5l4.5 4.5L19 7.5"/>'
WARN = '<path d="M12 4.2l9 15.6H3z"/><path d="M12 10v4.2"/><path d="M12 17.4v.1"/>'
LOCK = '<rect x="5" y="11" width="14" height="9" rx="1.5"/><path d="M8 11V8a4 4 0 018 0v3"/>'


def svg(path, size=18, sw=2, color='currentColor'):
    return (f'<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="{color}" stroke-width="{sw}" '
            f'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{path}</svg>')


def primary(text, h=38, fs=14.5, extra=''):
    return (f'<button type="button" class="btn" style="height:{h}px;padding:0 {h // 2 + 8}px;border:0;border-radius:{h // 2}px;background:#fff;color:#111;font-size:{fs}px;font-weight:650;cursor:pointer;'
            f'box-shadow:0 0 26px rgba(170,205,255,.34), 0 6px 16px rgba(6,10,22,.3);white-space:nowrap;{extra}">{text}</button>')


def link(text, color='#fff', fs=13.5, w=600):
    return f'<button type="button" style="padding:0;border:0;background:transparent;color:{color};font-size:{fs}px;font-weight:{w};cursor:pointer;white-space:nowrap">{text}</button>'


def dot(color):
    return f'<span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:{color};margin-right:8px;flex:0 0 auto"></span>'


def topbar(glass, title, count, segs, ph, value=None, search_w=200):
    sg = ''.join(f'<label class="seg"><input type="radio" name="f" {"checked" if i == 0 else ""}><span>{t}{(" <b style=\"opacity:.7;font-weight:600\">" + c + "</b>") if c else ""}</span></label>'
                 for i, (t, c) in enumerate(segs))
    val = f'value="{value}" ' if value else ''
    return (f'<header class="glass top" style="left:116px;top:16px;width:964px;height:52px;border-radius:26px;{surface(glass, "Bar", 7)}">{rim(glass, "bar")}'
            f'<div style="position:relative;display:flex;align-items:center;gap:14px;height:100%;padding:0 12px 0 22px">'
            f'<span style="font-size:18px;font-weight:650;letter-spacing:-0.01em">{title}</span><span style="color:rgba(255,255,255,0.64);font-size:13px">{count}</span><span style="flex:1"></span>'
            f'<div role="radiogroup" aria-label="Фильтр" style="display:flex;gap:2px;padding:3px;border-radius:19px;background:rgba(255,255,255,0.10)">{sg}</div>'
            f'<label style="display:flex;align-items:center;gap:8px;width:{search_w}px;height:34px;padding:0 12px;box-sizing:border-box;border-radius:17px;background:rgba(255,255,255,0.12);color:rgba(255,255,255,0.72)">{U.icon("search", 15)}'
            f'<input type="text" {val}placeholder="{ph}" aria-label="{ph}" style="flex:1;min-width:0;border:0;outline:0;background:transparent;color:#fff;font:inherit;font-size:13px"></label>'
            f'</div></header>\n')


def side(glass, inner, h=540, top=16):
    bg = "background:rgba(24,34,52,.44);" if glass else ""
    return (f'<aside class="glass insp" style="left:1096px;top:{top}px;width:328px;height:{h}px;border-radius:32px;{bg}{surface(glass, "Chk", 14)}">{rim(glass, "chk")}'
            f'<div style="position:relative;display:flex;flex-direction:column;gap:16px;padding:24px 22px;height:100%;box-sizing:border-box">{inner}</div></aside>\n')


def site_rows(rows):
    out = ''
    for i, (name, note, color) in enumerate(rows):
        out += (f'<div style="display:flex;align-items:center;padding:11px 14px;{"border-top:1px solid rgba(255,255,255,.09)" if i else ""}">{dot(color)}'
                f'<span style="flex:1;font-weight:550">{name}</span><span style="font-size:12.5px;color:rgba(255,255,255,.68)">{note}</span></div>')
    return f'<div style="border-radius:16px;background:rgba(255,255,255,.09);overflow:hidden">{out}</div>'


# ------------------------------------------------------------- Обновления
def poster(y, img, name, old, new, why, meta, status, crop=''):
    extra = ''
    if crop:
        d = dict(p.split(':') for p in crop.split(';'))
        extra = f'background-size:{d["size"]};background-position:{d["pos"]};'
    grey = 'filter:grayscale(1) brightness(.8);' if status == 'lost' else ''
    chip = {'update': '', 'maybe': f'<span style="display:inline-flex;align-self:flex-start;align-items:center;margin-bottom:8px;padding:4px 11px 4px 9px;border-radius:13px;background:rgba(255,255,255,.14);font-size:12.5px;font-weight:600">{dot("#ffd36b")}Возможно, обновлён</span>',
            'lost': f'<span style="display:inline-flex;align-self:flex-start;align-items:center;margin-bottom:8px;padding:4px 11px 4px 9px;border-radius:13px;background:rgba(255,255,255,.14);font-size:12.5px;font-weight:600">{dot("#ffb340")}Страница пропала</span>'}[status]
    if new:
        change = (f'<div style="font-size:26px;font-weight:700;letter-spacing:-0.015em;line-height:1.1"><span style="color:rgba(255,255,255,.55);font-weight:500;text-decoration:line-through">{old}</span>'
                  f'<span style="color:rgba(255,255,255,.6);font-weight:500"> → </span>{new}</div>')
    else:
        change = ''
    btns = {'update': primary('Обновить') + link('Открыть страницу') + link('Пропустить эту версию', 'rgba(255,255,255,.7)', w=500),
            'maybe': primary('Открыть страницу') + link('Версия не изменилась', 'rgba(255,255,255,.7)', w=500),
            'lost': primary('Найти замену') + link('Не проверять этот мод', 'rgba(255,255,255,.7)', w=500)}[status]
    return (f'<article class="card" style="position:absolute;left:116px;top:{y}px;width:964px;animation-delay:{120 + (y // 200) * 70}ms">'
            f'<div class="plate" style="height:196px;box-sizing:border-box;display:flex;gap:20px;padding:10px">'
            f'<div class="ph pic p-{img}" style="flex:0 0 316px;height:176px;border-radius:17px;{grey}{extra}box-shadow:inset 0 0 0 1px rgba(255,255,255,.12)"></div>'
            f'<div style="flex:1;min-width:0;display:flex;flex-direction:column;justify-content:center;padding:4px 10px 4px 0">{chip}'
            f'<div style="font-size:15px;color:rgba(255,255,255,.8);font-weight:600;margin-bottom:3px">{name}</div>{change}'
            f'<div style="margin-top:8px;font-size:13.5px;color:#fff">{why}</div><div style="margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.66)">{meta}</div>'
            f'<div style="display:flex;align-items:center;gap:22px;margin-top:14px">{btns}</div></div></div></article>\n')


def updates(glass=True):
    filters = (lens('lgRail', 'rail', 84, 212, 70) + lens('lgGear', 'gear', 56, 56, 56) + lens('lgBar', 'bar', 964, 52, 80) + lens('lgChk', 'chk', 328, 540, 96)) if glass else ''
    body = poster(88, 'dacia', 'Dacia Sandero Stepway', '1.0', '1.5', 'Версия на сайте выше установленной.', 'Modland, страница обновлена 2 дня назад. Размер 142 → 151 МБ.', 'update')
    body += poster(304, 'truck', 'Kenworth T610 с цистерной', '1.4b', '1.5', 'Версия на сайте выше установленной.', 'ModsGaming, страница обновлена 3 недели назад.', 'update')
    body += poster(520, 'lebedka', 'Rally Stage Pack', '', '', 'Версия та же, но размер на сайте изменился: 410 → 438 МБ.', 'Modland. Возможно, автор перезалил файл. Проверьте описание на странице.', 'maybe')
    body += poster(736, 'mustang', 'Ford Mustang Shelby GT500', '', '', 'Страницу не нашли две проверки подряд.', 'Мод в игре работает как раньше, не обновляется только проверка.', 'lost')
    body += flies() + chrome(glass, 'updates')
    body += topbar(glass, 'Обновления', '3 доступно', [('Доступные', '3'), ('Скрытые', '2'), ('Страница пропала', '1')], 'Найти')
    inner = (f'<div><div style="font-size:20px;font-weight:700;letter-spacing:-0.01em">Проверка</div><div style="margin-top:3px;font-size:13px;color:rgba(255,255,255,.68)">Последняя сегодня в 14:02. Окно можно закрыть: найденное сохранится.</div></div>'
             f'{primary("Обновить всё (2)", 44, 15, "width:100%")}'
             f'<div><div style="font-size:13px;font-weight:600;color:rgba(255,255,255,.66);padding:0 4px 8px">Сайты</div>{site_rows([("Modland", "готово", "#30d158"), ("ModsGaming", "медленно", "#ffb340"), ("Репозиторий BeamNG", "готово", "#30d158")])}</div>'
             f'<div style="display:flex;align-items:center;gap:12px;padding:0 4px"><span style="flex:1;font-size:13.5px;font-weight:550">Проверять при запуске</span>'
             f'<span class="trk" style="background:linear-gradient(#c6dfff,#8fbfff);box-shadow:0 0 22px rgba(150,200,255,.5)"><i style="transform:translateX(20px)"></i></span></div>'
             f'<div style="margin-top:auto;display:flex;align-items:center;gap:10px">{link("Проверить сейчас", w=650)}<span style="font-size:12.5px;color:rgba(255,255,255,.6)">автоматически раз в сутки</span></div>')
    body += side(glass, inner, 470)
    return page('Обновления', filters, body, glass)


# ------------------------------------------------------------- Поиск
RESULTS = [
    ('lambo', 'Lamborghini Huracán EVO', '1.6 · kuro_garage', 'free', 'Установить', ''),
    ('lambo', 'Huracán Performante', '2.0 · dreamline', 'free', 'Установить', 'size:190%;pos:30% 60%'),
    ('lambo', 'Aventador SVJ', '1.1 · apex_works', 'paid', None, 'size:230%;pos:70% 50%'),
    ('lambo', 'Lamborghini Urus Pack', '0.7 · автор не указан', 'unknown', 'Установить', 'size:170%;pos:90% 30%'),
    ('lambo', 'Gallardo Street', '1.3 · low_and_slow', 'free', 'Установить', 'size:210%;pos:10% 70%'),
    ('lambo', 'Huracán Rally', '0.6 · gravelcrew', 'free', 'Установить', 'size:250%;pos:50% 20%'),
]


def stile(x, y, pos, img, name, sub, price, action, crop):
    extra = ''
    if crop:
        d = dict(p.split(':') for p in crop.split(';'))
        extra = f'background-size:{d["size"]};background-position:{d["pos"]};'
    chip = {'free': '<span style="padding:4px 10px;border-radius:13px;background:rgba(255,255,255,.94);color:#14181f;font-size:12px;font-weight:700">Бесплатно</span>',
            'paid': f'<span style="display:inline-flex;align-items:center;padding:4px 10px 4px 8px;border-radius:13px;background:rgba(20,28,44,.78);color:#fff;font-size:12px;font-weight:700">{dot("#ffb340")}Платный</span>',
            'unknown': '<span style="padding:3px 9px;border-radius:13px;border:1.5px solid rgba(255,255,255,.9);background:rgba(20,28,44,.6);color:#fff;font-size:12px;font-weight:600">Цена не указана</span>'}[price]
    btn = (primary(action, 34, 13.5) if action else
           '<button type="button" class="btn" style="height:34px;padding:0 18px;border:1.5px solid #ffb340;border-radius:17px;background:transparent;color:#ffb340;font-size:13.5px;font-weight:650;cursor:pointer;white-space:nowrap">Купить у автора</button>')
    return (f'<article class="card" style="position:absolute;left:{x}px;top:{y}px;width:308px;animation-delay:{80 + pos * 55}ms">'
            f'<div class="plate" style="height:286px;box-sizing:border-box">'
            f'<div class="ph pic p-{img}" style="height:169px;border-radius:17px;{extra}box-shadow:inset 0 0 0 1px rgba(255,255,255,.12)"><span style="position:absolute;top:10px;left:10px">{chip}</span></div>'
            f'<div style="padding:10px 6px 0"><div style="font-size:15px;font-weight:620;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#fff">{name}</div>'
            f'<div style="margin-top:1px;font-size:12.5px;color:rgba(255,255,255,.72);white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{sub}</div>'
            f'<div style="display:flex;align-items:center;gap:18px;margin-top:12px">{btn}{link("Страница", w=600)}</div></div></div></article>\n')


def search(glass=True):
    filters = (lens('lgRail', 'rail', 84, 212, 70) + lens('lgGear', 'gear', 56, 56, 56) + lens('lgBar', 'bar', 964, 52, 80) + lens('lgChk', 'chk', 328, 540, 96)) if glass else ''
    body = ''
    for pos, r in enumerate(RESULTS + [('lambo', 'Huracán STO', '1.0 · garage47', 'free', 'Установить', 'size:240%;pos:20% 40%'), ('lambo', 'Huracán Tecnica', '1.2 · volt_lab', 'free', 'Установить', 'size:200%;pos:80% 60%'), ('lambo', 'Diablo GT', '0.9 · retro_gt', 'free', 'Установить', 'size:220%;pos:40% 80%')]):
        body += stile(116 + (pos % 3) * 328, 88 + (pos // 3) * 302, pos, *r)
    body += flies() + chrome(glass, 'search')
    body += topbar(glass, 'Поиск', '9 модов', [('Все', '9'), ('Бесплатные', '7'), ('Установленные', '0')], 'Что искать', value='lamborghini', search_w=240)
    inner = (f'<div><div style="font-size:20px;font-weight:700;letter-spacing:-0.01em">Сайты</div><div style="margin-top:3px;font-size:13px;color:rgba(255,255,255,.68)">Один мод с разных сайтов собран в одну запись.</div></div>'
             f'{site_rows([("Modland", "нашёл 6", "#30d158"), ("ModsGaming", "нашёл 4", "#30d158"), ("Репозиторий BeamNG", "нашёл 3", "#30d158")])}'
             f'<div style="font-size:13px;color:rgba(255,255,255,.72);line-height:1.45;padding:0 4px"><span style="display:flex;align-items:flex-start">{dot("#ffb340")}<span>Один сайт ответил медленно, поэтому показаны не все результаты. Поиск продолжается.</span></span></div>'
             f'<div style="margin-top:auto;font-size:12.5px;color:rgba(255,255,255,.62);padding:0 4px;line-height:1.45">Платные моды не скачиваются: вместо «Установить» кнопка ведёт на страницу автора.</div>')
    body += side(glass, inner, 420)
    return page('Поиск', filters, body, glass)


# ------------------------------------------------------------- Окно загрузки
STEPS = [('Нажмите Download', 'Если сайт просит подождать или пройти проверку, сделайте это там же.'),
         ('Файл придёт сам', 'Менеджер заберёт готовую загрузку.'),
         ('Проверка архива', 'Структура, опасные пути, подозрительные файлы.'),
         ('Установка', 'Прошлая версия останется в копиях.')]


def steps(done):
    out = ''
    for i, (t, n) in enumerate(STEPS):
        if i < done:
            mark = f'<span style="width:24px;height:24px;border-radius:50%;background:#fff;color:#111;display:flex;align-items:center;justify-content:center">{svg(CHECK, 14, 2.6)}</span>'
            tcol = 'rgba(255,255,255,.78)'
        elif i == done:
            mark = '<span class="pulse" style="width:24px;height:24px;border-radius:50%;box-sizing:border-box;border:2px solid #fff;background:rgba(255,255,255,.18);box-shadow:0 0 18px rgba(170,205,255,.6)"></span>'
            tcol = '#fff'
        else:
            mark = '<span style="width:24px;height:24px;border-radius:50%;box-sizing:border-box;border:2px solid rgba(255,255,255,.4)"></span>'
            tcol = 'rgba(255,255,255,.62)'
        line = '' if i == len(STEPS) - 1 else f'<span style="flex:1;width:2px;margin:4px 0;background:rgba(255,255,255,{.5 if i < done else .22})"></span>'
        out += (f'<li style="display:flex;gap:14px"><div style="display:flex;flex-direction:column;align-items:center">{mark}{line}</div>'
                f'<div style="padding-bottom:22px"><div style="font-size:15px;font-weight:{650 if i == done else 600};color:{tcol}">{t}</div>'
                f'<div style="margin-top:2px;font-size:12.5px;color:rgba(255,255,255,.64);line-height:1.4">{n}</div></div></li>')
    return f'<ol style="margin:0;padding:0;list-style:none;display:flex;flex-direction:column;flex:1">{out}</ol>'


def download(verified=False):
    filters = lens('lgDl', 'dl', 1100, 700, 96)
    if not verified:
        left = (f'<div style="display:flex;align-items:center;gap:8px;height:34px;padding:0 14px;border-radius:17px;background:rgba(255,255,255,.12);font-size:13.5px;color:rgba(255,255,255,.8)">{svg(LOCK, 14, 2.2)}modsfire.com/abc123</div>'
                f'<div style="flex:1;border-radius:22px;background:#f4f5f7;color:#16181d;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;box-shadow:inset 0 0 0 1px rgba(255,255,255,.4)">'
                f'<div style="width:50px;height:50px;border-radius:13px;background:#d9dde3"></div><div style="font-weight:650;font-size:18px">dacia_sandero_v1.5.zip</div><div style="color:#5b616b">151 МБ</div>'
                f'<button type="button" class="pulse" style="height:48px;padding:0 44px;border:0;border-radius:24px;background:#2f6bff;color:#fff;font-weight:650;font-size:16px;cursor:pointer">Download</button>'
                f'<div style="max-width:380px;text-align:center;font-size:12.5px;color:#5b616b;line-height:1.5">Это условная страница. Настоящая откроется здесь же, а все кнопки на ней нажимаете вы сами.</div></div>')
    else:
        row = lambda ok, text: (f'<div style="display:flex;gap:12px;align-items:flex-start;font-size:14.5px;line-height:1.45"><span style="flex:0 0 auto;width:24px;height:24px;border-radius:50%;background:{"#fff" if ok else "#ffb340"};color:#111;display:flex;align-items:center;justify-content:center">'
                                f'{svg(CHECK if ok else WARN, 14, 2.6)}</span><span style="padding-top:2px">{text}</span></div>')
        left = (f'<div style="flex:1;border-radius:22px;background:rgba(255,255,255,.09);box-shadow:inset 0 0 0 1px rgba(255,255,255,.12);padding:26px 28px;display:flex;flex-direction:column;gap:16px">'
                f'<div><div style="font-size:22px;font-weight:700;letter-spacing:-0.01em">Архив проверен</div><div style="margin-top:3px;font-size:13px;color:rgba(255,255,255,.68)">dacia_sandero_v1.5.zip, 151 МБ</div></div>'
                f'{row(True, "Структура верная: внутри <b>vehicles</b> с автомобилем.")}{row(True, "Опасных путей и подозрительных файлов нет.")}'
                f'{row(False, "В архиве есть папка <b>lua</b> с кодом. Устанавливайте, если доверяете автору.")}'
                f'<div style="border-radius:16px;background:rgba(255,255,255,.09);overflow:hidden;margin-top:6px">{info_rows([("Что внутри", "Машина Dacia Sandero Stepway"), ("Файлов", "214, после распаковки 312 МБ"), ("Код (lua)", "2 файла, ниже можно посмотреть"), ("Отпечаток", "совпадает с версией на сайте")])}</div>'
                f'<div style="margin-top:auto;display:flex;flex-direction:column;gap:14px"><div style="display:flex;align-items:center;gap:22px">{primary("Установить", 46, 15.5)}{link("Отмена", w=600)}</div>'
                f'<div style="font-size:12.5px;color:rgba(255,255,255,.66)">Версия 1.0 уйдёт в прошлые версии. Выключенный мод останется выключенным.</div></div></div>')
    sheet = (f'<section class="glass top" aria-label="Скачивание" style="left:170px;top:130px;width:1100px;height:700px;border-radius:36px;background:rgba(24,34,52,.44);{surface(True, "Dl", 14)}">{rim(True, "dl")}'
             f'<div style="position:relative;display:flex;flex-direction:column;gap:20px;height:100%;padding:34px 40px;box-sizing:border-box">'
             f'<div style="display:flex;align-items:center;gap:18px"><div class="pic p-dacia" style="flex:0 0 auto;width:84px;height:52px;border-radius:12px;box-shadow:inset 0 0 0 1px rgba(255,255,255,.2), 0 6px 16px rgba(6,10,22,.4)"></div>'
             f'<div style="flex:1"><div style="font-size:22px;font-weight:700;letter-spacing:-0.01em">Скачивание</div><div style="margin-top:2px;font-size:13.5px;color:rgba(255,255,255,.72)">Dacia Sandero Stepway: обновление с 1.0 до 1.5</div></div>{link("Закрыть", w=650)}</div>'
             f'<div style="flex:1;min-height:0;display:flex;gap:32px"><div style="flex:1;min-width:0;display:flex;flex-direction:column;gap:14px">{left}</div>'
             f'<div style="flex:0 0 300px;display:flex;flex-direction:column;padding-top:4px">{steps(3 if verified else 0)}'
             f'<div style="font-size:12.5px;color:rgba(255,255,255,.62);line-height:1.45">Менеджер сам ничего не нажимает на странице и не обходит ожидание и рекламу.</div></div></div></div></section>\n')
    body = flies() + sheet
    return page('Окно загрузки', filters, body, True)


# ------------------------------------------------------------- Настройки
SECTIONS = ['Внешний вид', 'Загрузка модов', 'Папки', 'Обновления', 'Сайты']


def settings_page(section):
    mapname, H = {'Внешний вид': ('set', 640), 'Загрузка модов': ('set_d', 470), 'Сайты': ('set_s', 560)}[section]
    filters = lens('lgRail', 'rail', 84, 212, 70) + lens('lgGear', 'gear', 56, 56, 56) + lens('lgSet', mapname, 1308, H, 96) + lens('lgPv', 'pv', 420, 56, 80)
    nav = ''.join(
        f'<a class="nv" href="#" style="display:flex;align-items:center;height:42px;padding:0 16px;border-radius:21px;color:{"#fff" if t == section else "rgba(255,255,255,.76)"};text-decoration:none;font-size:14.5px;font-weight:{650 if t == section else 500};'
        f'background:{"rgba(255,255,255,.20);box-shadow:0 0 22px rgba(170,205,255,.3)" if t == section else "transparent"}">{t}</a>' for t in SECTIONS)
    if section == 'Загрузка модов':
        content = (toggle('dl', False, 'Разрешить загрузку модов', 'Выключено: в программе нет кнопок «Скачать» и «Установить». Менеджер показывает ваши моды, проверяет обновления и ищет по сайтам, а скачивать вы идёте на страницу автора.')
                   + '<div style="height:12px"></div>'
                   + '<div style="display:flex;align-items:center;gap:20px;padding:18px 20px;border-radius:20px;background:rgba(255,255,255,.09)"><span style="flex:1"><span style="display:block;font-size:16px;font-weight:620">После скачивания</span>'
                     '<span style="display:block;margin-top:4px;font-size:13px;color:rgba(255,255,255,.7)">Установка всегда с вашего подтверждения. Прошлая версия остаётся в копиях.</span></span>'
                     '<span style="padding:8px 16px;border-radius:18px;background:rgba(255,255,255,.14);font-size:14px;font-weight:600">Спрашивать каждый раз</span></div>'
                   + '<div style="margin-top:20px;font-size:13px;color:rgba(255,255,255,.68);max-width:600px;line-height:1.5">Платные моды не скачиваются никогда: вместо «Скачать» стоит «Купить у автора». Страницы файлообменников открываются в окне программы, а кнопки на них нажимаете вы.</div>')
    elif section == 'Сайты':
        srcs = [('Modland', 'Каталог. Отвечает нормально.', True), ('ModsGaming', 'Каталог на русском. Отвечает медленно.', True),
                ('STMods', 'Каталог. Сейчас не открывается, проверка пропускается.', False), ('Репозиторий BeamNG', 'Официальный каталог и обновления от авторов.', True)]
        content = (''.join(toggle(f's{i}', on, n, t) + '<div style="height:10px"></div>' for i, (n, t, on) in enumerate(srcs))
                   + '<div style="margin-top:12px;font-size:13px;color:rgba(255,255,255,.68)">Выключенный сайт не участвует в поиске и проверке обновлений. Сбой одного сайта не ломает остальные.</div>')
    elif section == 'Внешний вид':
        content = (toggle('lg', True, 'Жидкое стекло', 'Панели преломляют фон, как настоящее стекло. Если интерфейс работает медленно, выключите: вид останется прежним, пропадёт только преломление.')
                   + preview(True) + toggle('an', True, 'Анимированный фон', 'Фото медленно меняет цвет, над ним плавают огоньки. Выключите, чтобы экономить ресурсы или если движение мешает.')
                   + '<div style="margin-top:22px;font-size:13px;color:rgba(255,255,255,.64);max-width:560px">Если в Windows включено «Уменьшить анимацию» или «Уменьшить прозрачность», программа учтёт это сама, даже когда здесь всё включено.</div>')
    else:
        content = ''
    body = flies() + chrome(True, 'settings')
    body += (f'<section class="glass top" aria-label="Настройки" style="left:116px;top:16px;width:1308px;height:{H}px;border-radius:32px;background:rgba(24,34,52,.44);{surface(True, "Set", 14)}">{rim(True, mapname)}'
             f'<div style="position:relative;display:flex;height:100%;padding:36px 44px 36px 36px;box-sizing:border-box;gap:44px">'
             f'<div style="flex:0 0 230px"><div style="font-size:28px;font-weight:700;letter-spacing:-0.02em;margin:0 0 24px 16px">Настройки</div><nav style="display:flex;flex-direction:column;gap:4px" aria-label="Разделы настроек">{nav}</nav></div>'
             f'<div style="flex:1;min-width:0;max-width:760px"><div style="font-size:22px;font-weight:700;letter-spacing:-0.01em;margin:6px 0 20px">{section}</div>{content}</div></div></section>\n')
    return page('Настройки: ' + section, filters, body, True)


if __name__ == '__main__':
    jobs = {'Updates': updates(True), 'Search': search(True), 'Download': download(False), 'DownloadChecked': download(True),
            'SettingsDownload': settings_page('Загрузка модов'), 'SettingsSites': settings_page('Сайты')}
    for n, html in jobs.items():
        open(f'{OUT}/{n}.dc.html', 'w').write(html)
    for f in sorted(os.listdir(OUT)):
        print(f, os.path.getsize(f'{OUT}/{f}'))
