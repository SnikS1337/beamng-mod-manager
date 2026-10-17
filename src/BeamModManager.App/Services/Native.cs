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

#pragma warning disable CS0649 // поля заполняет Windows
    private struct SystemPowerStatus
    {
        public byte ACLineStatus;
        public byte BatteryFlag;
        public byte BatteryLifePercent;
        public byte SystemStatusFlag;
        public int BatteryLifeTime;
        public int BatteryFullLifeTime;
    }
#pragma warning restore CS0649

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetSystemPowerStatus(out SystemPowerStatus status);

    /// <summary>
    /// Компьютер работает от батареи или в Windows включена экономия энергии: анимации не быстрее 60 кадров
    /// (FramePolicy). Настольный компьютер без батареи и неизвестное состояние — нет.
    /// </summary>
    [DllImport("user32.dll")] private static extern IntPtr GetDC(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern int ReleaseDC(IntPtr hwnd, IntPtr hdc);
    [DllImport("gdi32.dll")] private static extern int GetDeviceCaps(IntPtr hdc, int index);

    /// <summary>Частота основного экрана, Гц; 0 — не узнать (тогда фон идёт на 30 кадрах, BackgroundPace).</summary>
    public static int RefreshRate()
    {
        const int VREFRESH = 116;
        try
        {
            var dc = GetDC(IntPtr.Zero);
            if (dc == IntPtr.Zero) return 0;
            try { var hz = GetDeviceCaps(dc, VREFRESH); return hz > 1 ? hz : 0; } // 0 и 1 — «по умолчанию у устройства»
            finally { ReleaseDC(IntPtr.Zero, dc); }
        }
        catch (Exception e) when (e is DllNotFoundException or EntryPointNotFoundException) { return 0; }
    }

    public static bool PowerSaving()
    {
        try { return GetSystemPowerStatus(out var s) && (s.ACLineStatus == 0 || s.SystemStatusFlag == 1); }
        catch (Exception e) when (e is DllNotFoundException or EntryPointNotFoundException) { return false; }
    }

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
