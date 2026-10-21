using System.Windows;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Серый налёт выключенного мода (DESIGN_MOTION 3.1). Мод выключили или включили на глазах — налёт плавно проявляется
/// (0,45 с) или гаснет (0,35 с). Карточка только что создана (прокрутка, фильтр, запуск) — сразу в нужном виде: раньше
/// это делал триггер разметки, и каждая выключенная карточка при прокрутке на глазах «засыпала» заново.
/// </summary>
public static class SleepFade
{
    /// <summary>Мод включён (true — налёта нет). Привязывается к Enabled карточки.</summary>
    public static readonly DependencyProperty AwakeProperty = DependencyProperty.RegisterAttached(
        "Awake", typeof(bool), typeof(SleepFade), new PropertyMetadata(true, OnAwakeChanged));

    public static bool GetAwake(DependencyObject d) => (bool)d.GetValue(AwakeProperty);
    public static void SetAwake(DependencyObject d, bool value) => d.SetValue(AwakeProperty, value);

    private static void OnAwakeChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is not FrameworkElement el) return;
        var to = (bool)e.NewValue ? 0.0 : 1.0;
        if (!el.IsLoaded || SpringDriver.ReducedMotion)
        {
            el.BeginAnimation(UIElement.OpacityProperty, null);
            el.Opacity = to;
            return;
        }
        el.BeginAnimation(UIElement.OpacityProperty,
            new DoubleAnimation(to, TimeSpan.FromSeconds(to > 0 ? 0.45 : 0.35)) { EasingFunction = Anim.Smooth });
    }
}
