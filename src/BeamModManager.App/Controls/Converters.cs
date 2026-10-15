using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using BeamModManager.Ui.Garage;

namespace BeamModManager.App.Controls;

/// <summary>Картинки карточек: читаются один раз, уменьшенными, и замораживаются (их можно рисовать без пересчёта).</summary>
public static class ImageCache
{
    private static readonly Dictionary<string, BitmapSource?> ColorCache = new(StringComparer.OrdinalIgnoreCase);
    private static readonly Dictionary<string, BitmapSource?> Gray = new(StringComparer.OrdinalIgnoreCase);

    public static BitmapSource? Load(string? path, int decodeWidth = 616)
    {
        if (string.IsNullOrEmpty(path)) return null;
        var key = path + "|" + decodeWidth;
        if (ColorCache.TryGetValue(key, out var hit)) return hit;
        BitmapSource? img = null;
        try
        {
            var bi = new BitmapImage();
            bi.BeginInit();
            bi.CacheOption = BitmapCacheOption.OnLoad; // файл не держим открытым: кэш можно чистить
            bi.CreateOptions = BitmapCreateOptions.IgnoreColorProfile;
            bi.DecodePixelWidth = decodeWidth;
            bi.UriSource = new Uri(path, UriKind.Absolute);
            bi.EndInit();
            bi.Freeze();
            img = bi;
        }
        catch (Exception e) when (e is IOException or NotSupportedException or UnauthorizedAccessException or ArgumentException or FileFormatException) { }
        ColorCache[key] = img;
        return img;
    }

    /// <summary>Выключенный мод «спит»: фото в оттенках серого (DESIGN.md раздел 4).</summary>
    public static BitmapSource? LoadGray(string? path)
    {
        if (string.IsNullOrEmpty(path)) return null;
        if (Gray.TryGetValue(path, out var hit)) return hit;
        BitmapSource? g = null;
        if (Load(path) is { } src)
        {
            var f = new FormatConvertedBitmap(src, PixelFormats.Gray8, null, 0);
            f.Freeze();
            g = f;
        }
        Gray[path] = g;
        return g;
    }

    public static void Forget(string path)
    {
        foreach (var k in ColorCache.Keys.Where(k => k.StartsWith(path + "|", StringComparison.OrdinalIgnoreCase)).ToList()) ColorCache.Remove(k);
        Gray.Remove(path);
    }
}

/// <summary>[путь к картинке, включён] → кисть фото (цветная или серая); null, если картинки нет.</summary>
public sealed class PreviewBrushConverter : IMultiValueConverter
{
    public object? Convert(object[] values, Type targetType, object? parameter, CultureInfo culture)
    {
        var path = values.Length > 0 ? values[0] as string : null;
        var enabled = values.Length < 2 || values[1] is not bool b || b;
        var img = enabled ? ImageCache.Load(path) : ImageCache.LoadGray(path);
        if (img is null) return null;
        var brush = new ImageBrush(img) { Stretch = Stretch.UniformToFill, AlignmentX = AlignmentX.Center, AlignmentY = AlignmentY.Center };
        brush.Freeze();
        return brush;
    }

    public object[] ConvertBack(object value, Type[] targetTypes, object? parameter, CultureInfo culture) => throw new NotSupportedException();
}

public sealed class NullToCollapsedConverter : IValueConverter
{
    public bool Invert { get; set; }
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var has = value is not null && (value is not string s || s.Length > 0);
        return has ^ Invert ? Visibility.Visible : Visibility.Collapsed;
    }
    public object ConvertBack(object value, Type targetType, object? parameter, CultureInfo culture) => throw new NotSupportedException();
}

public sealed class BoolToVisibilityConverter : IValueConverter
{
    public bool Invert { get; set; }
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        (value is true) ^ Invert ? Visibility.Visible : Visibility.Collapsed;
    public object ConvertBack(object value, Type targetType, object? parameter, CultureInfo culture) => throw new NotSupportedException();
}

/// <summary>Значок заглушки по типу мода (машина, карта, звук, скрипт).</summary>
public sealed class PlaceholderIconConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var key = value switch
        {
            PlaceholderKind.Vehicle => "IconCar",
            PlaceholderKind.Map => "IconMap",
            PlaceholderKind.Sound => "IconSound",
            PlaceholderKind.Script => "IconScript",
            _ => "IconBox",
        };
        return Application.Current.TryFindResource(key);
    }
    public object ConvertBack(object value, Type targetType, object? parameter, CultureInfo culture) => throw new NotSupportedException();
}

/// <summary>Цвет точки на плашке: голубая — обновление, янтарная — сомнительное.</summary>
public sealed class ChipDotConverter : IValueConverter
{
    private static readonly Brush Blue = Frozen(0x8F, 0xD0, 0xFF);
    private static readonly Brush Amber = Frozen(0xFF, 0xB3, 0x40);
    private static Brush Frozen(byte r, byte g, byte b) { var x = new SolidColorBrush(System.Windows.Media.Color.FromRgb(r, g, b)); x.Freeze(); return x; }

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is ChipKind.Update ? Blue : Amber;
    public object ConvertBack(object value, Type targetType, object? parameter, CultureInfo culture) => throw new NotSupportedException();
}
