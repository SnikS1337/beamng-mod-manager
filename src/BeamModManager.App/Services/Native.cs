using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace BeamModManager.App.Services;

internal static class Native
{
    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    private const int DWMWA_USE_IMMERSIVE_DARK_MODE = 20;
    private const int DWMWA_WINDOW_CORNER_PREFERENCE = 33;
    private const int DWMWCP_ROUND = 2;

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MINMAXINFO { public POINT Reserved, MaxSize, MaxPosition, MinTrackSize, MaxTrackSize; }

    /// <summary>
    /// Только для снимков на сборочной машине: её экран 1024 × 768, и Windows не даёт окну стать больше.
    /// Снимаем это ограничение, чтобы снять окно в настоящем размере 1440 × 960.
    /// </summary>
    public static void AllowLargerThanScreen(Window w)
    {
        if (HwndSource.FromHwnd(new WindowInteropHelper(w).Handle) is not { } src) return;
        src.AddHook((IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled) =>
        {
            if (msg != 0x0024) return IntPtr.Zero; // WM_GETMINMAXINFO
            var mmi = Marshal.PtrToStructure<MINMAXINFO>(lParam);
            mmi.MaxTrackSize = new POINT { X = 8000, Y = 8000 };
            Marshal.StructureToPtr(mmi, lParam, false);
            handled = true;
            return IntPtr.Zero;
        });
    }

    /// <summary>Скруглённые углы окна Windows 11 и тёмная системная рамка. На Windows 10 вызовы тихо не срабатывают.</summary>
    public static void StyleWindow(Window w)
    {
        var h = new WindowInteropHelper(w).Handle;
        if (h == IntPtr.Zero) return;
        var round = DWMWCP_ROUND;
        _ = DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, ref round, sizeof(int));
        var dark = 1;
        _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, ref dark, sizeof(int));
    }
}
