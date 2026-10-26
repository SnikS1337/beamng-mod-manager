"""Значок программы «Мод-куб из балок» (выбран 2026-10-17, доска «Значок программы»).

Связь с BeamNG без их логотипа: игра собирает всё из узлов и балок, значок — ящик-мод, собранный так же.
Плоский рисунок без свечения и градиентов: тёмно-синяя плитка в цвет окна, грани куба в три тона,
белые балки, оранжевые узлы. Мелкие размеры нарисованы отдельно (меньше деталей, толще линии),
а не уменьшены: в 16 px диагонали и узлы слиплись бы в кашу.

Запуск: python3 tools/icon/make_icon.py — перерисовывает исходники design/icon/*.svg
и PNG-размеры (растрирует Chromium из Playwright, сохраняет через Pillow).
Значки приложения собираются из крупного исходника командой Tauri CLI:
    npm run tauri icon -- design/icon/app-large.svg
(она обновляет src-tauri/icons: ICO, ICNS, PNG и значки Store).
"""
import io
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT_SVG = os.path.join(ROOT, "design", "icon")

PLATE = "#1B2547"
TOP, LEFT, RIGHT = "#3E5294", "#212B4E", "#2E3D72"
BEAM = "#F3F5FA"
NODE = "#FF8A2B"


def cube(detail):
    """detail: 2 — крупно (диагонали граней, все узлы), 1 — средне (без диагоналей), 0 — мелко (толще, узлов 3)."""
    # правильный изометрический куб, центр плитки 128; ребро ~76
    t, ul, ur, c, ll, lr, b = (128, 52), (62, 90), (194, 90), (128, 128), (62, 166), (194, 166), (128, 204)
    if detail == 0:  # мелко: куб крупнее, чтобы линии не тонули
        t, ul, ur, c, ll, lr, b = (128, 40), (52, 84), (204, 84), (128, 128), (52, 172), (204, 172), (128, 216)
    P = lambda *pts: " ".join(f"{x},{y}" for x, y in pts)
    sw = {2: 11, 1: 14, 0: 22}[detail]
    s = []
    s.append(f'<polygon points="{P(t, ur, c, ul)}" fill="{TOP}"/>')
    s.append(f'<polygon points="{P(ul, c, b, ll)}" fill="{LEFT}"/>')
    s.append(f'<polygon points="{P(ur, c, b, lr)}" fill="{RIGHT}"/>')
    if detail == 2:  # распорки граней — тоньше и тише рёбер
        s.append(f'<g stroke="{BEAM}" stroke-opacity="0.45" stroke-width="6" stroke-linecap="round">'
                 f'<line x1="{t[0]}" y1="{t[1]}" x2="{c[0]}" y2="{c[1]}"/>'
                 f'<line x1="{ul[0]}" y1="{ul[1]}" x2="{b[0]}" y2="{b[1]}"/>'
                 f'<line x1="{ur[0]}" y1="{ur[1]}" x2="{b[0]}" y2="{b[1]}"/></g>')
    s.append(f'<g fill="none" stroke="{BEAM}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round">'
             f'<polygon points="{P(t, ur, lr, b, ll, ul)}"/>'
             f'<polyline points="{P(ul, c, ur)}"/><line x1="{c[0]}" y1="{c[1]}" x2="{b[0]}" y2="{b[1]}"/></g>')
    nodes = [t, ul, ur, c, ll, lr, b] if detail > 0 else [t, c, b]
    r = {2: 12, 1: 15, 0: 24}[detail]
    ring = {2: 5, 1: 6, 0: 8}[detail]
    for x, y in nodes:
        s.append(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{NODE}" stroke="{PLATE}" stroke-width="{ring}"/>')
    return "\n  ".join(s)


def svg(detail):
    rx = 56 if detail > 0 else 48
    pad = 12 if detail > 0 else 4
    size = 256 - 2 * pad
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">\n'
            f'  <rect x="{pad}" y="{pad}" width="{size}" height="{size}" rx="{rx}" fill="{PLATE}"/>\n'
            f'  {cube(detail)}\n</svg>\n')


def main():
    os.makedirs(OUT_SVG, exist_ok=True)
    files = {"app-large.svg": svg(2), "app-medium.svg": svg(1), "app-small.svg": svg(0)}
    for name, text in files.items():
        with open(os.path.join(OUT_SVG, name), "w", encoding="utf-8") as f:
            f.write(text)
    # какие размеры из какого рисунка
    plan = [(256, "app-large.svg"), (128, "app-large.svg"), (64, "app-large.svg"), (48, "app-large.svg"),
            (40, "app-medium.svg"), (32, "app-medium.svg"), (24, "app-small.svg"), (20, "app-small.svg"), (16, "app-small.svg")]
    from playwright.sync_api import sync_playwright
    from PIL import Image
    images = {}
    with sync_playwright() as p:
        b = p.chromium.launch()
        pg = b.new_page()
        for px, name in plan:
            pg.set_viewport_size({"width": px, "height": px})
            pg.set_content(f'<html><body style="margin:0;background:transparent">'
                           f'<img src="data:image/svg+xml;base64,{__import__("base64").b64encode(files[name].encode()).decode()}" '
                           f'width="{px}" height="{px}" style="display:block"></body></html>')
            png = pg.screenshot(omit_background=True, clip={"x": 0, "y": 0, "width": px, "height": px})
            images[px] = Image.open(io.BytesIO(png)).convert("RGBA")
            images[px].save(os.path.join(OUT_SVG, f"app-{px}.png"))
        b.close()
    print("ok:", OUT_SVG)
    print("Дальше: npm run tauri icon -- design/icon/app-large.svg")


if __name__ == "__main__":
    sys.exit(main())
