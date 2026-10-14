"""Ледяная текстура для стеклянных панелей: иней по краям, прожилки кристаллов, зернистость, искры льда.
Запуск:  python3 gen_ice.py   -> ice_<имя>.png (RGBA). Нужны numpy, scipy, Pillow."""
import numpy as np
from PIL import Image
from scipy.ndimage import gaussian_filter
from gen import sdf_rrect

SPECS = [('rail', 84, 284, 36, 34), ('gear', 56, 56, 28, 22), ('bar', 964, 52, 26, 24), ('insp', 328, 928, 32, 52), ('card', 308, 236, 26, 60),
         ('chk', 328, 540, 32, 50), ('set', 1308, 640, 32, 50), ('set_d', 1308, 470, 32, 50), ('set_s', 1308, 560, 32, 50), ('dl', 1100, 700, 36, 50),
         ('set_a', 1308, 760, 32, 50), ('pv', 420, 56, 28, 24)]


def fractal(rng, W, H, base, octaves=5, gain=0.55):
    """Шум 0..1: сумма сглаженных случайных полей разной крупности."""
    out = np.zeros((H, W)); amp = 1.0; tot = 0.0
    for o in range(octaves):
        s = max(2, int(base / (2 ** o)))
        f = rng.random((H // s + 3, W // s + 3))
        im = Image.fromarray((f * 255).astype(np.uint8)).resize((W + 2 * s, H + 2 * s), Image.BICUBIC)
        a = np.asarray(im, dtype=float)[:H, :W] / 255.0
        out += a * amp; tot += amp; amp *= gain
    out /= tot
    return (out - out.min()) / (out.max() - out.min() + 1e-9)


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def make(name, W, H, R, F, seed):
    rng = np.random.default_rng(seed)
    d = -sdf_rrect(W, H, R)                      # расстояние внутрь от края
    inside = (d >= 0).astype(float)
    gy, gx = np.gradient(d)
    gn = np.hypot(gx, gy) + 1e-6
    # ближайшая точка края: от пикселя против направления внутрь; иней растёт «пальцами» вглубь от края
    y, x = np.mgrid[0:H, 0:W]
    bx = np.clip(x - d * gx / gn, 0, W - 1); by = np.clip(y - d * gy / gn, 0, H - 1)
    from scipy.ndimage import map_coordinates
    big = fractal(rng, W, H, 7, 3, 0.6)          # мелкие пальцы
    mid = fractal(rng, W, H, 22, 3, 0.55)        # длина пальцев
    nb = map_coordinates(big, [by, bx], order=1)
    nm = map_coordinates(mid, [by, bx], order=1)
    reach = F * (0.25 + 1.25 * (0.65 * nm + 0.35 * nb))
    frost = np.clip(1 - d / reach, 0, 1) ** 1.6
    frost *= 0.7 + 0.3 * smooth(0.3, 0.7, fractal(rng, W, H, 5, 2))   # зернистость внутри инея
    # редкие тонкие прожилки (едва заметные)
    ridge = 1 - np.abs(fractal(rng, W, H, 30, 4, 0.5) - 0.5) * 2
    vein = smooth(0.95, 0.992, ridge) * np.clip(1 - d / (F * 2.2), 0, 1)
    # лёгкая изморозь на всей поверхности и зерно
    grain = fractal(rng, W, H, 2, 1)
    mist = 0.035 * smooth(0.2, 0.8, fractal(rng, W, H, 90, 3)) + 0.03 * grain
    # искорки кристаллов у края
    sparkle = smooth(0.84, 0.95, fractal(rng, W, H, 2, 1)) * np.clip(1 - d / (F * 1.3), 0, 1)
    a = np.clip(frost * 0.62 + vein * 0.16 + mist + sparkle * 0.40, 0, 0.9) * inside
    a = gaussian_filter(a, 0.5) * inside
    cold = np.clip(frost * 0.9, 0, 1)
    r = 246 - 26 * cold; g = 251 - 12 * cold; b = np.full_like(r, 255)
    rgba = np.dstack([r, g, b, a * 255]).clip(0, 255).astype(np.uint8)
    Image.fromarray(rgba, 'RGBA').save(f'ice_{name}.png')


if __name__ == '__main__':
    for i, (n, W, H, R, F) in enumerate(SPECS):
        make(n, W, H, R, F, seed=11 + i)
    print('ok')
