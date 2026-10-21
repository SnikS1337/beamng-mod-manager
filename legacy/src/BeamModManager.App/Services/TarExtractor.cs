using System.Diagnostics;
using System.IO;
using BeamModManager.Core.Install;

namespace BeamModManager.App.Services;

/// <summary>
/// RAR и 7z через tar из Windows (bsdtar, есть в Windows 10 с 2018 года и в Windows 11; решено 2026-10-18: без сторонних
/// пакетов). Новый формат RAR5 читает только tar свежей Windows 11 — на старой будет понятная причина. tar сам не выпускает
/// файлы из папки (пути с «..» и абсолютные отбрасываются, ссылки наружу не открываются); размер распакованного окно
/// проверяет на ходу и прерывает распаковку, если архив разрастается больше предела.
/// </summary>
public sealed class TarExtractor : IArchiveExtractor
{
    private static readonly TimeSpan Timeout = TimeSpan.FromMinutes(10);

    public static string? TarPath
    {
        get
        {
            var p = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "tar.exe");
            return File.Exists(p) ? p : null;
        }
    }

    public string? Extract(string archivePath, string targetDir, long maxBytes, CancellationToken ct)
    {
        if (TarPath is not { } tar) return "В этой Windows нет распаковщика RAR и 7z. Перепакуйте архив в zip";
        var psi = new ProcessStartInfo(tar)
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardError = true,
            RedirectStandardOutput = true,
            WorkingDirectory = targetDir,
        };
        psi.ArgumentList.Add("-xf");
        psi.ArgumentList.Add(archivePath);
        psi.ArgumentList.Add("-C");
        psi.ArgumentList.Add(targetDir);
        using var p = new Process { StartInfo = psi };
        var err = new System.Text.StringBuilder();
        p.ErrorDataReceived += (_, e) => { if (e.Data is { } line && err.Length < 4000) err.AppendLine(line); };
        p.OutputDataReceived += (_, _) => { };
        try { p.Start(); }
        catch (Exception e) when (e is System.ComponentModel.Win32Exception or InvalidOperationException) { return "Не удалось запустить распаковщик: " + e.Message; }
        p.BeginErrorReadLine();
        p.BeginOutputReadLine();
        var sw = Stopwatch.StartNew();
        string? stop = null;
        while (!p.WaitForExit(250))
        {
            if (ct.IsCancellationRequested) stop = "отменено";
            else if (sw.Elapsed > Timeout) stop = "Распаковка идёт слишком долго";
            else if (SizeOf(targetDir) > maxBytes) stop = "Распакованный архив слишком большой";
            if (stop is null) continue;
            try { p.Kill(entireProcessTree: true); } catch (Exception e) when (e is InvalidOperationException or System.ComponentModel.Win32Exception) { }
            p.WaitForExit(5000);
            ct.ThrowIfCancellationRequested();
            return stop;
        }
        p.WaitForExit(); // дочитать вывод
        if (SizeOf(targetDir) > maxBytes) return "Распакованный архив слишком большой";
        if (p.ExitCode == 0) return null;
        return Explain(err.ToString(), Path.GetExtension(archivePath));
    }

    /// <summary>
    /// Почему tar не распаковал — словами. Встроенный tar старой Windows 10 не умеет ни RAR5, ни 7z со сжатием LZMA (а это
    /// почти все 7z): тогда честно «Windows не умеет», а не «повреждён» (2026-10-18, найдено перед 0.3).
    /// </summary>
    public static string Explain(string stderr, string ext)
    {
        bool Has(string s) => stderr.Contains(s, StringComparison.OrdinalIgnoreCase);
        if (Has("encrypt") || Has("passphrase") || Has("password")) return "Архив под паролем";
        var unsupported = Has("RAR5") || Has("Unrecognized archive format") || Has("not supported") || Has("unsupported") || Has("lzma") || Has("bzip2") || Has("ppmd");
        if (unsupported && ext.Equals(".rar", StringComparison.OrdinalIgnoreCase))
            return "Этот RAR новой версии, а распаковщик Windows его не читает. Перепакуйте архив в zip";
        if (unsupported) return "Распаковщик Windows этот архив не читает (так бывает на Windows 10). Перепакуйте его в zip";
        return "Архив не распаковался: он повреждён или недокачан";
    }

    private static long SizeOf(string dir)
    {
        long total = 0;
        try
        {
            // ссылки не считаем (и не идём по ним): размер — только того, что распаковано на самом деле
            foreach (var f in new DirectoryInfo(dir).EnumerateFiles("*", new EnumerationOptions { RecurseSubdirectories = true, IgnoreInaccessible = true, AttributesToSkip = FileAttributes.ReparsePoint }))
                total += f.Length;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
        return total;
    }
}
