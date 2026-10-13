"""Собирает макеты интерфейса: Гараж (жидкое стекло), Гараж (без жидкого стекла), Настройки.
Запуск: python3 gen.py && python3 build_ui.py   (результат в out/)
Фотографии лежат в photos/ (jpg). Карты преломления делает gen.py."""
import base64, os, random

OUT = 'out'
os.makedirs(OUT, exist_ok=True)
FONT = "'Segoe UI Variable Text','Segoe UI Variable Display','SF Pro Text',system-ui,-apple-system,'Segoe UI',sans-serif"


def b64(path, mime):
    return f'data:{mime};base64,' + base64.b64encode(open(path, 'rb').read()).decode()


def A(name):
    return b64(name, 'image/png')


def photo(name):
    return b64(f'photos/{name}.jpg', 'image/jpeg')


def lens(fid, name, w, h, scale=46):
    """Преломление по карте смещения. Три канала цвета смещаются чуть по-разному:
    на краях появляется тонкая радужная кромка, как у настоящего стекла."""
    def ch(res, k, keep):
        m = {'r': '1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0', 'g': '0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0', 'b': '0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0'}[keep]
        return (f'<feDisplacementMap in="SourceGraphic" in2="map" scale="{scale * k:.0f}" xChannelSelector="R" yChannelSelector="G" result="{res}d"/>'
                f'<feColorMatrix in="{res}d" type="matrix" values="{m}" result="{res}"/>')
    return (f'<filter id="{fid}" x="0" y="0" width="{w}" height="{h}" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">'
            f'<feImage href="{A(name + ".png")}" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" result="map"/>'
            f'{ch("r", 1.0, "r")}{ch("g", 1.12, "g")}{ch("b", 1.26, "b")}'
            f'<feComposite in="r" in2="g" operator="arithmetic" k1="0" k2="1" k3="1" k4="0" result="rg"/>'
            f'<feComposite in="rg" in2="b" operator="arithmetic" k1="0" k2="1" k3="1" k4="0"/></filter>')


ICON = {
    'grid': '<path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"/>',
    'upd': '<path d="M12 3a9 9 0 100 18 9 9 0 000-18zM12 8v8m0 0l-3-3m3 3l3-3"/>',
    'search': '<path d="M11 4a7 7 0 100 14 7 7 0 000-14zM21 21l-4.3-4.3"/>',
    # шестерёнка (контур в стиле Feather, лицензия MIT)
    'gear': '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z"/>',
}


def icon(k, size=22, sw=1.7):
    return (f'<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="{sw}" '
            f'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{ICON[k]}</svg>')


def css(glass):
    tint = 'rgba(30,40,58,0.26)' if glass else 'rgba(34,46,68,0.66)'
    shadow = ('0 22px 56px rgba(8,12,24,0.38), inset 0 0 0 1px rgba(255,255,255,0.12)' if glass else
              '0 18px 44px rgba(8,12,24,0.34), inset 0 0 0 1px rgba(255,255,255,0.16), inset 0 1px 0 rgba(255,255,255,0.26)')
    photos = '\n'.join(f'.p-{n}{{background-image:url({photo(n)})}}' for n in
                       ['wall', 'mustang', 'dacia', 'tractor', 'truck', 'station', 'traffic', 'coupe', 'lebedka', 'lambo'])
    return f''':root{{--ease:cubic-bezier(.22,.8,.24,1);--fast:.25s;--mid:.55s;--slow:1s;--beat:6s}}
body{{margin:0;background:#1d2533}}
button{{font-family:inherit}}
a:focus-visible,button:focus-visible,input:focus-visible{{outline:3px solid #fff;outline-offset:2px}}
input::placeholder{{color:rgba(255,255,255,0.62)}}
{photos}
.pic{{background-size:cover;background-position:center;background-repeat:no-repeat}}
.glass{{position:absolute;color:#f6f8fb;background:{tint};box-shadow:{shadow}}}
.glass.lq::before{{content:"";position:absolute;inset:0;border-radius:inherit;pointer-events:none;background:linear-gradient(170deg,rgba(255,255,255,.20) 0%,rgba(255,255,255,.04) 34%,transparent 60%)}}
.rim{{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-repeat:no-repeat;background-size:100% 100%}}
.blob{{position:absolute;border-radius:50%;will-change:transform}}
@keyframes d1{{0%{{transform:translate(0,0) scale(1)}}50%{{transform:translate(140px,-90px) scale(1.18)}}100%{{transform:translate(-70px,70px) scale(1.04)}}}}
@keyframes d2{{0%{{transform:translate(0,0) scale(1.05)}}50%{{transform:translate(-160px,110px) scale(.92)}}100%{{transform:translate(60px,-60px) scale(1.12)}}}}
@keyframes d3{{0%{{transform:translate(0,0) scale(1)}}50%{{transform:translate(180px,-120px) scale(1.2)}}100%{{transform:translate(-90px,-30px) scale(.95)}}}}
.b1{{animation:d1 48s ease-in-out infinite alternate}}
.b2{{animation:d2 48s ease-in-out infinite alternate;animation-delay:-12s}}
.b3{{animation:d3 48s ease-in-out infinite alternate;animation-delay:-24s}}
@keyframes kb{{0%{{transform:scale(1.04) translate(0,0)}}100%{{transform:scale(1.12) translate(-22px,-14px)}}}}
.wall{{position:absolute;inset:-30px;animation:kb 48s ease-in-out infinite alternate}}
@keyframes flow{{0%{{background-position:0% 50%}}100%{{background-position:300% 50%}}}}
.grade{{position:absolute;inset:0;background:linear-gradient(110deg,hsla(212,75%,52%,.40),hsla(262,55%,56%,.34),hsla(24,85%,58%,.34),hsla(190,70%,50%,.34),hsla(212,75%,52%,.40));background-size:300% 100%;animation:flow 48s linear infinite;mix-blend-mode:soft-light}}
@keyframes dropin{{0%{{opacity:0;transform:translateY(-18px) scale(.97)}}100%{{opacity:1;transform:none}}}}
@keyframes leftin{{0%{{opacity:0;transform:translateX(-24px)}}100%{{opacity:1;transform:none}}}}
.top{{animation:dropin .9s .15s var(--ease) both}}
.lft{{animation:leftin .9s .05s var(--ease) both}}
.nv{{transition:background var(--fast) var(--ease), transform var(--fast) var(--ease)}}
.nv:hover{{background:rgba(255,255,255,.14) !important;transform:translateY(-1px)}}
.seg input{{position:absolute;opacity:0;pointer-events:none}}
.seg span{{display:block;padding:6px 14px;border-radius:15px;font-size:13px;font-weight:550;color:rgba(255,255,255,.76);cursor:pointer;transition:background var(--mid) var(--ease), color var(--fast)}}
.seg span:hover{{background:rgba(255,255,255,.10)}}
.seg input:checked + span{{background:rgba(255,255,255,.24);color:#fff;font-weight:650;box-shadow:0 0 18px rgba(170,205,255,.25)}}
.seg input:focus-visible + span{{outline:3px solid #fff;outline-offset:1px}}
.btn{{transition:transform var(--fast) var(--ease), box-shadow var(--mid) var(--ease), background var(--fast)}}
.btn:hover{{transform:translateY(-1px)}}
.btn:active{{transform:scale(.97)}}
@keyframes fly{{0%{{transform:translate(0,0)}}100%{{transform:translate(var(--dx),var(--dy))}}}}
@keyframes tw{{0%,100%{{opacity:.15}}50%{{opacity:.95}}}}
.fly{{position:absolute;border-radius:50%;mix-blend-mode:screen;pointer-events:none;background:radial-gradient(circle,#fff 0,rgba(255,244,214,.9) 28%,rgba(255,226,170,0) 70%);animation:fly var(--t) ease-in-out infinite alternate, tw var(--w) ease-in-out infinite}}
@keyframes rise{{0%{{opacity:0;transform:translateY(22px) scale(.96)}}100%{{opacity:1;transform:none}}}}
.card{{animation:rise .8s cubic-bezier(.2,.8,.2,1) both}}
.ph{{position:relative;overflow:hidden;transition:transform var(--mid) var(--ease), box-shadow var(--mid) var(--ease)}}
.ph::after{{content:"";position:absolute;inset:0;background:linear-gradient(115deg,transparent 35%,rgba(255,255,255,.28) 50%,transparent 65%);background-size:260% 100%;background-position:130% 0;transition:background-position .9s cubic-bezier(.2,.8,.2,1)}}
.card:hover .ph{{transform:translateY(-5px) scale(1.025);box-shadow:0 18px 40px rgba(6,10,22,.5), 0 0 34px rgba(170,205,255,.25)}}
.card:hover .ph::after{{background-position:-30% 0}}
@keyframes breathe{{0%,100%{{box-shadow:0 0 0 2px rgba(255,255,255,.92), 0 10px 34px rgba(140,190,255,.30)}}50%{{box-shadow:0 0 0 2px rgba(255,255,255,.92), 0 14px 56px rgba(140,190,255,.66)}}}}
.sel{{animation:breathe var(--beat) ease-in-out infinite}}
@keyframes pulse{{0%,40%{{box-shadow:0 0 0 0 rgba(255,255,255,.75)}}75%,100%{{box-shadow:0 0 0 10px rgba(255,255,255,0)}}}}
.pulse{{animation:pulse var(--beat) ease-out infinite}}
@keyframes slidein{{0%{{opacity:0;transform:translateX(60px)}}100%{{opacity:1;transform:none}}}}
.insp>div>*{{flex-shrink:0}}
.insp{{animation:slidein 1s .35s cubic-bezier(.2,.8,.2,1) both}}
@keyframes shine{{0%,40%{{background-position:-120% 0}}75%,100%{{background-position:220% 0}}}}
.cta{{position:relative;overflow:hidden}}
.cta::after{{content:"";position:absolute;inset:0;background:linear-gradient(110deg,transparent 40%,rgba(120,170,255,.45) 50%,transparent 60%);background-size:200% 100%;animation:shine var(--beat) var(--ease) infinite;pointer-events:none}}
.tip::before{{content:attr(data-tip);position:absolute;left:calc(100% + 12px);top:50%;transform:translateY(-50%) translateX(-6px);padding:7px 12px;border-radius:14px;background:rgba(20,28,44,.82);color:#fff;font-size:12.5px;font-weight:550;white-space:nowrap;opacity:0;pointer-events:none;transition:opacity var(--fast) var(--ease), transform var(--fast) var(--ease);box-shadow:0 8px 24px rgba(6,10,22,.4)}}
.tip:hover::before,.tip:focus-visible::before{{opacity:1;transform:translateY(-50%)}}
.tgl{{position:absolute;opacity:0;pointer-events:none}}
.trow{{display:flex;align-items:center;gap:20px;cursor:pointer}}
.trk{{flex:0 0 auto;width:52px;height:32px;border-radius:16px;background:rgba(255,255,255,.22);position:relative;transition:background var(--mid) var(--ease), box-shadow var(--mid) var(--ease)}}
.trk i{{position:absolute;left:3px;top:3px;width:26px;height:26px;border-radius:50%;background:#fff;box-shadow:0 2px 6px rgba(6,10,22,.4);transition:transform var(--mid) var(--ease)}}
.tgl:checked + .trow .trk{{background:linear-gradient(#c6dfff,#8fbfff);box-shadow:0 0 22px rgba(150,200,255,.5)}}
.tgl:checked + .trow .trk i{{transform:translateX(20px)}}
.tgl:focus-visible + .trow .trk{{outline:3px solid #fff;outline-offset:2px}}
.pv{{position:absolute;transition:opacity var(--mid) var(--ease)}}
.tgl:checked ~ .pvbox .pv-off,.tgl:not(:checked) ~ .pvbox .pv-on{{opacity:0}}
@media (prefers-reduced-motion:reduce){{.b1,.b2,.b3,.wall,.grade,.top,.lft,.fly,.card,.sel,.pulse,.insp,.cta::after{{animation:none !important}}.fly{{opacity:.5}}}}
@media (prefers-reduced-transparency:reduce){{.glass{{-webkit-backdrop-filter:none !important;backdrop-filter:none !important;background:#2a3446 !important}}}}
'''


def page(title, filters, body, glass, anim=True):
    if glass:
        body = body.replace('class="glass ', 'class="glass lq ')
    return f'''<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<title>{title}</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<style>{css(glass)}</style>
</helmet>
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>{filters}</defs></svg>
<div style="position:relative;width:1440px;height:960px;overflow:hidden;background:#1d2533;color:#f6f8fb;font-family:{FONT};font-size:14px;line-height:1.35">
<div class="wall pic p-wall"></div>
<div class="grade"></div>
<div style="position:absolute;inset:0;background:linear-gradient(180deg,rgba(12,18,32,.30),rgba(12,18,32,.52))"></div>
<div class="blob b1" style="left:-220px;top:380px;width:760px;height:760px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(200,60%,56%,.26) 0%,transparent 66%)"></div>
<div class="blob b2" style="left:760px;top:-300px;width:860px;height:860px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(250,45%,62%,.22) 0%,transparent 66%)"></div>
<div class="blob b3" style="left:380px;top:560px;width:640px;height:640px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(30,70%,62%,.18) 0%,transparent 66%)"></div>
{body}
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


def surface(glass, name, blur, scale=None):
    """Стиль поверхности: с жидким стеклом (размытие + преломление + кромка) или матовой (только размытие)."""
    if glass:
        f = f'blur({blur}px) url(#lg{name}) saturate(1.4)'
    else:
        f = 'blur(26px) saturate(1.15)'
    return f'-webkit-backdrop-filter:{f};backdrop-filter:{f}'


def rim(glass, name):
    if glass:
        return f'<div class="rim" style="background-image:url({A(name + "_rim.png")})"></div>'
    return '<div class="rim"></div>'


def flies():
    r = random.Random(7)
    out = ''
    for _ in range(18):
        s = r.choice([3, 4, 5, 6, 8]); x = r.randint(20, 1400); y = r.randint(40, 920)
        dx = r.randint(-90, 90); dy = r.randint(-120, 60); t = r.randint(14, 30); w = round(r.uniform(2.5, 6.5), 1); dl = -r.randint(0, 20)
        out += (f'<span class="fly" style="left:{x}px;top:{y}px;width:{s}px;height:{s}px;--dx:{dx}px;--dy:{dy}px;--t:{t}s;--w:{w}s;'
                f'animation-delay:{dl}s,{dl}s;box-shadow:0 0 {s * 3}px {s}px rgba(255,236,190,.28)"></span>')
    return out + '\n'


# ---------------------------------------------------------------- Гараж
# (картинка, название, подпись, статус, фон-позиция для повторов)
GRID = [
    ('dacia', 'Honda Civic EK9', '1.6 · 88 МБ', None, 'size:210%;pos:80% 100%'),
    ('truck', 'Subaru Impreza 22B', '1.3 · 97 МБ', None, 'size:210%;pos:20% 100%'),
    ('lambo', 'Toyota AE86 Trueno', '2.1 · 74 МБ', None, 'size:210%;pos:50% 100%'),
    ('coupe', 'Mini Cooper S 1275', '1.1 · 52 МБ', None, 'size:210%;pos:60% 100%'),
    ('dacia', 'Dacia Sandero Stepway', '1.0 → 1.5', 'update', ''),
    ('mustang', 'Ford Mustang Shelby GT500', '1.2 · 120 МБ', 'lost', ''),
    ('tractor', 'New Holland T7', '2.3 · 96 МБ', None, ''),
    ('lebedka', 'Rally Stage Pack', '0.8 · 410 МБ', 'maybe', 'size:150%;pos:20% 30%'),
    ('truck', 'Kenworth T610 с цистерной', '1.4b → 1.5', 'update', ''),
    ('traffic', 'Traffic Reborn', '3.1 · 54 МБ', None, ''),
    ('station', 'Заправка Diesel', '1.0 · 12 МБ', 'off', ''),
    ('coupe', 'Coastal Highway', '1.1 · 640 МБ', None, 'size:170%;pos:80% 20%'),
    ('lambo', 'Lamborghini Huracán EVO', '1.6 · 88 МБ', None, ''),
    ('lebedka', 'Lebedka Tow', '1.1 · 8 МБ', None, ''),
    ('coupe', 'Falcon RS Coupe', '2.0 · 140 МБ', None, ''),
    ('mustang', 'Skyline R34 GT-R', '1.8 · 110 МБ', None, 'size:190%;pos:30% 60%'),
    ('tractor', 'Mazda RX-7 FD', '1.0 · 83 МБ', None, 'size:200%;pos:30% 40%'),
    ('mustang', 'Alpine Pass', '2.0 · 512 МБ', None, 'size:200%;pos:70% 50%'),
    ('lambo', 'Volga 24 Classic', '1.0 · 64 МБ', None, 'size:190%;pos:20% 70%'),
    ('dacia', 'Nordschleife', '4.2 · 1,2 ГБ', None, 'size:190%;pos:60% 60%'),
]


def tile(x, y, w, pos, img, n, sub, status, crop, sel, cap=True):
    ph = int(w * 0.58)
    extra = ''
    if crop:
        d = dict(p.split(':') for p in crop.split(';'))
        extra = f'background-size:{d["size"]};background-position:{d["pos"]};'
    grey = 'filter:grayscale(1) brightness(.78);' if status == 'off' else ''
    badge = ''
    if status == 'update':
        badge = '<span class="pulse" style="position:absolute;top:10px;right:10px;padding:4px 10px;border-radius:13px;background:rgba(255,255,255,.94);color:#14181f;font-size:12px;font-weight:700">Есть обновление</span>'
    line2 = sub
    if status == 'update':
        line2 = f'<span style="color:#fff;font-weight:600">{sub}</span>'
    if status == 'lost':
        line2 = '<span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:#ffb340;margin-right:7px"></span>Страница пропала'
    if status == 'maybe':
        line2 = '<span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:#ffd36b;margin-right:7px"></span>Возможно, обновлён'
    if status == 'off':
        line2 = 'Выключен'
    return (f'<article class="card" style="position:absolute;left:{x}px;top:{y}px;width:{w}px;animation-delay:{60 + pos * 45}ms">'
            f'<div class="ph pic p-{img}{" sel" if sel else ""}" style="height:{ph}px;border-radius:20px;{grey}{extra}box-shadow:0 4px 18px rgba(6,10,22,.35), inset 0 0 0 1px rgba(255,255,255,.14)">{badge}</div>'
            + (f'<div style="padding:10px 4px 0"><div style="font-size:15px;font-weight:620;letter-spacing:-0.005em;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#fff">{n}</div>'
            f'<div style="margin-top:1px;font-size:12.5px;color:rgba(255,255,255,.7);white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{line2}</div></div>' if cap else '') + '</article>\n')


def navitem(k, label, on, badge=None):
    bg = 'rgba(255,255,255,0.20);box-shadow:0 0 24px rgba(170,205,255,.38)' if on else 'transparent'
    col = '#fff' if on else 'rgba(255,255,255,0.74)'
    bd = (f'<span class="pulse" style="position:absolute;top:3px;right:6px;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;border-radius:9px;background:#fff;color:#111;'
          f'font-size:11px;font-weight:700;display:flex;align-items:center;justify-content:center">{badge}</span>') if badge else ''
    return (f'<a class="nv" href="#" style="position:relative;width:70px;padding:9px 0 7px;border-radius:20px;background:{bg};color:{col};text-decoration:none;display:flex;'
            f'flex-direction:column;align-items:center;gap:4px;font-size:11px;font-weight:{600 if on else 500}">{icon(k)}<span>{label}</span>{bd}</a>')


def chrome(glass, active):
    """Левая панель и кнопка настроек. active: 'garage' или 'settings'."""
    s_on = active == 'settings'
    ring = 'box-shadow:0 0 0 2px rgba(255,255,255,.9), 0 0 26px rgba(170,205,255,.45);' if s_on else ''
    return (f'<nav class="glass lft" aria-label="Разделы" style="left:16px;top:16px;width:84px;height:212px;border-radius:36px;{surface(glass, "Rail", 7)}">'
            f'{rim(glass, "rail")}<div style="position:relative;display:flex;flex-direction:column;align-items:center;gap:6px;padding:16px 0">'
            f'{navitem("grid", "Гараж", active == "garage")}{navitem("upd", "Обновления", False, "3")}{navitem("search", "Поиск", False)}</div></nav>\n'
            f'<a class="glass lft nv tip" data-tip="Настройки" href="#" aria-label="Настройки" style="left:30px;top:888px;width:56px;height:56px;border-radius:28px;display:flex;align-items:center;'
            f'justify-content:center;color:{"#fff" if s_on else "rgba(255,255,255,.86)"};text-decoration:none;{ring}{surface(glass, "Gear", 7)}">'
            f'{rim(glass, "gear")}<span style="position:relative">{icon("gear", 25, 1.6)}</span></a>\n')


def info_rows(items):
    return ''.join(f'<div style="display:flex;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.09)" if i else ""}"><span style="flex:0 0 88px;color:rgba(255,255,255,0.64)">{k}</span>'
                   f'<span style="flex:1;min-width:0;font-weight:500">{v}</span></div>' for i, (k, v) in enumerate(items))


def garage(glass):
    filters = (lens('lgRail', 'rail', 84, 212, 70) + lens('lgGear', 'gear', 56, 56, 56) + lens('lgBar', 'bar', 964, 52, 80) + lens('lgInsp', 'insp', 328, 928, 96)) if glass else ''
    body = ''
    cw = 308
    rowh = int(cw * 0.58) + 62
    for pos, t in enumerate(GRID):
        col, row = pos % 4, pos // 4
        body += tile(116 + col * (cw + 20), -96 + row * (rowh + 20), cw, pos, *t[:5], sel=(pos == 4), cap=(col < 3))
    body += flies() + chrome(glass, 'garage')
    segs = ''.join(f'<label class="seg"><input type="radio" name="f" {"checked" if i == 0 else ""}><span>{t}{(" <b style=\"opacity:.7;font-weight:600\">" + c + "</b>") if c else ""}</span></label>'
                   for i, (t, c) in enumerate([('Все', None), ('Включённые', None), ('Нужно обновить', '3'), ('Не проверяются', '12')]))
    body += (f'<header class="glass top" style="left:116px;top:16px;width:964px;height:52px;border-radius:26px;{surface(glass, "Bar", 7)}">{rim(glass, "bar")}'
             f'<div style="position:relative;display:flex;align-items:center;gap:14px;height:100%;padding:0 12px 0 22px">'
             f'<span style="font-size:18px;font-weight:650;letter-spacing:-0.01em">Гараж</span><span style="color:rgba(255,255,255,0.64);font-size:13px">128 модов</span><span style="flex:1"></span>'
             f'<div role="radiogroup" aria-label="Фильтр" style="display:flex;gap:2px;padding:3px;border-radius:19px;background:rgba(255,255,255,0.10)">{segs}</div>'
             f'<label style="display:flex;align-items:center;gap:8px;width:200px;height:34px;padding:0 12px;box-sizing:border-box;border-radius:17px;background:rgba(255,255,255,0.12);color:rgba(255,255,255,0.72)">{icon("search", 15)}'
             f'<input type="text" placeholder="Найти в гараже" aria-label="Найти среди установленных модов" style="flex:1;min-width:0;border:0;outline:0;background:transparent;color:#fff;font:inherit;font-size:13px"></label>'
             f'</div></header>\n')
    inside = info_rows([('Машина', 'Dacia Sandero Stepway'), ('Скрипты', 'Нет, безопасно')])
    rows = info_rows([('Размер', '142 МБ, на сайте 151 МБ'), ('Установлен', '12 августа'), ('Следим на', 'Modland, ModsGaming'), ('Проверен', 'сегодня, 14:02')])
    copies = ''.join(f'<div style="display:flex;align-items:center;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.09)" if i else ""}"><div style="flex:1"><div style="font-weight:500">{w}</div>'
                     f'<div style="font-size:12.5px;color:rgba(255,255,255,0.64)">{nn}</div></div><button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;text-decoration:underline;text-underline-offset:3px;cursor:pointer">Вернуть</button></div>'
                     for i, (w, nn) in enumerate([('12 августа, 21:40', 'Версия 1.0, 142 МБ'), ('3 июня, 09:12', 'Версия 0.9, 138 МБ')]))
    box = 'display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.09);overflow:hidden'
    cap = 'font-size:13px;font-weight:600;color:rgba(255,255,255,0.66);padding:0 4px 8px'
    body += (f'<aside class="glass insp" aria-label="Выбранный мод" style="left:1096px;top:16px;width:328px;height:928px;border-radius:32px;{'background:rgba(24,34,52,.44);' if glass else ''}{surface(glass, "Insp", 14)}">{rim(glass, "insp")}'
             f'<div style="position:relative;display:flex;flex-direction:column;gap:18px;padding:22px 22px 24px;height:100%;box-sizing:border-box">'
             f'<div class="pic p-dacia" style="height:150px;border-radius:20px;box-shadow:0 10px 30px rgba(6,10,22,0.4), 0 0 40px rgba(150,195,255,.22), inset 0 0 0 1px rgba(255,255,255,0.2)"></div>'
             f'<div><div style="font-size:24px;font-weight:700;letter-spacing:-0.02em;line-height:1.15">Dacia Sandero Stepway</div><div style="margin-top:3px;color:rgba(255,255,255,0.7)">kuro_garage</div></div>'
             f'<div style="display:flex;flex-direction:column;gap:8px"><button type="button" class="cta btn" style="height:44px;border:0;border-radius:22px;background:#fff;color:#111;font-size:15px;font-weight:650;cursor:pointer;box-shadow:0 0 30px rgba(170,205,255,.42), 0 6px 18px rgba(6,10,22,0.3)">Обновить до 1.5</button>'
             f'<div style="text-align:center;font-size:12.5px;color:rgba(255,255,255,0.7)">Сейчас стоит 1.0. Новая версия есть на Modland и ModsGaming.</div></div>'
             f'<div style="{box}">{rows}</div>'
             f'<div><div style="{cap}">Внутри архива</div><div style="{box}">{inside}</div></div>'
             f'<div><div style="{cap}">Прошлые версии</div><div style="{box}">{copies}</div></div>'
             f'<div style="margin-top:auto;display:flex;flex-wrap:wrap;gap:10px 22px;padding:0 4px">'
             f'<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Открыть страницу</button>'
             f'<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Выключить</button>'
             f'<button type="button" style="padding:0;border:0;background:transparent;color:#ffb0a8;font-size:13.5px;font-weight:600;cursor:pointer">Удалить</button>'
             f'<span style="font-size:12.5px;color:rgba(255,255,255,.6);align-self:center">Можно вернуть позже</span></div></div></aside>\n')
    return page('Гараж' if glass else 'Гараж без жидкого стекла', filters, body, glass)


# ---------------------------------------------------------------- Настройки
def toggle(tid, checked, title, text):
    return (f'<input id="{tid}" class="tgl" type="checkbox" {"checked" if checked else ""}>'
            f'<label for="{tid}" class="trow" style="padding:18px 20px;border-radius:20px;background:rgba(255,255,255,.09)">'
            f'<span style="flex:1"><span style="display:block;font-size:16px;font-weight:620">{title}</span>'
            f'<span style="display:block;margin-top:4px;font-size:13px;color:rgba(255,255,255,.7);max-width:520px">{text}</span></span>'
            f'<span class="trk"><i></i></span></label>')


def preview(glass_on):
    """Мини-превью: стеклянная панель над карточкой. Показывает, чем отличаются режимы."""
    def bar(on):
        f = f'blur(5px) url(#lgPv) saturate(1.4)' if on else 'blur(26px) saturate(1.15)'
        tint = 'rgba(30,40,58,0.26)' if on else 'rgba(34,46,68,0.66)'
        sh = 'inset 0 0 0 1px rgba(255,255,255,0.12)' if on else 'inset 0 0 0 1px rgba(255,255,255,0.16), inset 0 1px 0 rgba(255,255,255,0.26)'
        r = f'<div class="rim" style="background-image:url({A("pv_rim.png")})"></div>' if on else ''
        return (f'<div style="position:absolute;left:24px;top:22px;width:420px;height:56px;border-radius:28px;background:{tint};box-shadow:{sh};-webkit-backdrop-filter:{f};backdrop-filter:{f};overflow:visible">{r}'
                f'<div style="position:relative;display:flex;align-items:center;gap:12px;height:100%;padding:0 22px;font-size:14px"><b style="font-weight:650">Гараж</b>'
                f'<span style="color:rgba(255,255,255,.64);font-size:13px">128 модов</span><span style="flex:1"></span><span style="padding:5px 12px;border-radius:15px;background:rgba(255,255,255,.22);font-size:13px;font-weight:650">Все</span></div></div>')
    label_on = 'Жидкое стекло: края панели преломляют картинку'
    label_off = 'Матовое стекло: картинка размыта, края ровные'
    cap = lambda t: f'<span style="position:absolute;left:4px;top:0;font-size:13px;color:rgba(255,255,255,.72)">{t}</span>'
    return (f'<div class="pvbox" style="margin-top:14px"><div style="position:relative;height:178px;border-radius:22px;overflow:hidden;box-shadow:inset 0 0 0 1px rgba(255,255,255,.14)">'
            f'<div class="pic p-wall" style="position:absolute;inset:0;background-position:center 70%"></div>'
            f'<div class="pic p-mustang" style="position:absolute;left:34px;top:54px;width:300px;height:140px;border-radius:16px"></div>'
            f'<div class="pic p-lambo" style="position:absolute;left:348px;top:54px;width:260px;height:140px;border-radius:16px"></div>'
            f'<div class="pv pv-on" style="inset:0">{bar(True)}</div><div class="pv pv-off" style="inset:0">{bar(False)}</div></div>'
            f'<div style="position:relative;height:26px;margin-top:10px"><div class="pv pv-on" style="inset:0">{cap(label_on)}</div><div class="pv pv-off" style="inset:0">{cap(label_off)}</div></div></div>')


def settings():
    filters = lens('lgRail', 'rail', 84, 212, 70) + lens('lgGear', 'gear', 56, 56, 56) + lens('lgSet', 'set', 1308, 640, 96) + lens('lgPv', 'pv', 420, 56, 80)
    nav = ''.join(
        f'<a class="nv" href="#" style="display:flex;align-items:center;height:42px;padding:0 16px;border-radius:21px;color:{"#fff" if i == 0 else "rgba(255,255,255,.76)"};text-decoration:none;font-size:14.5px;font-weight:{650 if i == 0 else 500};'
        f'background:{"rgba(255,255,255,.20);box-shadow:0 0 22px rgba(170,205,255,.3)" if i == 0 else "transparent"}">{t}</a>'
        for i, t in enumerate(['Внешний вид', 'Загрузка модов', 'Папки', 'Обновления', 'Сайты']))
    body = flies() + chrome(True, 'settings')
    body += (f'<section class="glass top" aria-label="Настройки" style="left:116px;top:16px;width:1308px;height:640px;border-radius:32px;background:rgba(24,34,52,.44);{surface(True, "Set", 14)}">{rim(True, "set")}'
             f'<div style="position:relative;display:flex;height:100%;padding:36px 44px 36px 36px;box-sizing:border-box;gap:44px">'
             f'<div style="flex:0 0 230px"><div style="font-size:28px;font-weight:700;letter-spacing:-0.02em;margin:0 0 24px 16px">Настройки</div><nav style="display:flex;flex-direction:column;gap:4px" aria-label="Разделы настроек">{nav}</nav></div>'
             f'<div style="flex:1;min-width:0;max-width:760px"><div style="font-size:22px;font-weight:700;letter-spacing:-0.01em;margin:6px 0 20px">Внешний вид</div>'
             f'{toggle("lg", True, "Жидкое стекло", "Панели преломляют фон, как настоящее стекло. Если интерфейс работает медленно, выключите: вид останется прежним, пропадёт только преломление.")}'
             f'{preview(True)}'
                          f'{toggle("an", True, "Анимированный фон", "Фото медленно меняет цвет, над ним плавают огоньки. Выключите, чтобы экономить ресурсы или если движение мешает.")}'
             f'<div style="margin-top:22px;font-size:13px;color:rgba(255,255,255,.64);max-width:560px">Если в Windows включено «Уменьшить анимацию» или «Уменьшить прозрачность», программа учтёт это сама, даже когда здесь всё включено.</div>'
             f'</div></div></section>\n')
    return page('Настройки', filters, body, True)


if __name__ == '__main__':
    open(f'{OUT}/Main.dc.html', 'w').write(garage(True))
    open(f'{OUT}/MainMatte.dc.html', 'w').write(garage(False))
    open(f'{OUT}/Settings.dc.html', 'w').write(settings())
    for f in sorted(os.listdir(OUT)):
        print(f, os.path.getsize(f'{OUT}/{f}'))
