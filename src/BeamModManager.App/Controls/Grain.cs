using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace BeamModManager.App.Controls;

/// <summary>
/// Зерно поверх фона (2026-10-18): плавные переходы фона и пятен света на 8 битах цвета ложатся ступеньками — кольцами
/// вокруг пятен, особенно на светлом розовом и на экране с масштабом 200 % (видео с виртуалки). Неподвижное зерно в
/// пару единиц яркости размывает границы ступенек, глазу его не видно. Плитка 128 × 128, точки светлее и темнее поровну —
/// средний цвет фона не меняется.
/// </summary>
public static class Grain
{
    private const int Size = 128;

    /// <summary>Сила зерна: на сколько единиц яркости из 255 точка может уйти вверх или вниз.</summary>
    public const int Amplitude = 6;

    public static ImageBrush Brush()
    {
        var px = new byte[Size * Size * 4];
        var rnd = new Random(20261018); // одно и то же зерно при каждом запуске
        for (var i = 0; i < Size * Size; i++)
        {
            var a = (byte)rnd.Next(0, Amplitude + 1);
            var white = rnd.Next(2) == 0;
            var c = white ? a : (byte)0; // цвета с умноженной прозрачностью: белая точка — (a, a, a, a), чёрная — (0, 0, 0, a)
            px[i * 4] = c;
            px[i * 4 + 1] = c;
            px[i * 4 + 2] = c;
            px[i * 4 + 3] = a;
        }
        var bmp = BitmapSource.Create(Size, Size, 96, 96, PixelFormats.Pbgra32, null, px, Size * 4);
        bmp.Freeze();
        var brush = new ImageBrush(bmp)
        {
            TileMode = TileMode.Tile,
            Stretch = Stretch.None,
            ViewportUnits = BrushMappingMode.Absolute,
            Viewport = new Rect(0, 0, Size, Size),
        };
        RenderOptions.SetBitmapScalingMode(brush, BitmapScalingMode.NearestNeighbor);
        RenderOptions.SetCachingHint(brush, CachingHint.Cache);
        brush.Freeze();
        return brush;
    }
}
