"""Доска «Гараж: финальный вид» — главный экран со всеми решениями на 2026-10-14:
лёд при первом запуске, окно Windows с обычным заголовком, кнопка «Играть», самостоятельно движется только фон
(пульс только у непросмотренного бейджа «Обновления»), выключенный мод покрыт инеем.
Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_final.py"""
import re
import build_ui as U
import build_ice as I

TITLE_H = 32

PLAY = ('<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" '
        'stroke-linejoin="round" aria-hidden="true"><path d="M7 4.5v15l12.5-7.5z"/></svg>')

FINAL_CSS = '''
/* по DESIGN_MOTION.md: само движется только фон; выбранная карточка и главная кнопка не «дышат» */
.cd.sel{animation:none !important;box-shadow:0 22px 40px rgba(4,8,20,.5),0 0 40px rgba(140,195,255,.45) !important}
.cta::after{display:none !important}
.chip.up{animation:none !important}
.wintb{position:absolute;left:0;top:0;width:1440px;height:32px;z-index:50;display:flex;align-items:center;font-family:'Segoe UI Variable Text','Segoe UI',system-ui,sans-serif;
 background:#141b27;color:rgba(255,255,255,.86);font-size:12px;box-shadow:inset 0 -1px 0 rgba(255,255,255,.06)}
.wintb .ttl{padding-left:16px;flex:1}
.wintb .wb{width:46px;height:32px;display:flex;align-items:center;justify-content:center;color:rgba(255,255,255,.86)}
.wintb .wb:hover{background:rgba(255,255,255,.08)}.wintb .wb.x:hover{background:#c42b1c;color:#fff}
.play{position:absolute;left:16px;width:84px;height:84px;border-radius:32px;z-index:6;box-sizing:border-box;padding-top:15px;
 display:flex;flex-direction:column;align-items:center;gap:5px;color:#fff;font-size:11px;font-weight:600;cursor:pointer;
 background:rgba(14,24,44,.44);-webkit-backdrop-filter:blur(4px) saturate(1.4) brightness(.78);backdrop-filter:blur(4px) saturate(1.4) brightness(.78);
 box-shadow:0 22px 56px rgba(8,12,24,.38),inset 0 0 0 1px rgba(205,230,255,.22),inset 0 1.5px 0 rgba(235,246,255,.45),inset 0 0 22px rgba(170,205,255,.08);
 text-shadow:0 1px 6px rgba(6,12,24,.5);transition:transform var(--fast) var(--ease)}
.play:hover{transform:translateY(-2px)}.play:active{transform:scale(.97)}
.play:focus-visible{outline:3px solid #fff;outline-offset:2px}
.play svg{filter:drop-shadow(0 0 10px rgba(170,205,255,.55))}
'''


def build():
    html = I.build()
    # 1. Статусы карточек не пульсируют; бейдж «Обновления» пульсирует (непросмотренное) — он уже с классом pulse
    html = html.replace('<span class="chip dk up pulse">', '<span class="chip dk up">')
    # 2. «Не проверяется» словами у одной карточки (фильтр «Не проверяются 12»)
    html = html.replace('>3.1 · 54 МБ<', '>3.1 · не проверяется<', 1)
    # 3. Кнопка «Играть» внизу левой колонки (DESIGN_FLOWS.md 3.6)
    play = (f'<button type="button" class="play" aria-label="Играть: запустить BeamNG.drive через Steam" style="top:{960 - 16 - 84}px">'
            f'{PLAY}<span>Играть</span></button>\n')
    html = html.replace('<header class="glass', play + '<header class="glass', 1)
    # 4. Обычное окно Windows: заголовок 32 px сверху, содержимое 1440 × 960 под ним
    m = re.search(r'<div style="position:relative;width:1440px;height:960px;overflow:hidden;', html)
    assert m, 'обёртка страницы не найдена'
    start = m.start()
    tb = ('<div class="wintb" role="banner"><span class="ttl">ModManager BeamNG</span>'
          '<span class="wb" aria-label="Свернуть"><svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor"/></svg></span>'
          '<span class="wb" aria-label="Развернуть"><svg width="10" height="10" viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9" fill="none" stroke="currentColor"/></svg></span>'
          '<span class="wb x" aria-label="Закрыть"><svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor"/></svg></span></div>')
    html = (html[:start] + f'<div style="position:relative;width:1440px;height:{960 + TITLE_H}px;overflow:hidden;background:#141b27">{tb}'
            f'<div style="position:absolute;left:0;top:{TITLE_H}px;width:1440px;height:960px">' + html[start:])
    end = html.rindex('</x-dc>')
    html = html[:end] + '</div></div>\n' + html[end:]
    html = html.replace('"height":960', f'"height":{960 + TITLE_H}', 1)
    html = re.sub(r'<title>.*?</title>', '<title>Гараж: финальный вид</title>', html, count=1)
    return html.replace('</style>', FINAL_CSS + '</style>', 1)


if __name__ == '__main__':
    open(f'{U.OUT}/GarageFinal.dc.html', 'w').write(build())
    print('ok')
