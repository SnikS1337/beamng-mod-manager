using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Media.Media3D;

namespace BeamModManager.App.Controls;

/// <summary>
/// Линза капли для подписей (Shaders/Lens.fx, 2026-10-18): у кромки видно то, что чуть снаружи капли, сжатое к краю, с
/// тонкой радужной каймой; середина как есть. Элемент с эффектом больше капли на <see cref="Margin"/> со всех сторон.
/// </summary>
public sealed class LensEffect : ShaderEffect
{
    private const string ShaderPath = "pack://application:,,,/Shaders/Lens.ps";
    private static PixelShader? _shader;
    private static bool _loaded;

    public static readonly DependencyProperty InputProperty = RegisterPixelShaderSamplerProperty("Input", typeof(LensEffect), 0);

    private static readonly DependencyProperty ShaderSizeProperty = Register("ShaderSize", new System.Windows.Size(1, 1), 0);
    private static readonly DependencyProperty CornerProperty = Register("Corner", new Point4D(0, 0.5, 0.5, 0), 1);
    private static readonly DependencyProperty InvLensProperty = Register("InvLens", 1.0 / 14, 2);
    private static readonly DependencyProperty PushProperty = Register("Push", 9.0, 3);
    private static readonly DependencyProperty UnderProperty = Register("Under", 0.1, 4);
    private static readonly DependencyProperty EdgeProperty = Register("Edge", new Point4D(0, 0, 1, 1), 8);
    private static readonly DependencyProperty GeoProperty = Register("Geo", new Point4D(1, 1, 0.5, 0.5), 9);

    private static DependencyProperty Register<T>(string name, T value, int register) =>
        DependencyProperty.Register(name, typeof(T), typeof(LensEffect), new UIPropertyMetadata(value, PixelShaderConstantCallback(register)));

    /// <summary>Поле вокруг капли: его хватает на самый дальний сдвиг (синий канал, 1,26 × <see cref="Push"/>).</summary>
    public const double Margin = 12;

    /// <summary>Насколько снаружи берётся точка у самой кромки.</summary>
    public const double PushAtRim = 9;

    /// <summary>Ширина пояса у кромки, где идёт сжатие.</summary>
    public const double Band = 14;

    private System.Windows.Size _size = new(1, 1);
    private double _radius;

    public LensEffect()
    {
        if (Shader() is PixelShader shader) PixelShader = shader;
        PaddingLeft = PaddingTop = PaddingRight = PaddingBottom = 0;
        UpdateShaderValue(InputProperty);
        UpdateShaderValue(ShaderSizeProperty);
        UpdateShaderValue(CornerProperty);
        UpdateShaderValue(InvLensProperty);
        UpdateShaderValue(PushProperty);
        UpdateShaderValue(UnderProperty);
        UpdateShaderValue(EdgeProperty);
        UpdateShaderValue(GeoProperty);
        SetValue(InvLensProperty, 1 / Band);
        SetValue(PushProperty, PushAtRim);
        Geometry();
    }

    /// <summary>Размер самой капли (без поля).</summary>
    public System.Windows.Size Size { get => _size; set { _size = value; Geometry(); } }

    /// <summary>Радиус скругления капли.</summary>
    public double Radius { get => _radius; set { _radius = value; Geometry(); } }

    /// <summary>Яркость того, что под подписями (0 — тёмное, 1 — белое): цвет каймы там, где канал прошёл мимо подписи.</summary>
    public double Under { set => SetValue(UnderProperty, Math.Clamp(value, 0, 1)); }

    private void Geometry()
    {
        var w = Math.Max(1, _size.Width);
        var h = Math.Max(1, _size.Height);
        var r = Math.Clamp(_radius, 0, Math.Min(w, h) / 2);
        var ow = w + 2 * Margin;
        var oh = h + 2 * Margin;
        SetValue(ShaderSizeProperty, new System.Windows.Size(ow, oh));
        SetValue(CornerProperty, new Point4D(r, w / 2 - r, h / 2 - r, 0));
        var ex = Math.Min(0.25, 1 / ow);
        var ey = Math.Min(0.25, 1 / oh);
        SetValue(EdgeProperty, new Point4D(ex, ey, 1 - ex, 1 - ey));
        SetValue(GeoProperty, new Point4D(1 / ow, 1 / oh, ow / 2, oh / 2));
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
