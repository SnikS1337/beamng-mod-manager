import sys, random
import build as B
A=B.A; lens=B.lens; FONT=B.FONT

CSS = '''
:root{--ease:cubic-bezier(.22,.8,.24,1);--fast:.25s;--mid:.55s;--slow:1s;--beat:6s}
body{margin:0;background:#1d2533}
button{font-family:inherit}
a:focus-visible,button:focus-visible,input:focus-visible{outline:3px solid #fff;outline-offset:2px}
input::placeholder{color:rgba(255,255,255,0.62)}
.glass{position:absolute;color:#f6f8fb;background:rgba(30,40,58,0.34);box-shadow:0 22px 56px rgba(8,12,24,0.38), inset 0 0 0 1px rgba(255,255,255,0.12)}
.rim{position:absolute;inset:0;border-radius:inherit;pointer-events:none;background-repeat:no-repeat;background-size:100% 100%}
.blob{position:absolute;border-radius:50%;will-change:transform}
@keyframes d1{0%{transform:translate(0,0) scale(1)}50%{transform:translate(140px,-90px) scale(1.18)}100%{transform:translate(-70px,70px) scale(1.04)}}
@keyframes d2{0%{transform:translate(0,0) scale(1.05)}50%{transform:translate(-160px,110px) scale(.92)}100%{transform:translate(60px,-60px) scale(1.12)}}
@keyframes d3{0%{transform:translate(0,0) scale(1)}50%{transform:translate(180px,-120px) scale(1.2)}100%{transform:translate(-90px,-30px) scale(.95)}}
.b1{animation:d1 48s ease-in-out infinite alternate}
.b2{animation:d2 48s ease-in-out infinite alternate;animation-delay:-12s}
.b3{animation:d3 48s ease-in-out infinite alternate;animation-delay:-24s}
@keyframes kb{0%{transform:scale(1.04) translate(0,0)}100%{transform:scale(1.12) translate(-22px,-14px)}}
.wall{position:absolute;inset:-30px;animation:kb 48s ease-in-out infinite alternate}
@keyframes flow{0%{background-position:0% 50%}100%{background-position:300% 50%}}
.grade{position:absolute;inset:0;background:linear-gradient(110deg,hsla(212,75%,52%,.55),hsla(262,55%,56%,.45),hsla(24,85%,58%,.5),hsla(190,70%,50%,.45),hsla(212,75%,52%,.55));background-size:300% 100%;animation:flow 48s linear infinite;mix-blend-mode:soft-light}
@keyframes dropin{0%{opacity:0;transform:translateY(-18px) scale(.97)}100%{opacity:1;transform:none}}
@keyframes leftin{0%{opacity:0;transform:translateX(-24px)}100%{opacity:1;transform:none}}
.top{animation:dropin .9s .15s var(--ease) both}
.lft{animation:leftin .9s .05s var(--ease) both}
.nv{transition:background var(--fast) var(--ease), transform var(--fast) var(--ease)}
.nv:hover{background:rgba(255,255,255,.14) !important;transform:translateY(-1px)}
.seg input{position:absolute;opacity:0;pointer-events:none}
.seg span{display:block;padding:6px 14px;border-radius:15px;font-size:13px;font-weight:550;color:rgba(255,255,255,.76);cursor:pointer;transition:background var(--mid) var(--ease), color var(--fast)}
.seg span:hover{background:rgba(255,255,255,.10)}
.seg input:checked + span{background:rgba(255,255,255,.24);color:#fff;font-weight:650;box-shadow:0 0 18px rgba(170,205,255,.25)}
.seg input:focus-visible + span{outline:3px solid #fff;outline-offset:1px}
.btn{transition:transform var(--fast) var(--ease), box-shadow var(--mid) var(--ease), background var(--fast)}
.btn:hover{transform:translateY(-1px)}
.btn:active{transform:scale(.97)}
@keyframes fly{0%{transform:translate(0,0)}100%{transform:translate(var(--dx),var(--dy))}}
@keyframes tw{0%,100%{opacity:.15}50%{opacity:.95}}
.fly{position:absolute;border-radius:50%;mix-blend-mode:screen;pointer-events:none;background:radial-gradient(circle,#fff 0,rgba(255,244,214,.9) 28%,rgba(255,226,170,0) 70%);animation:fly var(--t) ease-in-out infinite alternate, tw var(--w) ease-in-out infinite}
@keyframes rise{0%{opacity:0;transform:translateY(22px) scale(.96)}100%{opacity:1;transform:none}}
.card{animation:rise .8s cubic-bezier(.2,.8,.2,1) both}
.ph{position:relative;overflow:hidden;transition:transform var(--mid) var(--ease), box-shadow var(--mid) var(--ease)}
.ph::after{content:"";position:absolute;inset:0;background:linear-gradient(115deg,transparent 35%,rgba(255,255,255,.28) 50%,transparent 65%);background-size:260% 100%;background-position:130% 0;transition:background-position .9s cubic-bezier(.2,.8,.2,1)}
.card:hover .ph{transform:translateY(-5px) scale(1.025);box-shadow:0 18px 40px rgba(6,10,22,.5), 0 0 34px rgba(170,205,255,.25)}
.card:hover .ph::after{background-position:-30% 0}
@keyframes breathe{0%,100%{box-shadow:0 0 0 2px rgba(255,255,255,.92), 0 10px 34px rgba(140,190,255,.30)}50%{box-shadow:0 0 0 2px rgba(255,255,255,.92), 0 14px 56px rgba(140,190,255,.66)}}
.sel{animation:breathe var(--beat) ease-in-out infinite}
@keyframes pulse{0%,40%{box-shadow:0 0 0 0 rgba(255,255,255,.75)}75%,100%{box-shadow:0 0 0 10px rgba(255,255,255,0)}}
.pulse{animation:pulse var(--beat) ease-out infinite}
@keyframes slidein{0%{opacity:0;transform:translateX(60px)}100%{opacity:1;transform:none}}
.insp>div>*{flex-shrink:0}
.tip::before{content:attr(data-tip);position:absolute;left:calc(100% + 12px);top:50%;transform:translateY(-50%) translateX(-6px);padding:7px 12px;border-radius:14px;background:rgba(20,28,44,.82);color:#fff;font-size:12.5px;font-weight:550;white-space:nowrap;opacity:0;pointer-events:none;transition:opacity var(--fast) var(--ease), transform var(--fast) var(--ease);box-shadow:0 8px 24px rgba(6,10,22,.4)}
.tip:hover::before,.tip:focus-visible::before{opacity:1;transform:translateY(-50%)}
.insp{animation:slidein 1s .35s cubic-bezier(.2,.8,.2,1) both}
@keyframes shine{0%,40%{background-position:-120% 0}75%,100%{background-position:220% 0}}
.cta{position:relative;overflow:hidden}
.cta::after{content:"";position:absolute;inset:0;background:linear-gradient(110deg,transparent 40%,rgba(120,170,255,.45) 50%,transparent 60%);background-size:200% 100%;animation:shine var(--beat) var(--ease) infinite;pointer-events:none}
@keyframes dot{0%,100%{opacity:.55;transform:scale(.85)}50%{opacity:1;transform:scale(1.15)}}
.dot{animation:dot var(--beat) ease-in-out infinite}
@media (prefers-reduced-motion:reduce){.b1,.b2,.b3,.wall,.grade,.top,.lft,.fly,.card,.sel,.pulse,.insp,.cta::after,.dot{animation:none !important}.fly{opacity:.5}}
@media (prefers-reduced-transparency:reduce){.glass{-webkit-backdrop-filter:none !important;backdrop-filter:none !important;background:#2a3446 !important}}
'''

def head(title, filters):
    WALL=scene(1440,960,3,214,sat=36,road=True,bokeh=True,wall=True)
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
<style>{CSS}</style>
</helmet>
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
{filters}
</defs></svg>
<div style="position:relative;width:1440px;height:960px;overflow:hidden;background:linear-gradient(165deg,hsl(220 24% 30%) 0%,hsl(212 26% 22%) 100%);color:#f6f8fb;font-family:{FONT};font-size:14px;line-height:1.35">
<div class="wall">{WALL}</div>
<div class="grade"></div>
<div style="position:absolute;inset:0;background:linear-gradient(180deg,rgba(12,18,32,.30),rgba(12,18,32,.52))"></div>
<div class="blob b1" style="left:-220px;top:380px;width:760px;height:760px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(200,60%,56%,.30) 0%,transparent 66%)"></div>
<div class="blob b2" style="left:760px;top:-300px;width:860px;height:860px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(250,45%,62%,.26) 0%,transparent 66%)"></div>
<div class="blob b3" style="left:380px;top:560px;width:640px;height:640px;mix-blend-mode:screen;background:radial-gradient(circle,hsla(30,70%,62%,.20) 0%,transparent 66%)"></div>
'''
TAILH = '''
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":1440,"height":960}}'>
class Component extends DCLogic {
  renderVals() { return {}; }
}
</script>
</body>
</html>
'''


import math
_sid=[0]
def scene(w,h,seed,hue,sat=34,road=False,bokeh=False,wall=False):
    r=random.Random(seed); _sid[0]+=1; k=_sid[0]
    hy=h*(0.60 if not road else 0.56)
    def ridge(base,amp,jag):
        terms=[(r.uniform(.4,1)*amp,r.uniform(1.2,3.5)*math.pi/w,r.uniform(0,6.28)) for _ in range(4)]
        pts=[]
        for i in range(0,29):
            x=w*i/28
            y=base-sum(a*math.sin(f*x+p) for a,f,p in terms)/2.2+r.uniform(-jag,jag)
            pts.append((x,y))
        return 'M0 %d '%h+' '.join('L%.0f %.0f'%p for p in pts)+' L%d %d Z'%(w,h)
    L=lambda l,s=sat:f'hsl({hue} {s}% {l}%)'
    layers=''
    for j,(base,amp,lig,op) in enumerate([(hy-h*.16,h*.12,50,.55),(hy-h*.07,h*.10,38,.75),(hy+h*.005,h*.07,26,1)]):
        layers+=f'<path d="{ridge(base,amp,h*.006)}" fill="{L(lig,sat-6)}" opacity="{op}"/>'
    gx=w*r.uniform(.28,.72)
    ground=f'<rect x="0" y="{hy:.0f}" width="{w}" height="{h-hy:.0f}" fill="url(#g{k})"/>'
    rd=''
    if road:
        vx=gx; top=hy+h*.012
        rd=(f'<polygon points="{vx-w*.012:.0f},{top:.0f} {vx+w*.012:.0f},{top:.0f} {w*.78:.0f},{h} {w*.22:.0f},{h}" fill="url(#r{k})"/>'
            f'<polygon points="{vx-1:.0f},{top:.0f} {vx+1:.0f},{top:.0f} {vx+w*.012:.0f},{h} {vx-w*.012:.0f},{h}" fill="hsla({hue+30},60%,85%,.18)"/>')
    bk=''
    if bokeh:
        for _ in range(16):
            cx=r.uniform(0,w); cy=r.uniform(h*.62,h*.98); rr=r.uniform(h*.012,h*.05)
            bk+=f'<circle cx="{cx:.0f}" cy="{cy:.0f}" r="{rr:.0f}" fill="hsla({r.choice([hue+30,40,hue])},70%,82%,{r.uniform(.07,.2):.2f})"/>'
        bk=f'<g filter="url(#bl{k})">{bk}</g>'
    return (f'<svg viewBox="0 0 {w} {h}" preserveAspectRatio="xMidYMid slice" style="position:absolute;inset:0;width:100%;height:100%;display:block" aria-hidden="true"><defs>'
      f'<linearGradient id="s{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl({hue} {sat}% 20%)"/><stop offset=".45" stop-color="hsl({hue+8} {sat+6}% 42%)"/><stop offset=".62" stop-color="hsl({hue+30} {sat+18}% 68%)"/><stop offset="1" stop-color="hsl({hue+40} {sat+20}% 78%)"/></linearGradient>'
      f'<radialGradient id="sun{k}" cx="{gx/w:.2f}" cy="{hy/h:.2f}" r=".55"><stop offset="0" stop-color="hsla(38,90%,86%,.95)"/><stop offset=".18" stop-color="hsla(30,85%,72%,.5)"/><stop offset="1" stop-color="hsla(30,80%,60%,0)"/></radialGradient>'
      f'<linearGradient id="g{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl({hue+20} {sat-6}% 34%)"/><stop offset=".22" stop-color="hsl({hue} {sat-8}% 19%)"/><stop offset="1" stop-color="hsl({hue} {sat-10}% 7%)"/></linearGradient>'
      f'<linearGradient id="r{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl({hue} 14% 36%)" stop-opacity=".9"/><stop offset=".3" stop-color="hsl({hue} 14% 24%)" stop-opacity=".9"/><stop offset="1" stop-color="hsl({hue} 14% 10%)"/></linearGradient>'
      f'<linearGradient id="hz{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsla({hue+30},60%,80%,0)"/><stop offset=".5" stop-color="hsla({hue+30},60%,80%,.22)"/><stop offset="1" stop-color="hsla({hue+30},60%,80%,0)"/></linearGradient>'
      f'<filter id="gr{k}" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".8" numOctaves="2" seed="{seed}" stitchTiles="stitch"/><feColorMatrix values="0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 .9 0 0 0 -.33"/></filter>'
      f'<filter id="bl{k}" x="-5%" y="-5%" width="110%" height="110%"><feGaussianBlur stdDeviation="{3 if not wall else 5}"/></filter></defs>'
      f'<rect width="{w}" height="{h}" fill="url(#s{k})"/><rect width="{w}" height="{h}" fill="url(#sun{k})"/>'
      f'{layers}<rect x="0" y="{hy-h*.12:.0f}" width="{w}" height="{h*.24:.0f}" fill="url(#hz{k})"/>{ground}{rd}{bk}'
      f'<rect width="{w}" height="{h}" filter="url(#gr{k})" opacity=".35" style="mix-blend-mode:overlay"/></svg>')

PAL=[210,222,198,236,206,244,214,196,230,218,240,202,226,30,212,234,200,248,220,208]
def art(h, v=0, dim=False):
    s=26 if h!=30 else 22
    bx=[22,60,38][v%3]; by=[14,22,10][v%3]
    return (f"radial-gradient(70% 55% at {bx}% {by}%, hsla({h},40%,86%,.34) 0%, transparent 62%),"
            f"radial-gradient(90% 60% at 80% 100%, hsla({h+20},30%,56%,.30) 0%, transparent 70%),"
            f"linear-gradient(180deg, hsl({h} {s}% 58%) 0%, hsl({h} {s-2}% 36%) 52%, hsl({h} {s-6}% 15%) 100%)")

def tile(i,x,y,w,pos,n,sub,h,status,sel,cap):
    ph=int(w*0.58)
    off = status=='off'
    flt='filter:grayscale(1) brightness(.78);' if off else ''
    badge=''
    if status=='update':
        badge='<span class="pulse" style="position:absolute;top:10px;right:10px;padding:4px 10px;border-radius:13px;background:rgba(255,255,255,.94);color:#14181f;font-size:12px;font-weight:700">Есть обновление</span>'
    line2=sub
    if status=='update': line2=f'<span style="color:#fff;font-weight:600">{sub}</span>'
    if status=='lost': line2='<span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:#ffb340;margin-right:7px"></span>Страница пропала'
    if status=='maybe': line2='<span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:#ffd36b;margin-right:7px"></span>Возможно, обновлён'
    if status=='off': line2='Выключен'
    capt = f'<div style="padding:10px 4px 0"><div style="font-size:15px;font-weight:620;letter-spacing:-0.005em;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#fff">{n}</div><div style="margin-top:1px;font-size:12.5px;color:rgba(255,255,255,.66);white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{line2}</div></div>' if cap else ''
    return f'''<article class="card" style="position:absolute;left:{x}px;top:{y}px;width:{w}px;animation-delay:{60+pos*45}ms">
<div class="ph{' sel' if sel else ''}" style="height:{ph}px;border-radius:20px;{flt}box-shadow:0 4px 18px rgba(6,10,22,.35), inset 0 0 0 1px rgba(255,255,255,.14)">{scene(480,278,i*13+5,h,road=(i%2==0))}{badge}</div>{capt}</article>
'''

MODS=[('Audi A3 8V','1.0 → 1.5','update'),('BMW E36 Touring','2.3 · 96 МБ',None),('Kansai Drift Track','Выключен','off'),
('Pickup Bruiser 4x4','1.4b → 1.5','update'),('Ford Mustang GT 1969','1.2 · 120 МБ','lost'),('Realistic Engine Sounds','3.1 · 54 МБ',None),
('Coastal Highway','1.1 · 640 МБ',None),('Rally Stage Pack','0.8 · 410 МБ','maybe'),('Volga 24 Classic','1.0 · 64 МБ',None),
('Honda Civic EK9','1.6 · 88 МБ',None),('Alpine Pass','2.0 · 512 МБ',None),('Lancia Delta Integrale','0.9 · 101 МБ',None),
('Subaru Impreza 22B','1.3 · 97 МБ',None),('Nordschleife Touristenfahrten','4.2 · 1,2 ГБ',None),('Toyota AE86 Trueno','2.1 · 74 МБ',None),
('Mazda RX-7 FD','1.0 · 83 МБ',None),('Gravel Canyon','0.5 · 288 МБ',None),('Skyline R34 GT-R','1.8 · 110 МБ',None),
('Dirt Oval Pack','1.0 · 156 МБ',None),('Mini Cooper S 1275','1.1 · 52 МБ',None)]

def flies():
    r=random.Random(7); out=''
    for k in range(18):
        s=r.choice([3,4,5,6,8]); x=r.randint(20,1400); y=r.randint(40,920)
        dx=r.randint(-90,90); dy=r.randint(-120,60); t=r.randint(14,30); w=round(r.uniform(2.5,6.5),1); dl=-r.randint(0,20)
        out+=f'<span class="fly" style="left:{x}px;top:{y}px;width:{s}px;height:{s}px;--dx:{dx}px;--dy:{dy}px;--t:{t}s;--w:{w}s;animation-delay:{dl}s,{dl}s;box-shadow:0 0 {s*3}px {s}px rgba(255,236,190,.28)"></span>'
    return out+'\n'

def main():
    filters = lens('lgRail','rail',84,212,scale=34)+lens('lgGear','gear',56,56,scale=26)+lens('lgBar','bar',964,52)+lens('lgInsp','insp',328,928)
    body=''
    cw=308; rowh=int(cw*0.58)+62
    order=[1,2,3, 0,4,5, 6,7,8, 9,10,11, 12,13,14, 15,16,17]
    for pos,i in enumerate(order):
        n,s,st=MODS[i]; col=pos%3; row=pos//3
        body+=tile(i,116+col*(cw+20),-96+row*(rowh+20),cw,pos,n,s,PAL[i],st,i==0,True)
    body+=flies()
    def navitem(k,label,on,badge=None):
        bg='rgba(255,255,255,0.20);box-shadow:0 0 24px rgba(170,205,255,.38)' if on else 'transparent'
        col='#fff' if on else 'rgba(255,255,255,0.74)'
        bd=f'<span class="pulse" style="position:absolute;top:3px;right:6px;min-width:18px;height:18px;padding:0 5px;box-sizing:border-box;border-radius:9px;background:#fff;color:#111;font-size:11px;font-weight:700;display:flex;align-items:center;justify-content:center">{badge}</span>' if badge else ''
        return f'<a class="nv" href="#" style="position:relative;width:70px;padding:9px 0 7px;border-radius:20px;background:{bg};color:{col};text-decoration:none;display:flex;flex-direction:column;align-items:center;gap:4px;font-size:11px;font-weight:{600 if on else 500}">{B.icon(B.ICON[k])}<span>{label}</span>{bd}</a>'
    body+=f'''<nav class="glass lft" aria-label="Разделы" style="left:16px;top:16px;width:84px;height:212px;border-radius:36px;-webkit-backdrop-filter:blur(7px) url(#lgRail) saturate(1.4);backdrop-filter:blur(7px) url(#lgRail) saturate(1.4)">
<div class="rim" style="background-image:url({A('rail_rim.png')})"></div>
<div style="position:relative;display:flex;flex-direction:column;align-items:center;gap:6px;padding:16px 0">{navitem('grid','Гараж',True)}{navitem('upd','Обновления',False,'3')}{navitem('search','Поиск',False)}</div></nav>
<a class="glass lft nv tip" data-tip="Настройки" href="#" aria-label="Настройки" style="left:30px;top:888px;width:56px;height:56px;border-radius:28px;display:flex;align-items:center;justify-content:center;color:rgba(255,255,255,.86);text-decoration:none;-webkit-backdrop-filter:blur(7px) url(#lgGear) saturate(1.4);backdrop-filter:blur(7px) url(#lgGear) saturate(1.4)">
<div class="rim" style="background-image:url({A('gear_rim.png')})"></div><span style="position:relative">{B.icon(B.ICON['gear'],24)}</span></a>
'''
    segs=''.join(f'<label class="seg"><input type="radio" name="f" {"checked" if i==0 else ""}><span>{t}{(" <b style=\"opacity:.7;font-weight:600\">"+c+"</b>") if c else ""}</span></label>' for i,(t,c) in enumerate([('Все',None),('Включённые',None),('Нужно обновить','3'),('Не проверяются','12')]))
    body+=f'''<header class="glass top" style="left:116px;top:16px;width:964px;height:52px;border-radius:26px;-webkit-backdrop-filter:blur(7px) url(#lgBar) saturate(1.4);backdrop-filter:blur(7px) url(#lgBar) saturate(1.4)">
<div class="rim" style="background-image:url({A('bar_rim.png')})"></div>
<div style="position:relative;display:flex;align-items:center;gap:14px;height:100%;padding:0 12px 0 22px">
<span style="font-size:18px;font-weight:650;letter-spacing:-0.01em">Гараж</span><span style="color:rgba(255,255,255,0.64);font-size:13px">128 модов</span><span style="flex:1"></span>
<div role="radiogroup" aria-label="Фильтр" style="display:flex;gap:2px;padding:3px;border-radius:19px;background:rgba(255,255,255,0.10)">{segs}</div>
<label style="display:flex;align-items:center;gap:8px;width:200px;height:34px;padding:0 12px;box-sizing:border-box;border-radius:17px;background:rgba(255,255,255,0.12);color:rgba(255,255,255,0.72)">{B.icon(B.ICON['search'],15)}<input type="text" placeholder="Найти в гараже" aria-label="Найти среди установленных модов" style="flex:1;min-width:0;border:0;outline:0;background:transparent;color:#fff;font:inherit;font-size:13px"></label>
</div></header>
'''
    rows=''.join(f'<div style="display:flex;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.09)" if i else ""}"><span style="flex:0 0 88px;color:rgba(255,255,255,0.64)">{k}</span><span style="flex:1;min-width:0;font-weight:500">{v}</span></div>' for i,(k,v) in enumerate([('Размер','142 МБ, на сайте 151 МБ'),('Установлен','12 августа'),('Следим на','Modland, ModsGaming'),('Проверен','сегодня, 14:02')]))
    inside=''.join(f'<div style="display:flex;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.09)" if i else ""}"><span style="flex:0 0 88px;color:rgba(255,255,255,0.64)">{k}</span><span style="flex:1;min-width:0;font-weight:500">{v}</span></div>' for i,(k,v) in enumerate([('Машина','Audi A3 8V'),('Скрипты','Нет, безопасно')]))
    copies=''.join(f'<div style="display:flex;align-items:center;gap:12px;padding:10px 14px;{"border-top:1px solid rgba(255,255,255,0.09)" if i else ""}"><div style="flex:1"><div style="font-weight:500">{w}</div><div style="font-size:12.5px;color:rgba(255,255,255,0.64)">{nn}</div></div><button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;text-decoration:underline;text-underline-offset:3px;cursor:pointer">Вернуть</button></div>' for i,(w,nn) in enumerate([('12 августа, 21:40','Версия 1.0, 142 МБ'),('3 июня, 09:12','Версия 0.9, 138 МБ')]))
    body+=f'''<aside class="glass insp" aria-label="Выбранный мод" style="left:1096px;top:16px;width:328px;height:928px;border-radius:32px;-webkit-backdrop-filter:blur(14px) url(#lgInsp) saturate(1.4);backdrop-filter:blur(14px) url(#lgInsp) saturate(1.4)">
<div class="rim" style="background-image:url({A('insp_rim.png')})"></div>
<div style="position:relative;display:flex;flex-direction:column;gap:18px;padding:22px 22px 24px;height:100%;box-sizing:border-box">
<div style="position:relative;overflow:hidden;height:150px;border-radius:20px;box-shadow:0 10px 30px rgba(6,10,22,0.4), 0 0 40px rgba(150,195,255,.22), inset 0 0 0 1px rgba(255,255,255,0.2)">{scene(480,278,5,210,road=True)}</div>
<div><div style="font-size:24px;font-weight:700;letter-spacing:-0.02em;line-height:1.15">Audi A3 8V</div><div style="margin-top:3px;color:rgba(255,255,255,0.7)">kuro_garage</div></div>
<div style="display:flex;flex-direction:column;gap:8px">
<button type="button" class="cta btn" style="height:44px;border:0;border-radius:22px;background:#fff;color:#111;font-size:15px;font-weight:650;cursor:pointer;box-shadow:0 0 30px rgba(170,205,255,.42), 0 6px 18px rgba(6,10,22,0.3)">Обновить до 1.5</button>
<div style="text-align:center;font-size:12.5px;color:rgba(255,255,255,0.7)">Сейчас стоит 1.0. Новая версия есть на Modland и ModsGaming.</div></div>
<div style="display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.09);overflow:hidden">{rows}</div>
<div><div style="font-size:13px;font-weight:600;color:rgba(255,255,255,0.66);padding:0 4px 8px">Внутри архива</div>
<div style="display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.09);overflow:hidden">{inside}</div></div>
<div><div style="font-size:13px;font-weight:600;color:rgba(255,255,255,0.66);padding:0 4px 8px">Прошлые версии</div>
<div style="display:flex;flex-direction:column;border-radius:16px;background:rgba(255,255,255,0.09);overflow:hidden">{copies}</div></div>
<div style="margin-top:auto;display:flex;flex-wrap:wrap;gap:10px 22px;padding:0 4px">
<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Открыть страницу</button>
<button type="button" style="padding:0;border:0;background:transparent;color:#fff;font-size:13.5px;font-weight:600;cursor:pointer">Выключить</button>
<button type="button" style="padding:0;border:0;background:transparent;color:#ffb0a8;font-size:13.5px;font-weight:600;cursor:pointer">Удалить</button><span style="font-size:12.5px;color:rgba(255,255,255,.6);align-self:center">Можно вернуть позже</span>
</div></div></aside>
'''
    open('out/Main.dc.html','w').write(head('Гараж',filters)+body+TAILH)
main()
