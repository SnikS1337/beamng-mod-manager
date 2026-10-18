using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Media.Imaging;

namespace BeamModManager.App.Controls;

/// <summary>
/// Зерно фона прибавкой к цвету (Shaders/Dither.fx, 2026-10-18): ±2,5 единицы яркости на любом фоне, точка шума на точку
/// экрана. Прячет кольца-ступеньки вокруг пятен света лучше слоя зерна поверх (<see cref="Grain"/>), у которого на тёмном
/// тёмные точки почти ничего не меняли. Размер в точках экрана задаёт окно (<see cref="SetPixelSize"/>).
/// </summary>
public sealed class DitherEffect : ShaderEffect
{
    private const string ShaderPath = "pack://application:,,,/Shaders/Dither.ps";
    private const int Tile = 128;
    private static PixelShader? _shader;
    private static bool _loaded;

    /// <summary>Размах шума: от −2,5 до +2,5 единицы из 255.</summary>
    public const double Levels = 5;

    public static readonly DependencyProperty InputProperty = RegisterPixelShaderSamplerProperty("Input", typeof(DitherEffect), 0);

    private static readonly DependencyProperty NoiseProperty =
        RegisterPixelShaderSamplerProperty("Noise", typeof(DitherEffect), 1, SamplingMode.NearestNeighbor);

    private static readonly DependencyProperty TilesProperty = DependencyProperty.Register(
        "Tiles", typeof(Point), typeof(DitherEffect), new UIPropertyMetadata(new Point(8, 8), PixelShaderConstantCallback(0)));

    private static readonly DependencyProperty AmountProperty = DependencyProperty.Register(
        "Amount", typeof(double), typeof(DitherEffect), new UIPropertyMetadata(Levels / 255, PixelShaderConstantCallback(1)));

    public DitherEffect()
    {
        if (Shader() is PixelShader shader) PixelShader = shader;
        PaddingLeft = PaddingTop = PaddingRight = PaddingBottom = 0;
        SetValue(NoiseProperty, NoiseBrush);
        UpdateShaderValue(InputProperty);
        UpdateShaderValue(NoiseProperty);
        UpdateShaderValue(TilesProperty);
        UpdateShaderValue(AmountProperty);
    }

    /// <summary>Размер слоя с эффектом в точках экрана: одна точка шума — одна точка экрана, без растяжения.</summary>
    public void SetPixelSize(double width, double height) =>
        SetValue(TilesProperty, new Point(Math.Max(1, width) / Tile, Math.Max(1, height) / Tile));

    /// <summary>Шум со «треугольным» распределением (сумма двух равномерных): ровнее прячет ступеньки, чем равномерный.</summary>
    private static readonly ImageBrush NoiseBrush = MakeNoise();

    private static ImageBrush MakeNoise()
    {
        var px = new byte[Tile * Tile];
        var rnd = new Random(20261018); // одно и то же зерно при каждом запуске
        for (var i = 0; i < px.Length; i++) px[i] = (byte)((rnd.Next(256) + rnd.Next(256)) / 2);
        var bmp = BitmapSource.Create(Tile, Tile, 96, 96, PixelFormats.Gray8, null, px, Tile);
        bmp.Freeze();
        var brush = new ImageBrush(bmp) { Stretch = Stretch.Fill };
        brush.Freeze();
        return brush;
    }

    /// <summary>Шейдер загрузился. Звать из потока окна.</summary>
    public static bool IsAvailable => Shader() is not null;

    private static PixelShader? Shader()
    {
        if (_loaded) return _shader;
        _loaded = true;
        try
        {
            var info = Application.GetResourceStream(new Uri(ShaderPath, UriKind.Absolute));
            if (info?.Stream is not Stream stream) return null;
            using (stream)
            {
                var shader = new PixelShader();
                shader.SetStreamSource(stream);
                if (shader.CanFreeze) shader.Freeze();
                _shader = shader;
            }
        }
        catch (Exception e) when (e is IOException or UriFormatException or InvalidOperationException or ArgumentException)
        {
            _shader = null;
        }
        return _shader;
    }
}
