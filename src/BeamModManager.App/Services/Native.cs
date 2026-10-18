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

    // ───────────── развёрнутое окно по рабочей области (2026-10-18: панель задач пропадала) ─────────────

    [StructLayout(LayoutKind.Sequential)] private struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] private struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct MINMAXINFO { public POINT Reserved, MaxSize, MaxPosition, MinTrackSize, MaxTrackSize; }
    [StructLayout(LayoutKind.Sequential)] private struct MONITORINFO { public int Size; public RECT Monitor, Work; public uint Flags; }

    [DllImport("user32.dll")] private static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] private static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFO info);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    private const uint MONITOR_DEFAULTTONEAREST = 2;
    public const int WM_GETMINMAXINFO = 0x0024;

    private static bool WorkArea(IntPtr hwnd, out RECT monitor, out RECT work)
    {
        monitor = work = default;
        var m = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if (m == IntPtr.Zero) return false;
        var info = new MONITORINFO { Size = Marshal.SizeOf<MONITORINFO>() };
        if (!GetMonitorInfo(m, ref info)) return false;
        monitor = info.Monitor;
        work = info.Work;
        return true;
    }

    /// <summary>
    /// WM_GETMINMAXINFO: окно без рамки Windows по умолчанию разворачивается на весь монитор и закрывает панель задач.
    /// Отдаём размер и место рабочей области монитора, на котором окно.
    /// </summary>
    public static void FitMaximizedToWorkArea(IntPtr hwnd, IntPtr lParam)
    {
        if (!WorkArea(hwnd, out var monitor, out var work)) return;
        // панель задач скрывается сама: рабочая область — весь монитор, и окно во весь экран не дало бы ей выехать.
        // Оставляем у её края пару пикселей, как делают окна с рамкой Windows
        if (work.Left == monitor.Left && work.Top == monitor.Top && work.Right == monitor.Right && work.Bottom == monitor.Bottom
            && AutoHideTaskbarEdge(monitor) is { } edge)
        {
            switch (edge)
            {
                case ABE_LEFT: work.Left += 2; break;
                case ABE_TOP: work.Top += 2; break;
                case ABE_RIGHT: work.Right -= 2; break;
                default: work.Bottom -= 2; break;
            }
        }
        var mmi = Marshal.PtrToStructure<MINMAXINFO>(lParam);
        mmi.MaxPosition.X = work.Left - monitor.Left;
        mmi.MaxPosition.Y = work.Top - monitor.Top;
        mmi.MaxSize.X = work.Right - work.Left;
        mmi.MaxSize.Y = work.Bottom - work.Top;
        Marshal.StructureToPtr(mmi, lParam, false);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct APPBARDATA { public int Size; public IntPtr Hwnd; public uint CallbackMessage; public uint Edge; public RECT Rect; public IntPtr Param; }

    [DllImport("shell32.dll")] private static extern UIntPtr SHAppBarMessage(uint message, ref APPBARDATA data);

    private const uint ABM_GETSTATE = 4, ABM_GETTASKBARPOS = 5, ABS_AUTOHIDE = 1;
    private const uint ABE_LEFT = 0, ABE_TOP = 1, ABE_RIGHT = 2;

    /// <summary>Край монитора, у которого прячется панель задач, если она скрывается сама и стоит на этом мониторе.</summary>
    private static uint? AutoHideTaskbarEdge(RECT monitor)
    {
        var data = new APPBARDATA { Size = Marshal.SizeOf<APPBARDATA>() };
        if ((SHAppBarMessage(ABM_GETSTATE, ref data).ToUInt64() & ABS_AUTOHIDE) == 0) return null;
        if (SHAppBarMessage(ABM_GETTASKBARPOS, ref data) == UIntPtr.Zero) return null;
        var r = data.Rect;
        var onThisMonitor = r.Left < monitor.Right && r.Right > monitor.Left && r.Top < monitor.Bottom && r.Bottom > monitor.Top;
        return onThisMonitor ? data.Edge : null;
    }

    /// <summary>
    /// Насколько развёрнутое окно выходит за рабочую область (в пикселях): слева, сверху, справа, снизу. Содержимое отодвигается
    /// на столько же, чтобы ничего не уходило за край экрана; если окно в рабочей области — нули.
    /// </summary>
    public static (int Left, int Top, int Right, int Bottom) Overhang(IntPtr hwnd)
    {
        if (!WorkArea(hwnd, out _, out var work) || !GetWindowRect(hwnd, out var r)) return (0, 0, 0, 0);
        return (Math.Max(0, work.Left - r.Left), Math.Max(0, work.Top - r.Top), Math.Max(0, r.Right - work.Right), Math.Max(0, r.Bottom - work.Bottom));
    }
}
