namespace BeamModManager.Ui.Look;

/// <summary>
/// Минимальный размер окна по экрану. Обычно 1000 × 680 (DESIGN_FLOWS раздел 2), но экран бывает меньше: ноутбук
/// 1366 × 768 с масштабом 125 % — это 1093 × 614 рабочей области, а со 150 % — 911 × 512. Окно больше экрана Windows
/// не сжимает: низ и правый край уходили бы за экран вместе с кнопками. Поэтому минимум не больше рабочей области
/// (за вычетом рамки 16), но и не меньше 640 × 480 — дальше раскладка не имеет смысла.
/// </summary>
public static class WindowFit
{
    public const double MinWidth = 1000, MinHeight = 680, Frame = 16, FloorWidth = 640, FloorHeight = 480;

    public static (double Width, double Height) Minimum(double workWidth, double workHeight)
    {
        var w = double.IsFinite(workWidth) && workWidth > 0 ? workWidth - Frame : MinWidth;
        var h = double.IsFinite(workHeight) && workHeight > 0 ? workHeight - Frame : MinHeight;
        return (Math.Clamp(w, FloorWidth, MinWidth), Math.Clamp(h, FloorHeight, MinHeight));
    }
}
