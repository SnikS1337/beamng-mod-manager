using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Animation;
using BeamModManager.App.Motion;

namespace BeamModManager.App.Controls;

/// <summary>
/// Светлая полоса по скелету (DESIGN_COMPONENTS 3.10): раз в 6 с проходит слева направо за 1,2 с, остальное время её
/// нет. Пока скелет не на экране — стоит; при выключенных анимациях её нет совсем. 30 кадров в секунду: полоса мягкая.
/// </summary>
public static class Shimmer
{
    public static readonly DependencyProperty RunProperty = DependencyProperty.RegisterAttached(
        "Run", typeof(bool), typeof(Shimmer), new PropertyMetadata(false, OnRunChanged));

    public static bool GetRun(DependencyObject d) => (bool)d.GetValue(RunProperty);
    public static void SetRun(DependencyObject d, bool value) => d.SetValue(RunProperty, value);

    private static void OnRunChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is not FrameworkElement el) return;
        el.Loaded -= Start;
        el.Unloaded -= Stop;
        if (e.NewValue is not true) { Stop(el, null!); return; }
        el.Loaded += Start;
        el.Unloaded += Stop;
        if (el.IsLoaded) Start(el, null!);
    }

    private static void Start(object sender, RoutedEventArgs e)
    {
        var el = (FrameworkElement)sender;
        if (SpringDriver.ReducedMotion) { el.Visibility = Visibility.Hidden; return; }
        var move = el.RenderTransform as TranslateTransform ?? new TranslateTransform();
        el.RenderTransform = move;
        var width = (el.Parent as FrameworkElement)?.ActualWidth is > 0 and var w ? w : 340;
        var a = new DoubleAnimationUsingKeyFrames { Duration = TimeSpan.FromSeconds(6), RepeatBehavior = RepeatBehavior.Forever };
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(-el.Width - 20, KeyTime.FromTimeSpan(TimeSpan.Zero)));
        a.KeyFrames.Add(new EasingDoubleKeyFrame(width + 20, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(1.2)), Anim.Smooth));
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(width + 20, KeyTime.FromTimeSpan(TimeSpan.FromSeconds(6))));
        Timeline.SetDesiredFrameRate(a, 30);
        move.BeginAnimation(TranslateTransform.XProperty, a);
    }

    private static void Stop(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { RenderTransform: TranslateTransform move }) move.BeginAnimation(TranslateTransform.XProperty, null);
    }
}
