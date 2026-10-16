using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Media;

namespace BeamModManager.App.Controls;

/// <summary>
/// Список карточек одной высоты в одну колонку («Обновления»), который создаёт только видимые карточки с запасом в экран
/// сверху и снизу, остальные — при прокрутке. Высота списка полная, прокрутка честная. Как у сетки гаража
/// (<see cref="CardGridPanel"/>): сотни карточек со стеклом и тенью иначе создавались бы секундами при каждом показе.
/// </summary>
public sealed class RowListPanel : VirtualizingPanel
{
    /// <summary>Шаг ряда: высота карточки вместе с отступом под ней.</summary>
    public double RowHeight { get; set; } = 216;

    private ScrollViewer? _scroll;
    private int _first, _last = -1;

    public RowListPanel()
    {
        Loaded += (_, _) =>
        {
            for (var p = VisualTreeHelper.GetParent(this); p is not null && _scroll is null; p = VisualTreeHelper.GetParent(p))
                _scroll = p as ScrollViewer;
            if (_scroll is not null) _scroll.ScrollChanged += (_, _) => OnScrolled();
            InvalidateMeasure();
        };
        // раздел показали (до этого высота окна прокрутки была нулевой) — пересчитать, что видно
        IsVisibleChanged += (_, _) => InvalidateMeasure();
    }

    private (int First, int Last) WantedRange(int count)
    {
        if (count == 0) return (0, -1);
        if (_scroll is null || !IsLoaded || _scroll.ViewportHeight <= 0) return (0, Math.Min(count - 1, 7));
        double origin;
        try { origin = TransformToAncestor(_scroll).Transform(new Point(0, 0)).Y; }
        catch (InvalidOperationException) { return (0, Math.Min(count - 1, 7)); }
        var pad = _scroll.ViewportHeight;
        var top = -origin - pad;
        var bottom = -origin + _scroll.ViewportHeight + pad;
        var first = Math.Max(0, (int)Math.Floor(top / RowHeight));
        var last = Math.Max(first, (int)Math.Floor(Math.Max(0, bottom) / RowHeight));
        return (Math.Min(count - 1, first), Math.Min(count - 1, last));
    }

    private void OnScrolled()
    {
        var (f, l) = WantedRange(ItemsControl.GetItemsOwner(this)?.Items.Count ?? 0);
        if (f < _first || l > _last) InvalidateMeasure();
    }

    protected override Size MeasureOverride(Size available)
    {
        var w = double.IsInfinity(available.Width) ? 800 : available.Width;
        var count = ItemsControl.GetItemsOwner(this)?.Items.Count ?? 0;
        var (first, last) = WantedRange(count);
        Realize(first, last);
        foreach (UIElement c in InternalChildren) c.Measure(new Size(w, RowHeight));
        return new Size(w, count * RowHeight);
    }

    protected override Size ArrangeOverride(Size final)
    {
        var gen = ItemContainerGenerator;
        for (var i = 0; i < InternalChildren.Count; i++)
        {
            var index = gen?.IndexFromGeneratorPosition(new GeneratorPosition(i, 0)) ?? i;
            if (index < 0) index = i;
            InternalChildren[i].Arrange(new Rect(0, index * RowHeight, final.Width, RowHeight));
        }
        return final;
    }

    private void Realize(int first, int last)
    {
        _ = InternalChildren; // без этого обращения генератор у панели ещё не готов
        var gen = ItemContainerGenerator;
        if (gen is null) return;
        for (var i = InternalChildren.Count - 1; i >= 0; i--)
        {
            var pos = new GeneratorPosition(i, 0);
            var index = gen.IndexFromGeneratorPosition(pos);
            if (index >= first && index <= last) continue;
            gen.Remove(pos, 1);
            RemoveInternalChildRange(i, 1);
        }
        _first = first;
        _last = last;
        if (last < first) return;
        var start = gen.GeneratorPositionFromIndex(first);
        var child = start.Offset == 0 ? start.Index : start.Index + 1;
        using (gen.StartAt(start, GeneratorDirection.Forward, allowStartAtRealizedItem: true))
            for (var index = first; index <= last; index++, child++)
            {
                if (gen.GenerateNext(out var isNew) is not UIElement el) break;
                if (!isNew) continue;
                if (child >= InternalChildren.Count) AddInternalChild(el); else InsertInternalChild(child, el);
                gen.PrepareItemContainer(el);
            }
    }

    protected override void OnItemsChanged(object sender, ItemsChangedEventArgs args)
    {
        switch (args.Action)
        {
            case System.Collections.Specialized.NotifyCollectionChangedAction.Remove:
            case System.Collections.Specialized.NotifyCollectionChangedAction.Replace:
            case System.Collections.Specialized.NotifyCollectionChangedAction.Move:
                if (args.ItemUICount > 0) RemoveInternalChildRange(args.Position.Index, args.ItemUICount);
                break;
        }
        base.OnItemsChanged(sender, args);
    }
}
