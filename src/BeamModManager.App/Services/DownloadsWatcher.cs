using System.IO;
using System.Runtime.InteropServices;
using BeamModManager.Core.Install;

namespace BeamModManager.App.Services;

/// <summary>
/// Следит за папкой «Загрузки», пока программа открыта и включено «Разрешить загрузку модов» (решено 2026-10-18: в фоне,
/// при закрытой программе, ничего не висит). Новый zip, rar или 7z отдаётся, когда браузер его докачал: имя уже без
/// «.crdownload» и «.part», размер 1,5 с не меняется и файл открывается на чтение. Что лежало до включения, не трогаем.
/// </summary>
public sealed class DownloadsWatcher : IDisposable
{
    private readonly Action<string> _ready;
    private readonly Dictionary<string, (long Size, DateTime Since)> _pending = new(StringComparer.OrdinalIgnoreCase);
    private readonly System.Threading.Timer _timer;
    private FileSystemWatcher? _fsw;

    /// <param name="ready">Архив докачан (вызывается не в потоке окна).</param>
    public DownloadsWatcher(Action<string> ready)
    {
        _ready = ready;
        _timer = new System.Threading.Timer(_ => Tick(), null, Timeout.Infinite, Timeout.Infinite);
    }

    public string? Folder => _fsw?.Path;

    /// <summary>Включить или выключить слежение. Папки нет — ничего не делает.</summary>
    public void SetEnabled(bool on)
    {
        if (on == (_fsw is not null)) return;
        if (!on)
        {
            _fsw!.Dispose();
            _fsw = null;
            _timer.Change(Timeout.Infinite, Timeout.Infinite);
            lock (_pending) _pending.Clear();
            return;
        }
        if (DownloadsPath() is not { } dir || !Directory.Exists(dir)) return;
        var w = new FileSystemWatcher(dir) { IncludeSubdirectories = false, NotifyFilter = NotifyFilters.FileName | NotifyFilters.Size | NotifyFilters.LastWrite };
        foreach (var ext in IncomingAnalyzer.Extensions) w.Filters.Add("*" + ext);
        w.Created += (_, e) => Seen(e.FullPath);
        w.Changed += (_, e) => Seen(e.FullPath);
        w.Renamed += (_, e) => Seen(e.FullPath); // браузер качает в «.crdownload» и переименовывает в конце
        w.Error += (_, _) => { }; // переполнение буфера: пропущенные события не страшны, их добьёт следующее изменение
        w.EnableRaisingEvents = true;
        _fsw = w;
        _timer.Change(500, 500);
    }

    private void Seen(string path)
    {
        if (!IncomingAnalyzer.IsCandidate(path)) return;
        lock (_pending) _pending[path] = (-1, DateTime.UtcNow);
    }

    private void Tick()
    {
        List<string> due = new();
        lock (_pending)
        {
            foreach (var (path, (size, since)) in _pending.ToList())
            {
                long now;
                try { now = new FileInfo(path).Length; }
                catch (Exception e) when (e is IOException or UnauthorizedAccessException) { _pending.Remove(path); continue; }
                if (!File.Exists(path)) { _pending.Remove(path); continue; }
                if (now != size) { _pending[path] = (now, DateTime.UtcNow); continue; }
                if (DateTime.UtcNow - since < TimeSpan.FromSeconds(1.5)) continue;
                if (!Readable(path)) continue; // браузер ещё держит файл
                _pending.Remove(path);
                due.Add(path);
            }
        }
        foreach (var p in due) _ready(p);
    }

    private static bool Readable(string path)
    {
        try { using var f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read); return f.Length > 0; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return false; }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern int SHGetKnownFolderPath([MarshalAs(UnmanagedType.LPStruct)] Guid rfid, uint flags, IntPtr token, out IntPtr path);

    /// <summary>Папка «Загрузки» пользователя (её можно перенести в настройках Windows — берём настоящую).</summary>
    public static string? DownloadsPath()
    {
        try
        {
            if (SHGetKnownFolderPath(new Guid("374DE290-123F-4565-9164-39C4925E467B"), 0, IntPtr.Zero, out var p) == 0)
            {
                try { return Marshal.PtrToStringUni(p); }
                finally { Marshal.FreeCoTaskMem(p); }
            }
        }
        catch (Exception e) when (e is DllNotFoundException or EntryPointNotFoundException) { }
        var fallback = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
        return Directory.Exists(fallback) ? fallback : null;
    }

    public void Dispose()
    {
        _fsw?.Dispose();
        _timer.Dispose();
    }
}
