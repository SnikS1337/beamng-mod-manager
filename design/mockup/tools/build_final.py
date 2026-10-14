"""Доска «Гараж: финальный вид» — главный экран со всеми решениями на 2026-10-14:
лёд при первом запуске, окно Windows с обычным заголовком, самостоятельно движется только фон
(пульс только у непросмотренного бейджа «Обновления»), выключенный мод покрыт инеем.
Запуск из папки tools после gen.py, gen_ice.py и build_ui.py:  python3 build_final.py"""
import re
import build_ui as U
import build_ice as I

TITLE_H = 32

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
'''


def build():
    html = I.build()
    # 1. Статусы карточек не пульсируют; бейдж «Обновления» пульсирует (непросмотренное) — он уже с классом pulse
    html = html.replace('<span class="chip dk up pulse">', '<span class="chip dk up">')
    # 2. «Не проверяется» словами у одной карточки (фильтр «Не проверяются 12»)
    html = html.replace('>3.1 · 54 МБ<', '>3.1 · не проверяется<', 1)
    # 3. Обычное окно Windows: заголовок 32 px сверху, содержимое 1440 × 960 под ним
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
