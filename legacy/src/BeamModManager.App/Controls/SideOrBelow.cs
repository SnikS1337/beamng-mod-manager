using System.Windows;
using System.Windows.Controls;

namespace BeamModManager.App.Controls;

/// <summary>
/// Строка настройки: подпись слева, выбор (сегменты, счётчик) справа. Если подписи остаётся меньше <see cref="MinText"/>,
/// выбор уходит под подпись — иначе на некоторой ширине окна подпись сжималась до пары букв в строке.
/// Первый ребёнок — подпись, второй — выбор.
/// </summary>
public sealed class SideOrBelow : Panel
{
    public double Gap { get; set; } = 16;
    public double MinText { get; set; } = 200;

    private bool _below;

    protected override Size MeasureOverride(Size available)
    {
        if (InternalChildren.Count < 2) return base.MeasureOverride(available);
        UIElement text = InternalChildren[0], pick = InternalChildren[1];
        var w = double.IsInfinity(available.Width) ? 10000 : available.Width;
        pick.Measure(new Size(w, double.PositiveInfinity));
        var room = w - pick.DesiredSize.Width - Gap;
        _below = room < MinText;
        if (_below)
        {
            text.Measure(new Size(w, double.PositiveInfinity));
            return new Size(Math.Max(text.DesiredSize.Width, pick.DesiredSize.Width), text.DesiredSize.Height + 10 + pick.DesiredSize.Height);
        }
        text.Measure(new Size(room, double.PositiveInfinity));
        return new Size(w, Math.Max(text.DesiredSize.Height, pick.DesiredSize.Height));
    }

    protected override Size ArrangeOverride(Size final)
    {
        if (InternalChildren.Count < 2) return base.ArrangeOverride(final);
        UIElement text = InternalChildren[0], pick = InternalChildren[1];
        if (_below)
        {
            text.Arrange(new Rect(0, 0, final.Width, text.DesiredSize.Height));
            pick.Arrange(new Rect(0, text.DesiredSize.Height + 10, pick.DesiredSize.Width, pick.DesiredSize.Height));
        }
        else
        {
            var pw = pick.DesiredSize.Width;
            var room = Math.Max(0, final.Width - pw - Gap);
            text.Arrange(new Rect(0, (final.Height - text.DesiredSize.Height) / 2, room, text.DesiredSize.Height));
            pick.Arrange(new Rect(final.Width - pw, (final.Height - pick.DesiredSize.Height) / 2, pw, pick.DesiredSize.Height));
        }
        return final;
    }
}
