using System.Windows;
using System.Windows.Controls;

namespace BeamModManager.App.Controls;

/// <summary>
/// Сетка карточек: 2–6 колонок, карточка 280–340 px, промежуток 20, пропорции 308 × 236 (DESIGN.md раздел 4).
/// В окне 1440 получается ровно 3 колонки по 308.
/// </summary>
public sealed class CardGridPanel : Panel
{
    public const double Gap = 20, MinItem = 280, MaxItem = 340, Ratio = 236.0 / 308.0;

    public int Columns { get; private set; } = 3;
    public double ItemWidth { get; private set; } = 308;

    public event EventHandler? ColumnsChanged;

    protected override Size MeasureOverride(Size available)
    {
        var w = double.IsInfinity(available.Width) ? 3 * 308 + 2 * Gap : available.Width;
        var cols = Math.Clamp((int)Math.Floor((w + Gap) / (MinItem + Gap)), 1, 6);
        var itemW = Math.Min(MaxItem, Math.Floor((w - Gap * (cols - 1)) / cols));
        var itemH = Math.Round(itemW * Ratio);
        if (cols != Columns) { Columns = cols; ColumnsChanged?.Invoke(this, EventArgs.Empty); }
        ItemWidth = itemW;
        foreach (UIElement c in InternalChildren) c.Measure(new Size(itemW, itemH));
        var n = InternalChildren.Count;
        var rows = (n + cols - 1) / cols;
        return new Size(w, rows == 0 ? 0 : rows * itemH + (rows - 1) * Gap);
    }

    protected override Size ArrangeOverride(Size final)
    {
        var itemW = ItemWidth;
        var itemH = Math.Round(itemW * Ratio);
        for (var i = 0; i < InternalChildren.Count; i++)
        {
            var row = i / Columns;
            var col = i % Columns;
            InternalChildren[i].Arrange(new Rect(col * (itemW + Gap), row * (itemH + Gap), itemW, itemH));
        }
        return final;
    }
}
