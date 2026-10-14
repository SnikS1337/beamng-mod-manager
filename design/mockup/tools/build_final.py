"""Доска «Гараж» (GarageFinal) — главный экран со всеми решениями на 2026-10-14:
лёд при первом запуске; своя верхушка окна: стандартной полосы Windows нет, фон доходит до самого верха,
кнопки «свернуть / развернуть / закрыть» — стеклянная капсула в правом верхнем углу; правая панель опущена под неё;
самостоятельно движется только фон (пульс только у непросмотренного бейджа «Обновления»);
выбранная карточка без отдельной обводки: приподнята и подсвечена; сетка заканчивается у правой панели.
Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_final.py"""
import re
import build_ui as U
import build_ice as I

CAP_W, CAP_H = 132, 36                 # капсула кнопок окна
INSP_TOP = 16 + CAP_H + 12             # правая панель начинается под капсулой
INSP_H = 960 - INSP_TOP - 16

FINAL_CSS = '''
/* по DESIGN_MOTION.md: само движется только фон; выбранная карточка и главная кнопка не «дышат» */
.cta::after{display:none !important}
.chip.up{animation:none !important}
/* выбранная карточка: без отдельного кольца; приподнята, светлая кромка ярче, мягкое холодное свечение */
.cd.sel{animation:none !important;transform:translateY(-4px) !important;
 box-shadow:0 24px 44px rgba(4,8,20,.55),0 0 46px rgba(150,200,255,.30) !important}
.cd.sel::after{display:none !important}
.cd.sel::before{box-shadow:inset 0 0 0 1px rgba(255,255,255,.42),inset 0 2px 0 rgba(255,255,255,.75),inset 1.5px 0 0 rgba(255,255,255,.26),inset 0 -1px 0 rgba(255,255,255,.16) !important}
/* кнопки окна: стеклянная капсула в правом верхнем углу; при наведении под кнопкой появляется капля жидкого стекла
   (та же, что в левой панели и фильтрах) и перетекает между кнопками; на «Закрыть» капля красная */
.wcap{position:absolute;right:16px;top:16px;height:36px;display:flex;border-radius:18px;z-index:6;
 background:rgba(14,24,44,.44);-webkit-backdrop-filter:blur(4px) saturate(1.4) brightness(.78);backdrop-filter:blur(4px) saturate(1.4) brightness(.78);
 box-shadow:0 14px 34px rgba(8,12,24,.38),inset 0 0 0 1px rgba(205,230,255,.22),inset 0 1.5px 0 rgba(235,246,255,.45)}
.wcap button{position:relative;z-index:4;width:44px;height:36px;border:0;padding:0;background:transparent;color:rgba(255,255,255,.9);display:flex;align-items:center;justify-content:center;cursor:pointer;transition:color var(--fast)}
.wcap button:hover{color:#fff}
.wcap button:focus-visible{outline:3px solid #fff;outline-offset:-3px;border-radius:18px}
.wcap .wdrop{top:1px;height:34px;left:-1px;width:46px;border-radius:17px;opacity:0;transform:scale(.6);
 transition:left .5s cubic-bezier(.3,1.35,.45,1),opacity .2s ease,transform .35s cubic-bezier(.3,1.35,.45,1),background .25s ease}
.wcap:has(button:hover) .wdrop{opacity:1;transform:scale(1)}
.wcap:has(button:nth-of-type(1):hover) .wdrop{left:-1px}
.wcap:has(button:nth-of-type(2):hover) .wdrop{left:43px}
.wcap:has(button:nth-of-type(3):hover) .wdrop{left:87px;
 background:linear-gradient(160deg,rgba(255,130,118,.62),rgba(214,58,46,.52) 52%,rgba(255,120,108,.58));
 box-shadow:inset 0 0 0 1px rgba(255,190,180,.45),inset 2px 2px 4px -1px rgba(255,235,230,.85),inset -2px -2px 4px -1px rgba(255,200,190,.45),0 8px 18px rgba(120,20,10,.45)}
@media (prefers-reduced-motion:reduce){.wcap .wdrop{transition:opacity .15s}}
'''

CAPSULE = ('<div class="wcap" role="group" aria-label="Окно">'
           '<button type="button" aria-label="Свернуть"><svg width="11" height="11" viewBox="0 0 11 11"><path d="M1 5.5h9" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg></button>'
           '<button type="button" aria-label="Развернуть"><svg width="11" height="11" viewBox="0 0 11 11"><rect x="1" y="1" width="9" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="1.3"/></svg></button>'
           '<button type="button" class="x" aria-label="Закрыть"><svg width="11" height="11" viewBox="0 0 11 11"><path d="M1.5 1.5l8 8M9.5 1.5l-8 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg></button>'
           '<i class="drop wdrop" aria-hidden="true"></i></div>\n')


def build():
    html = I.build()
    html = html.replace('<span class="chip dk up pulse">', '<span class="chip dk up">')
    html = html.replace('>3.1 · 54 МБ<', '>3.1 · не проверяется<', 1)
    # правая панель опускается под капсулу кнопок окна
    old = 'left:1096px;top:16px;width:328px;height:928px'
    assert old in html, 'правая панель не найдена'
    html = html.replace(old, f'left:1096px;top:{INSP_TOP}px;width:328px;height:{INSP_H}px', 1)
    html, n = re.subn(r'<filter id="lgInsp".*?</filter>', lambda m: U.lens('lgInsp', 'insp', 328, INSP_H, 96), html, count=1, flags=re.S)
    assert n == 1, 'линза правой панели не найдена'
    # фото в панели чуть ниже, чтобы содержимое поместилось
    html = html.replace('<div class="pic p-dacia" style="height:150px;', '<div class="pic p-dacia" style="height:132px;', 1)
    html = html.replace('<header class="glass', CAPSULE + '<header class="glass', 1)
    # четвёртая колонка карточек под правой панелью убрана: иначе она торчит в углу рядом с кнопками окна
    html, n = re.subn(r'<article class="card"[^>]*><div class="cd[^"]* under".*?</article>\n', '', html, flags=re.S)
    assert n > 0, 'карточки под панелью не найдены'
    html = re.sub(r'<title>.*?</title>', '<title>Гараж</title>', html, count=1)
    return html.replace('</style>', FINAL_CSS + '</style>', 1)


if __name__ == '__main__':
    open(f'{U.OUT}/GarageFinal.dc.html', 'w').write(build())
    print('ok')
