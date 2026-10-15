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

    /// <summary>Скруглённые углы окна Windows 11 и тёмная системная рамка. На Windows 10 вызовы тихо не срабатывают.</summary>
    public static void StyleWindow(Window w)
    {
        var h = new WindowInteropHelper(w).Handle;
        if (h == IntPtr.Zero) return;
        var round = DWMWCP_ROUND;
        _ = DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, ref round, sizeof(int));
        SetDarkFrame(w, true);
    }

    /// <summary>Тёмная или светлая системная рамка окна (тонкая кромка и тень Windows 11) — под оформление программы.</summary>
    public static void SetDarkFrame(Window w, bool dark)
    {
        var h = new WindowInteropHelper(w).Handle;
        if (h == IntPtr.Zero) return;
        var v = dark ? 1 : 0;
        _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, ref v, sizeof(int));
    }

    /// <summary>
    /// В Windows выключены «Эффекты прозрачности» (Параметры → Персонализация → Цвета). Тогда стекло плоское.
    /// Нет ключа или нет прав — считаем, что прозрачность включена.
    /// </summary>
    public static bool TransparencyOff() => PersonalizeValue("EnableTransparency") == 0;

    /// <summary>В Windows выбран светлый режим приложений (для оформления «Как в Windows»).</summary>
    public static bool SystemLight() => PersonalizeValue("AppsUseLightTheme") == 1;

    private static int? PersonalizeValue(string name)
    {
        try
        {
            using var k = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            return k?.GetValue(name) is int v ? v : null;
        }
        catch (Exception e) when (e is System.Security.SecurityException or UnauthorizedAccessException or System.IO.IOException) { return null; }
    }
}
