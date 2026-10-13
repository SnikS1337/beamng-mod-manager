import sys, os, random, json
URL = json.load(open('urls.json')) if os.path.exists('urls.json') and '--blob' in sys.argv else {}
import base64
def A(n):
    if '--inline' in sys.argv:
        return 'data:image/png;base64,'+base64.b64encode(open(n,'rb').read()).decode()
    return URL.get(n, n)
OUT = 'out'
os.makedirs(OUT, exist_ok=True)

FONT = "'Segoe UI Variable Text','Segoe UI Variable Display','SF Pro Text',system-ui,-apple-system,'Segoe UI',sans-serif"

def art(h, variant=0):
    w=lambda d:(h+d)%360
    return (f"radial-gradient(120% 90% at {18+variant*9}% 12%, hsl({w(25)} 72% 62%) 0%, transparent 55%),"
            f"radial-gradient(110% 100% at 85% 80%, hsl({w(-35)} 62% 34%) 0%, transparent 60%),"
            f"radial-gradient(90% 70% at 55% 108%, hsl({w(70)} 58% 52% / 0.85) 0%, transparent 60%), hsl({h} 42% 18%)")

def lens(fid, name, w, h, scale=46, blur=0):
    return f'''<filter id="{fid}" x="0" y="0" width="{w}" height="{h}" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
<feImage href="{A(name+'.png')}" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" result="map"/>
<feDisplacementMap in="SourceGraphic" in2="map" scale="{scale}" xChannelSelector="R" yChannelSelector="G"/>
</filter>'''

HEAD = '''<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<title>{title}</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<style>
body{{margin:0;background:#0b0c0f}}
button{{font-family:inherit}}
a:focus-visible,button:focus-visible,input:focus-visible{{outline:3px solid #fff;outline-offset:2px}}
input::placeholder{{color:rgba(255,255,255,0.58)}}
.glass{{position:absolute;color:#f4f5f7;background:rgba(14,16,22,0.56);box-shadow:0 24px 60px rgba(0,0,0,0.45), inset 0 0 0 1px rgba(255,255,255,0.10)}}
.rim{{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-repeat:no-repeat;background-size:100% 100%}}
@media (prefers-reduced-transparency:reduce){{.glass{{-webkit-backdrop-filter:none !important;backdrop-filter:none !important;background:#1b1d24 !important}}}}
</style>
</helmet>
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
{filters}
</defs></svg>
<div style="position:relative;width:1440px;height:960px;overflow:hidden;background:radial-gradient(900px 700px at 25% 70%, hsl(215 55% 22%), transparent 70%), radial-gradient(800px 600px at 90% 10%, hsl(300 40% 20%), transparent 70%), #0b0c0f;color:#f4f5f7;font-family:{font};font-size:14px;line-height:1.35">
'''
TAIL = '''
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

ICON = {
 'grid':'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
 'upd':'M12 3a9 9 0 100 18 9 9 0 000-18zM12 8v8m0 0l-3-3m3 3l3-3',
 'search':'M11 4a7 7 0 100 14 7 7 0 000-14zM21 21l-4.3-4.3',
 'gear':'M4 7h10M18 7h2M4 17h2M10 17h10M14 5v4M6 15v4',
}
def icon(d, size=22, sw=1.7):
    return f'<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="{d}"/></svg>'

def rail(active):
    items=[('grid','Гараж','Main.dc.html',None),('upd','Обновления','Updates.dc.html','3'),('search','Поиск','Search.dc.html',None),('gear','Настройки','Main.dc.html',None)]
    out=f'''<nav class="glass" aria-label="Разделы" style="left:16px;top:16px;width:84px;height:404px;border-radius:36px;-webkit-backdrop-filter:blur(7px) url(#lgRail) saturate(1.5);backdrop-filter:blur(7px) url(#lgRail) saturate(1.5)">
<div class="rim" style="background-image:url({A('rail_rim.png')})"></div>
<div style="position:relative;display:flex;flex-direction:column;align-items:center;gap:6px;padding:16px 0">
<svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.9" stroke-linecap="round" aria-hidden="true" style="margin-bottom:10px"><circle cx="12" cy="12" r="9.5"/><circle cx="12" cy="12" r="2.2" fill="#fff"/><path d="M12 12L12 4M12 12L19.6 9.5M12 12L16.7 18.5M12 12L7.3 18.5M12 12L4.4 9.5"/></svg>
'''
    for k,label,href,badge in items:
        on = (href==active and label!='Настройки')
        bg='rgba(255,255,255,0.20)' if on else 'transparent'
        col='#fff' if on else 'rgba(255,255,255,0.72)'
        wt=600 if on else 500
        b=f'<span style="position:absolute;top:3px;right:6px;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;border-radius:9px;background:#fff;color:#111;font-size:11px;font-weight:700;display:flex;align-items:center;justify-content:center">{badge}</span>' if badge else ''
        out+=f'''<a href="{href}" style="position:relative;width:70px;padding:9px 0 7px;border-radius:20px;background:{bg};color:{col};text-decoration:none;display:flex;flex-direction:column;align-items:center;gap:4px;font-size:10.5px;font-weight:{wt}">{icon(ICON[k])}<span>{label}</span>{b}</a>
'''
    out+='</div></nav>\n'
    return out

def bar(title, count, seg_items, active_idx, placeholder='Поиск'):
    seg=''
    for i,(l,c) in enumerate(seg_items):
        on=i==active_idx
        seg+=f'<button type="button" style="padding:6px 14px;border:0;border-radius:15px;background:{"rgba(255,255,255,0.22)" if on else "transparent"};color:{"#fff" if on else "rgba(255,255,255,0.74)"};font-size:13px;font-weight:{600 if on else 500};cursor:pointer">{l}{(" <span style=opacity:.7>"+c+"</span>") if c else ""}</button>'
    return f'''<header class="glass" style="left:116px;top:16px;width:964px;height:52px;border-radius:26px;-webkit-backdrop-filter:blur(7px) url(#lgBar) saturate(1.5);backdrop-filter:blur(7px) url(#lgBar) saturate(1.5)">
<div class="rim" style="background-image:url({A('bar_rim.png')})"></div>
<div style="position:relative;display:flex;align-items:center;gap:14px;height:100%;padding:0 12px 0 22px">
<span style="font-size:18px;font-weight:650;letter-spacing:-0.01em">{title}</span><span style="color:rgba(255,255,255,0.62);font-size:13px">{count}</span>
<span style="flex:1"></span>
<div role="tablist" style="display:flex;gap:2px;padding:3px;border-radius:19px;background:rgba(255,255,255,0.10)">{seg}</div>
<label style="display:flex;align-items:center;gap:8px;width:200px;height:34px;padding:0 12px;box-sizing:border-box;border-radius:17px;background:rgba(255,255,255,0.12);color:rgba(255,255,255,0.7)">{icon(ICON['search'],15,2)}<input type="text" placeholder="{placeholder}" aria-label="{placeholder}" style="flex:1;min-width:0;background:transparent;border:0;font:inherit;font-size:13.5px;color:#fff"></label>
</div></header>
'''

def tile(x,y,w,h,name,sub,hue,v=0,badge=None,sel=False,off=False,cap=True):
    ring='outline:2px solid #fff;outline-offset:3px;' if sel else ''
    flt='filter:grayscale(1) brightness(.8);' if off else ''
    b=''
    if badge=='update': b='<span style="position:absolute;top:10px;right:10px;display:flex;align-items:center;gap:5px;padding:4px 9px 4px 7px;border-radius:13px;background:rgba(255,255,255,0.92);color:#111;font-size:12px;font-weight:700"><svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="#111" stroke-width="3.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 19V5M5 12l7-7 7 7"/></svg>'+sub.split('→')[-1].strip()+'</span>'
    if badge=='lost': b='<span style="position:absolute;top:10px;right:10px;padding:4px 9px;border-radius:13px;background:#ff9f0a;color:#1a1100;font-size:12px;font-weight:700">Страница пропала</span>'
    if badge=='maybe': b='<span style="position:absolute;top:10px;right:10px;padding:4px 9px;border-radius:13px;background:rgba(255,255,255,0.92);color:#111;font-size:12px;font-weight:700">Возможно обновление</span>'
    subtxt = sub.split('→')[0].strip()+' → '+sub.split('→')[-1].strip() if badge=='update' else sub
    return f'''<article style="position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;border-radius:18px;overflow:hidden;background:{art(hue,v)};box-shadow:0 2px 14px rgba(0,0,0,0.5);{ring}">
<div style="position:absolute;inset:0;{flt}background:{art(hue,v)}"></div>
<div style="position:absolute;inset:0;background:linear-gradient(180deg,rgba(0,0,0,0) 50%,rgba(0,0,0,0.62) 100%)"></div>{b}
{'<div style="position:absolute;left:14px;right:14px;bottom:11px"><div' if cap else '<div style="display:none"><div'} style="font-size:15px;font-weight:650;color:#fff;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;{'opacity:.7' if off else ''}">{name}</div><div style="font-size:12.5px;color:rgba(255,255,255,0.78)">{subtxt}</div></div>
</article>
'''

def write(name, title, filters, body):
    html = HEAD.format(title=title, filters=filters, font=FONT) + body + TAIL
    open(f'{OUT}/{name}','w').write(html)

# ---------------- Main ----------------
MODS=[('Audi A3 8V','1.0 → 1.5',215,'update'),('BMW E36 Touring','2.3 · 96 МБ',28,None),('Kansai Drift Track','Выключен',265,'off'),
('Pickup Bruiser 4x4','1.4b → 1.5',95,'update'),('Ford Mustang GT 1969','1.2 · 120 МБ',355,'lost'),('Realistic Engine Sounds','3.1 · 54 МБ',300,None),
('Coastal Highway','1.1 · 640 МБ',190,None),('Rally Stage Pack','0.8 · 410 МБ',40,'maybe'),('Volga 24 Classic','1.0 · 64 МБ',150,None),
('Honda Civic EK9','1.6 · 88 МБ',330,None),('Alpine Pass','2.0 · 512 МБ',205,None),('Lancia Delta Integrale','0.9 · 101 МБ',5,None),
('Subaru Impreza 22B','1.3 · 97 МБ',235,None),('Nordschleife Touristenfahrten','4.2 · 1,2 ГБ',120,None),('Toyota AE86 Trueno','2.1 · 74 МБ',60,None),
('Mazda RX-7 FD','1.0 · 83 МБ',15,None),('Gravel Canyon','0.5 · 288 МБ',25,None),('Skyline R34 GT-R','1.8 · 110 МБ',250,None),
('Dirt Oval Pack','1.0 · 156 МБ',340,None),('Mini Cooper S 1275','1.1 · 52 МБ',175,None)]
def main():
    filters = lens('lgRail','rail',84,404) + lens('lgBar','bar',964,52) + lens('lgInsp','insp',328,928)
    body=''
    cw,ch=315,230
    order=[1,2,3,6, 0,4,5,7, 8,9,10,11, 12,13,14,15, 16,17,18,19]
    for pos,i in enumerate(order):
        n,s,h,bb=MODS[i]
        col=pos%4; row=pos//4
        x=116+col*(cw+16); y=-110+row*(ch+16)
        badge = bb if bb in ('update','lost','maybe') else None
        body+=tile(x,y,cw,ch,n,s,h,v=i%3,badge=badge,sel=(i==0),off=(bb=='off'),cap=(col<3))
    body+=rail('Main.dc.html')
    body+=bar('Гараж','128 модов',[('Все',None),('Включённые',None),('Нужно обновить','3'),('Без привязки','12')],0,'Найти мод')
    # inspector
    body+=f'''<aside class="glass" aria-label="Выбранный мод" style="left:1096px;top:16px;width:328px;height:928px;border-radius:32px;-webkit-backdrop-filter:blur(14px) url(#lgInsp) saturate(1.5);backdrop-filter:blur(14px) url(#lgInsp) saturate(1.5)">
<div class="rim" style="background-image:url({A('insp_rim.png')})"></div>
<div style="position:relative;display:flex;flex-direction:column;gap:18px;padding:22px 22px 24px;height:100%;box-sizing:border-box">
<div style="height:176px;border-radius:20px;background:{art(215,0)};box-shadow:0 8px 24px rgba(0,0,0,0.4), inset 0 0 0 1px rgba(255,255,255,0.18)"></div>
<div><div style="font-size:24px;font-weight:700;letter-spacing:-0.02em;line-height:1.15">Audi A3 8V</div><div style="margin-top:3px;color:rgba(255,255,255,0.68)">kuro_garage</div></div>
<div style="display:flex;flex-direction:column;gap:8px">
<button type="button" style="height:44px;border:0;border-radius:22px;background:#fff;color:#111;font-size:15px;font-weight:650;cursor:pointer;box-shadow:inset 0 1px 0 #fff, 0 6px 18px rgba(0,0,0,0.35)">Обновить до 1.5</button>
<div style="text-align:center;font-size:12.5px;color:rgba(255,255,255,0.68)">Сейчас стоит 1.0. Новая версия есть на Modland и ModsGaming.</div></div>
<div style="display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.08);overflow:hidden">
''' + ''.join(f'<div style="display:flex;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.08)" if i else ""}"><span style="flex:0 0 88px;color:rgba(255,255,255,0.62)">{k}</span><span style="flex:1;min-width:0;font-weight:500">{v}</span></div>' for i,(k,v) in enumerate([('Размер','142 МБ, на сайте 151 МБ'),('Установлен','12 августа'),('Привязки','Modland, ModsGaming'),('Проверен','сегодня, 14:02')])) + '''
</div>
<div><div style="font-size:13px;font-weight:600;color:rgba(255,255,255,0.62);padding:0 4px 8px">Копии</div>
<div style="display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.08);overflow:hidden">
''' + ''.join(f'<div style="display:flex;align-items:center;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.08)" if i else ""}"><div style="flex:1"><div style="font-weight:500">{w}</div><div style="font-size:12.5px;color:rgba(255,255,255,0.62)">{nn}</div></div><button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;text-decoration:underline;text-underline-offset:3px;cursor:pointer">Откатить</button></div>' for i,(w,nn) in enumerate([('12 августа, 21:40','Версия 1.0, 142 МБ'),('3 июня, 09:12','Версия 0.9, 138 МБ')])) + '''
</div></div>
<div style="margin-top:auto;display:flex;flex-wrap:wrap;gap:10px 22px;padding:0 4px">
<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Открыть страницу</button>
<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Выключить</button>
<button type="button" style="padding:0;border:0;background:transparent;color:#ff8a80;font-size:13.5px;font-weight:600;cursor:pointer">Удалить в копии</button>
</div></div></aside>
'''
    write('Main.dc.html','Гараж',filters,body)

if __name__=='__main__':
    main()
