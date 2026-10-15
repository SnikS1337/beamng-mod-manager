using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Media.Imaging;

namespace BeamModManager.App.Services;

/// <summary>
/// Фото-фон размывается один раз при запуске (16 px, DESIGN.md 3.1), а дальше двигается уже готовая картинка:
/// размытие на каждом кадре стоило бы видеокарте заметно больше.
/// </summary>
public static class Backdrop
{
    public static BitmapSource? LoadBlurred(string? path, double blur = 16)
    {
        if (string.IsNullOrEmpty(path) || !File.Exists(path)) return null;
        try
        {
            var bi = new BitmapImage();
            bi.BeginInit();
            bi.CacheOption = BitmapCacheOption.OnLoad;
            bi.UriSource = new Uri(Path.GetFullPath(path), UriKind.Absolute);
            bi.DecodePixelWidth = 1280; // размытому фону больше не нужно
            bi.EndInit();
            bi.Freeze();

            var pad = (int)Math.Ceiling(blur * 3);
            var w = bi.PixelWidth;
            var h = bi.PixelHeight;
            var img = new Image { Source = bi, Stretch = Stretch.Fill, Width = w + pad * 2, Height = h + pad * 2, Effect = new BlurEffect { Radius = blur, KernelType = KernelType.Gaussian } };
            img.Measure(new Size(img.Width, img.Height));
            img.Arrange(new Rect(-pad, -pad, img.Width, img.Height));
            var rt = new RenderTargetBitmap(w, h, 96, 96, PixelFormats.Pbgra32);
            rt.Render(img);
            rt.Freeze();
            return rt;
        }
        catch (Exception e) when (e is IOException or NotSupportedException or UnauthorizedAccessException or ArgumentException or FileFormatException or UriFormatException)
        {
            return null;
        }
    }
}
