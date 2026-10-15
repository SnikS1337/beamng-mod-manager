using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace BeamModManager.App.Services;

/// <summary>
/// Раскладки окна Windows 11 при наведении на «Развернуть» (DESIGN_FLOWS раздел 2). Система показывает их, только если
/// на вопрос «что под мышью» (WM_NCHITTEST) окно отвечает «кнопка развернуть». Тогда мышь над кнопкой приходит как
/// «неклиентская», поэтому наведение (капля) и нажатие обрабатываются здесь же.
/// </summary>
internal static class SnapLayouts
{
    private const int WM_NCHITTEST = 0x0084, WM_NCMOUSEMOVE = 0x00A0, WM_NCLBUTTONDOWN = 0x00A1, WM_NCLBUTTONUP = 0x00A2, WM_NCMOUSELEAVE = 0x02A2;
    private const int HTMAXBUTTON = 9;

    public static void Attach(Window w, FrameworkElement maxButton, Action<bool> hover, Action click)
    {
        if (HwndSource.FromHwnd(new WindowInteropHelper(w).Handle) is not { } src) return;
        var pressed = false;
        src.AddHook((IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled) =>
        {
            switch (msg)
            {
                case WM_NCHITTEST:
                    if (Over(maxButton, lParam)) { handled = true; return HTMAXBUTTON; }
                    break;
                case WM_NCMOUSEMOVE:
                    hover(wParam == HTMAXBUTTON);
                    break;
                case WM_NCMOUSELEAVE:
                    pressed = false;
                    hover(false);
                    break;
                case WM_NCLBUTTONDOWN when wParam == HTMAXBUTTON:
                    pressed = true;
                    handled = true;
                    break;
                case WM_NCLBUTTONUP when wParam == HTMAXBUTTON:
                    if (pressed) click();
                    pressed = false;
                    handled = true;
                    break;
            }
            return IntPtr.Zero;
        });
    }

    private static bool Over(FrameworkElement el, IntPtr lParam)
    {
        if (!el.IsVisible || PresentationSource.FromVisual(el) is null) return false;
        var x = unchecked((short)(long)lParam);
        var y = unchecked((short)((long)lParam >> 16));
        try
        {
            var p = el.PointFromScreen(new Point(x, y));
            return p.X >= 0 && p.Y >= 0 && p.X < el.ActualWidth && p.Y < el.ActualHeight;
        }
        catch (InvalidOperationException) { return false; }
    }
}
