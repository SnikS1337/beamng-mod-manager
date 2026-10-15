using System.IO;
using System.Windows.Media.Imaging;

namespace BeamModManager.App.Services;

/// <summary>Уменьшает картинку мода до 616 × 356 «с запасом» (DESIGN.md раздел 9) и пишет JPEG. Работает в фоновом потоке.</summary>
public static class Shrinker
{
    public const int W = 616, H = 356;

    public static byte[]? ToJpeg(byte[] raw)
    {
        try
        {
            // сначала узнаём размер, чтобы уменьшать по нужной стороне и не терять «cover»
            var probe = BitmapFrame.Create(new MemoryStream(raw), BitmapCreateOptions.DelayCreation, BitmapCacheOption.None);
            double pw = probe.PixelWidth, ph = probe.PixelHeight;
            if (pw <= 0 || ph <= 0) return null;

            var bi = new BitmapImage();
            bi.BeginInit();
            bi.CacheOption = BitmapCacheOption.OnLoad;
            bi.CreateOptions = BitmapCreateOptions.IgnoreColorProfile;
            if (pw / ph > (double)W / H) { if (ph > H) bi.DecodePixelHeight = H; }
            else if (pw > W) bi.DecodePixelWidth = W;
            bi.StreamSource = new MemoryStream(raw);
            bi.EndInit();
            bi.Freeze();

            var enc = new JpegBitmapEncoder { QualityLevel = 88 };
            enc.Frames.Add(BitmapFrame.Create(bi));
            using var ms = new MemoryStream();
            enc.Save(ms);
            return ms.ToArray();
        }
        catch (Exception e) when (e is NotSupportedException or FileFormatException or ArgumentException or InvalidOperationException or IOException
                                     or System.Runtime.InteropServices.COMException or OverflowException)
        {
            return null;
        }
    }
}
