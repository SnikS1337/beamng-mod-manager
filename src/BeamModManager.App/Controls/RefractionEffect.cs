using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Effects;

namespace BeamModManager.App.Controls;

/// <summary>
/// Преломление «жидкого стекла» по краям панели (DESIGN.md 3.2, шейдер Shaders/Glass.fx, PS 2.0: WPF рисует его и без видеокарты).
/// Все длины (<see cref="Size"/>, <see cref="Radius"/>, <see cref="Lens"/>, <see cref="Strength"/>) в одних единицах,
/// обычно в точках WPF: шейдер работает с их отношением. <see cref="Strength"/> = 0 — без преломления.
/// Перед использованием проверить <see cref="IsAvailable"/>; если false — матовое стекло без шейдера.
/// </summary>
public sealed class RefractionEffect : ShaderEffect
{
    private const string ShaderPath = "pack://application:,,,/Shaders/Glass.ps";
    private static PixelShader? _shader;
    private static bool _loaded;

    public static readonly DependencyProperty InputProperty =
        RegisterPixelShaderSamplerProperty("Input", typeof(RefractionEffect), 0);

    public static readonly DependencyProperty SizeProperty = DependencyProperty.Register(
        nameof(Size), typeof(System.Windows.Size), typeof(RefractionEffect),
        new UIPropertyMetadata(new System.Windows.Size(1, 1), PixelShaderConstantCallback(0)));

    public static readonly DependencyProperty RadiusProperty = DependencyProperty.Register(
        nameof(Radius), typeof(double), typeof(RefractionEffect), new UIPropertyMetadata(26.0, PixelShaderConstantCallback(1)));

    public static readonly DependencyProperty LensProperty = DependencyProperty.Register(
        nameof(Lens), typeof(double), typeof(RefractionEffect), new UIPropertyMetadata(40.0, PixelShaderConstantCallback(2)));

    public static readonly DependencyProperty StrengthProperty = DependencyProperty.Register(
        nameof(Strength), typeof(double), typeof(RefractionEffect), new UIPropertyMetadata(0.0, PixelShaderConstantCallback(3)));

    public static readonly DependencyProperty SaturationProperty = DependencyProperty.Register(
        nameof(Saturation), typeof(double), typeof(RefractionEffect), new UIPropertyMetadata(1.0, PixelShaderConstantCallback(4)));

    public static readonly DependencyProperty BrightnessProperty = DependencyProperty.Register(
        nameof(Brightness), typeof(double), typeof(RefractionEffect), new UIPropertyMetadata(1.0, PixelShaderConstantCallback(5)));

    /// <summary>Что преломляется. Обычно не задаётся: WPF подставляет сам элемент с эффектом.</summary>
    public Brush Input { get => (Brush)GetValue(InputProperty); set => SetValue(InputProperty, value); }

    /// <summary>Размер панели (ActualWidth × ActualHeight элемента с эффектом).</summary>
    public System.Windows.Size Size { get => (System.Windows.Size)GetValue(SizeProperty); set => SetValue(SizeProperty, value); }

    /// <summary>Радиус скругления панели.</summary>
    public double Radius { get => (double)GetValue(RadiusProperty); set => SetValue(RadiusProperty, value); }

    /// <summary>Ширина линзы у края: дальше от края смещения нет.</summary>
    public double Lens { get => (double)GetValue(LensProperty); set => SetValue(LensProperty, value); }

    /// <summary>Сила преломления, как scale у feDisplacementMap в макете: у самого края сдвиг Strength / 2.</summary>
    public double Strength { get => (double)GetValue(StrengthProperty); set => SetValue(StrengthProperty, value); }

    /// <summary>Насыщенность, как saturate() в CSS: 1 — без изменений.</summary>
    public double Saturation { get => (double)GetValue(SaturationProperty); set => SetValue(SaturationProperty, value); }

    /// <summary>Яркость, как brightness() в CSS: 1 — без изменений.</summary>
    public double Brightness { get => (double)GetValue(BrightnessProperty); set => SetValue(BrightnessProperty, value); }

    /// <summary>Шейдер загрузился. PS 2.0 WPF рисует на видеокарте, а без неё программно. Звать из потока окна.</summary>
    public static bool IsAvailable => Shader() is not null;

    public RefractionEffect()
    {
        if (Shader() is PixelShader shader) PixelShader = shader;
        PaddingLeft = 0;
        PaddingTop = 0;
        PaddingRight = 0;
        PaddingBottom = 0;
        UpdateShaderValue(InputProperty);
        UpdateShaderValue(SizeProperty);
        UpdateShaderValue(RadiusProperty);
        UpdateShaderValue(LensProperty);
        UpdateShaderValue(StrengthProperty);
        UpdateShaderValue(SaturationProperty);
        UpdateShaderValue(BrightnessProperty);
    }

    /// <summary>Один шейдер на все панели; грузится при первом обращении, неудача запоминается.</summary>
    private static PixelShader? Shader()
    {
        if (_loaded) return _shader;
        _loaded = true;
        try
        {
            // Uri создаётся здесь, а не в статическом поле: схема pack появляется вместе с Application
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
        // ресурса нет (шейдер не собран), схема pack не зарегистрирована, байты не похожи на шейдер
        catch (Exception e) when (e is IOException or UriFormatException or InvalidOperationException or ArgumentException)
        {
            _shader = null;
        }
        return _shader;
    }
}
