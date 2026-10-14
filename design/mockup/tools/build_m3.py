"""Доска «Гараж в Material 3» — только для сравнения стилей. Тёмная схема M3, цвета из одного ключа (синий).
Запуск из папки tools после build_ui.py:  python3 build_m3.py   (результат out/GarageM3.dc.html)"""
import build_ui as U

T = dict(  # тональная палитра M3, тёмная схема, ключ — синий
    bg='#111318', low='#191c20', cont='#1d2024', high='#282a2f', highest='#33353a',
    on='#e2e2e9', onv='#c3c6cf', out='#8d9199', outv='#43474e',
    pri='#a8c8ff', onpri='#06305f', pric='#1f477b', onpric='#d4e3ff',
    sec='#bcc7dc', secc='#3d4759', onsecc='#d8e3f8',
    amber='#ffb951', ambc='#5c4200', err='#ffb4ab', errc='#93000a', onerr='#690005')
P = {
    'car': 'M18.92 6.01C18.72 5.42 18.16 5 17.5 5h-11c-.66 0-1.21.42-1.42 1.01L3 12v8c0 .55.45 1 1 1h1c.55 0 1-.45 1-1v-1h12v1c0 .55.45 1 1 1h1c.55 0 1-.45 1-1v-8l-2.08-5.99zM6.5 16c-.83 0-1.5-.67-1.5-1.5S5.67 13 6.5 13s1.5.67 1.5 1.5S7.33 16 6.5 16zm11 0c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zM5 11l1.5-4.5h11L19 11H5z',
    'sync': 'M12 4V1L8 5l4 4V6c3.31 0 6 2.69 6 6 0 1.01-.25 1.97-.7 2.8l1.46 1.46C19.54 15.03 20 13.57 20 12c0-4.42-3.58-8-8-8zm0 14c-3.31 0-6-2.69-6-6 0-1.01.25-1.97.7-2.8L5.24 7.74C4.46 8.97 4 10.43 4 12c0 4.42 3.58 8 8 8v3l4-4-4-4v3z',
    'search': 'M15.5 14h-.79l-.28-.27A6.471 6.471 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
    'gear': 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 0 0 .12-.61l-1.92-3.32a.488.488 0 0 0-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 0 0-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58a.49.49 0 0 0-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z',
    'play': 'M8 5v14l11-7z',
    'check': 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
    'open': 'M19 19H5V5h7V3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14c1.1 0 2-.9 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z',
    'warn': 'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
    'hist': 'M13 3a9 9 0 0 0-9 9H1l3.89 3.89.07.14L9 12H6c0-3.87 3.13-7 7-7s7 3.13 7 7-3.13 7-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42A8.954 8.954 0 0 0 13 21a9 9 0 0 0 0-18zm-1 5v5l4.28 2.54.72-1.21-3.5-2.08V8H12z',
}


def ic(k, size=24, color='currentColor'):
    return f'<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="{color}" aria-hidden="true"><path d="{P[k]}"/></svg>'


FONT = "Roboto,'Segoe UI Variable Text','Segoe UI',system-ui,sans-serif"
PH = ['dacia', 'truck', 'lambo', 'coupe', 'mustang', 'tractor', 'lebedka', 'traffic', 'station']

CSS = f''':root{{--e:cubic-bezier(.2,0,0,1);--d:.3s}}
body{{margin:0;background:{T['bg']}}}
button{{font-family:inherit}}
a:focus-visible,button:focus-visible,input:focus-visible+span{{outline:3px solid {T['pri']};outline-offset:2px}}
''' + '\n'.join(f'.p-{n}{{background-image:url({U.photo(n)});background-size:cover;background-position:center}}' for n in PH) + f'''
.sl{{position:relative}}
.sl::after{{content:"";position:absolute;inset:0;border-radius:inherit;background:{T['on']};opacity:0;transition:opacity var(--d) var(--e);pointer-events:none}}
.sl:hover::after{{opacity:.08}}
.sl:active::after{{opacity:.12}}
.nvi{{display:flex;flex-direction:column;align-items:center;gap:4px;width:80px;color:{T['onv']};font-size:12px;font-weight:500;letter-spacing:.2px;text-decoration:none}}
.nvi .pill{{position:relative;width:56px;height:32px;border-radius:16px;display:flex;align-items:center;justify-content:center;transition:background var(--d) var(--e)}}
.nvi:hover .pill{{background:rgba(226,226,233,.08)}}
.nvi.on{{color:{T['on']}}}.nvi.on .pill{{background:{T['secc']};color:{T['onsecc']}}}
.chip input{{position:absolute;opacity:0;pointer-events:none}}
.chip span{{display:flex;align-items:center;gap:8px;height:32px;padding:0 16px;border-radius:8px;border:1px solid {T['out']};color:{T['onv']};font-size:14px;font-weight:500;letter-spacing:.1px;cursor:pointer;transition:background var(--d) var(--e)}}
.chip span:hover{{background:rgba(226,226,233,.08)}}
.chip input:checked+span{{background:{T['secc']};border-color:transparent;color:{T['onsecc']};padding-left:8px}}
.chip .ck{{display:none}}.chip input:checked+span .ck{{display:block}}
.card{{position:relative;border-radius:20px;background:{T['high']};overflow:hidden;cursor:pointer;transition:box-shadow var(--d) var(--e)}}
.card:hover{{box-shadow:0 1px 3px 1px rgba(0,0,0,.35),0 1px 2px rgba(0,0,0,.5)}}
.card::after{{content:"";position:absolute;inset:0;background:{T['on']};opacity:0;transition:opacity var(--d) var(--e);pointer-events:none}}
.card:hover::after{{opacity:.08}}
.card.sel{{box-shadow:inset 0 0 0 2px {T['pri']};background:#2a3342}}
.btn{{display:inline-flex;align-items:center;justify-content:center;gap:8px;height:40px;padding:0 24px;border:0;border-radius:20px;font-size:14px;font-weight:500;letter-spacing:.1px;cursor:pointer;white-space:nowrap}}
.btn.ic{{padding:0 24px 0 16px}}
.txt{{height:40px;padding:0 12px;border:0;border-radius:20px;background:transparent;font-size:14px;font-weight:500;letter-spacing:.1px;cursor:pointer}}
.txt:hover{{background:rgba(168,200,255,.08)}}
'''


def card(img, n, sub, status, crop, sel, pos):
    extra = ''
    if crop:
        d = dict(p.split(':') for p in crop.split(';'))
        extra = f'background-size:{d["size"]};background-position:{d["pos"]};'
    grey = 'filter:grayscale(1) brightness(.8);' if status == 'off' else ''
    lab = ''
    if status == 'update':
        lab = f'<span style="position:absolute;top:12px;right:12px;height:24px;padding:0 10px;display:flex;align-items:center;border-radius:8px;background:{T["pri"]};color:{T["onpri"]};font-size:12px;font-weight:500;letter-spacing:.5px">Есть обновление</span>'
    chk = (f'<span style="position:absolute;top:12px;left:12px;width:24px;height:24px;border-radius:50%;background:{T["pri"]};color:{T["onpri"]};display:flex;align-items:center;justify-content:center">{ic("check", 18)}</span>') if sel else ''
    s = sub
    if status == 'update':
        s = f'<span style="color:{T["pri"]};font-weight:500">{sub}</span>'
    elif status in ('lost', 'maybe'):
        s = f'<span style="display:inline-flex;align-items:center;gap:6px;color:{T["amber"]}">{ic("warn", 16)}{"Страница пропала" if status == "lost" else "Возможно, обновлён"}</span>'
    elif status == 'off':
        s = 'Выключен'
    return (f'<article class="card{" sel" if sel else ""}" tabindex="0">'
            f'<div class="p-{img}" style="height:171px;{grey}{extra}position:relative">{lab}{chk}</div>'
            f'<div style="padding:12px 16px 14px"><div style="font-size:16px;line-height:24px;font-weight:500;letter-spacing:.15px;color:{T["on"]};white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{n}</div>'
            f'<div style="font-size:14px;line-height:20px;letter-spacing:.25px;color:{T["onv"]};white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{s}</div></div></article>')


def rows(items, check=False):
    out = ''
    for i, (k, v) in enumerate(items):
        out += (f'<div style="display:flex;align-items:center;gap:12px;min-height:40px;padding:0 4px;{"border-top:1px solid " + T["outv"] if i else ""}">'
                f'<span style="flex:0 0 96px;font-size:14px;color:{T["onv"]}">{k}</span><span style="flex:1;font-size:14px;color:{T["on"]};display:flex;align-items:center;gap:6px">'
                f'{(ic("check", 18, T["pri"])) if check else ""}{v}</span></div>')
    return out


def build():
    nav = ''.join(f'<a class="nvi{" on" if on else ""}" href="#"><span class="pill">{ic(k)}{(f"<span style=\"position:absolute;top:-2px;right:2px;min-width:16px;height:16px;padding:0 4px;box-sizing:border-box;border-radius:8px;background:{T["err"]};color:{T["onerr"]};font-size:11px;font-weight:500;line-height:16px;text-align:center\">{b}</span>") if b else ""}</span>{label}</a>'
                  for k, label, on, b in [('car', 'Гараж', True, None), ('sync', 'Обновления', False, '3'), ('search', 'Поиск', False, None)])
    rail = (f'<nav aria-label="Разделы" style="position:absolute;left:0;top:0;bottom:0;width:80px;background:{T["bg"]};display:flex;flex-direction:column;align-items:center;gap:20px;padding-top:44px;box-sizing:border-box">'
            f'<div style="display:flex;flex-direction:column;align-items:center;gap:4px;margin-bottom:20px"><button class="sl" type="button" aria-label="Играть" style="width:56px;height:56px;border:0;border-radius:16px;background:{T["pric"]};color:{T["onpric"]};display:flex;align-items:center;justify-content:center;cursor:pointer;box-shadow:0 1px 3px 1px rgba(0,0,0,.3),0 1px 2px rgba(0,0,0,.5)">{ic("play", 28)}</button>'
            f'<span style="font-size:12px;font-weight:500;color:{T["onv"]};letter-spacing:.5px">Играть</span></div>'
            f'{nav}<div style="flex:1"></div><div style="padding-bottom:20px"><a class="nvi" href="#"><span class="pill">{ic("gear")}</span>Настройки</a></div></nav>')
    chips = ''.join(f'<label class="chip"><input type="radio" name="f" {"checked" if i == 0 else ""}><span><svg class="ck" width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="{P["check"]}"/></svg>{t}<span style="all:unset;opacity:.7">&nbsp;{c}</span></span></label>'
                    for i, (t, c) in enumerate([('Все', '128'), ('Включённые', '116'), ('Нужно обновить', '3'), ('Не проверяются', '12')]))
    head = (f'<header style="position:absolute;left:104px;top:28px;width:944px;display:flex;align-items:center;gap:16px">'
            f'<h1 style="margin:0;font-size:28px;line-height:36px;font-weight:400;color:{T["on"]}">Гараж</h1><span style="font-size:14px;color:{T["onv"]}">128 модов</span><span style="flex:1"></span>'
            f'<label class="sl" style="display:flex;align-items:center;gap:12px;width:360px;height:56px;padding:0 16px;box-sizing:border-box;border-radius:28px;background:{T["high"]};color:{T["onv"]}">{ic("search")}'
            f'<input type="text" placeholder="Найти в гараже" aria-label="Найти среди установленных модов" style="flex:1;min-width:0;border:0;outline:0;background:transparent;color:{T["on"]};font:inherit;font-size:16px;letter-spacing:.5px"></label></header>'
            f'<div role="radiogroup" aria-label="Фильтр" style="position:absolute;left:104px;top:104px;display:flex;gap:8px">{chips}</div>')
    cards = ''.join(card(t[0], t[1], t[2], t[3], t[4], pos == 4, pos) for pos, t in enumerate(U.GRID[:15]))
    grid = f'<main style="position:absolute;left:104px;top:152px;width:944px;bottom:0;overflow:hidden"><div style="display:grid;grid-template-columns:repeat(3,304px);gap:16px">{cards}</div></main>'
    sheet = (f'<aside aria-label="Выбранный мод" style="position:absolute;left:1072px;top:16px;width:352px;height:928px;border-radius:28px;background:{T["low"]};box-sizing:border-box;padding:16px;display:flex;flex-direction:column;gap:16px;overflow:hidden">'
             f'<div class="p-dacia" style="height:150px;flex:0 0 auto;border-radius:20px"></div>'
             f'<div style="padding:0 4px"><div style="font-size:24px;line-height:32px;font-weight:400;color:{T["on"]}">Dacia Sandero Stepway</div><div style="font-size:14px;color:{T["onv"]};margin-top:2px">kuro_garage</div></div>'
             f'<div style="display:flex;gap:12px;padding:12px 16px;border-radius:16px;background:{T["pric"]};color:{T["onpric"]};font-size:14px;line-height:20px;letter-spacing:.25px"><span style="flex:0 0 auto;margin-top:1px">{ic("sync", 20)}</span><span>Сейчас стоит 1.0. Новая версия 1.5 есть на Modland и ModsGaming.</span></div>'
             f'<div style="display:flex;gap:8px;padding:0 4px"><button class="btn sl" type="button" style="background:{T["pri"]};color:{T["onpri"]}">Обновить до 1.5</button>'
             f'<button class="btn ic sl" type="button" style="background:{T["secc"]};color:{T["onsecc"]}">{ic("open", 18)}Страница</button></div>'
             f'<div style="padding:0 4px">{rows([("Размер", "142 МБ, на сайте 151 МБ"), ("Установлен", "12 августа"), ("Следим на", "Modland, ModsGaming"), ("Проверен", "сегодня, 14:02")])}</div>'
             f'<div style="padding:0 4px"><div style="font-size:14px;font-weight:500;color:{T["pri"]};padding:0 0 4px;letter-spacing:.1px">Внутри архива</div>{rows([("Машина", "Dacia Sandero Stepway"), ("Скрипты", "Нет, безопасно")])}</div>'
             f'<div style="padding:0 4px"><div style="font-size:14px;font-weight:500;color:{T["pri"]};padding:0 0 4px;letter-spacing:.1px">Прошлые версии</div>'
             + ''.join(f'<div style="display:flex;align-items:center;gap:12px;min-height:56px;{"border-top:1px solid " + T["outv"] if i else ""}"><span style="color:{T["onv"]}">{ic("hist", 20)}</span><div style="flex:1"><div style="font-size:14px;color:{T["on"]}">{w}</div><div style="font-size:12px;color:{T["onv"]}">{nn}</div></div><button class="txt" type="button" style="color:{T["pri"]}">Вернуть</button></div>'
                       for i, (w, nn) in enumerate([('12 августа, 21:40', 'Версия 1.0, 142 МБ'), ('3 июня, 09:12', 'Версия 0.9, 138 МБ')]))
             + f'</div><div style="margin-top:auto;display:flex;align-items:center;gap:4px;padding:0 0 0 0"><button class="txt" type="button" style="color:{T["pri"]}">Выключить</button><button class="txt" type="button" style="color:{T["err"]}">Удалить</button><span style="font-size:12px;color:{T["onv"]};margin-left:4px;white-space:nowrap">Можно вернуть позже</span></div></aside>')
    return f'''<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<title>Гараж в Material 3</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<style>{CSS}</style>
</helmet>
<div style="position:relative;width:1440px;height:960px;overflow:hidden;background:{T['bg']};color:{T['on']};font-family:{FONT};font-size:14px;line-height:1.4">
{rail}{head}{grid}{sheet}
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{{"$preview":{{"width":1440,"height":960}}}}'>
class Component extends DCLogic {{
  renderVals() {{ return {{}}; }}
}}
</script>
</body>
</html>
'''


if __name__ == '__main__':
    open(f'{U.OUT}/GarageM3.dc.html', 'w').write(build())
    print('ok')
