using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace BeamModManager.App.Motion;

/// <summary>
/// Общие кусочки анимаций ответа (DESIGN_MOTION раздел 5): те же пружины, что «Lift» и «Smooth» в Theme.xaml.
/// Длительность анимации с пружиной берётся в 1,6 раза больше ощущаемой (<see cref="SpringEase.Settle"/>).
/// </summary>
public static class Anim
{
    /// <summary>Пружина без отскока: появление, перестроение, смена раздела.</summary>
    public static readonly SpringEase Smooth = Frozen(new SpringEase { Bounce = 0 });

    /// <summary>Пружина с упругостью 0,15: подъём карточки, выбор.</summary>
    public static readonly SpringEase Lift = Frozen(new SpringEase { Bounce = 0.15 });

    /// <summary>Сдвиг элемента для анимаций: свой незамороженный TranslateTransform (создаётся при первом обращении).</summary>
    public static TranslateTransform Mover(UIElement el)
    {
        if (el.RenderTransform is TranslateTransform { IsFrozen: false } t) return t;
        var m = new TranslateTransform();
        el.RenderTransform = m;
        return m;
    }

    /// <summary>Держит <paramref name="from"/> до <paramref name="delay"/>, затем пружиной идёт к <paramref name="to"/> за <paramref name="time"/>.</summary>
    public static DoubleAnimationUsingKeyFrames Hold(double from, double to, TimeSpan delay, TimeSpan time, IEasingFunction ease)
    {
        var a = new DoubleAnimationUsingKeyFrames();
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(from, KeyTime.FromTimeSpan(TimeSpan.Zero)));
        a.KeyFrames.Add(new DiscreteDoubleKeyFrame(from, KeyTime.FromTimeSpan(delay)));
        a.KeyFrames.Add(new EasingDoubleKeyFrame(to, KeyTime.FromTimeSpan(delay + time), ease));
        return a;
    }

    /// <summary>Обычная анимация from → to с пружиной.</summary>
    public static DoubleAnimation Go(double from, double to, double seconds, IEasingFunction ease) =>
        new DoubleAnimation(from, to, TimeSpan.FromSeconds(seconds)) { EasingFunction = ease };

    private static T Frozen<T>(T f) where T : Freezable { f.Freeze(); return f; }
}
