using System.IO.Compression;

namespace BeamModManager.Core.AppUpdate;

/// <summary>
/// Файловая часть обновления. Работающий exe в Windows нельзя перезаписать, но можно переименовать: старый уходит
/// в «*.old», новый встаёт на его место, затем программа перезапускается. Любой сбой — откат к прежнему exe.
/// </summary>
public static class UpdateInstaller
{
    public const string ExeName = "BeamModManager.exe";
    public const string OldSuffix = ".old";

    /// <summary>
    /// Достать exe из скачанного архива в папку подготовки. В архиве нужен ровно один BeamModManager.exe в корне;
    /// пути с папками и «..» не принимаются. <paramref name="versionOf"/> читает версию exe (на Windows — из его ресурсов).
    /// </summary>
    public static (string? ExePath, string? Error) Extract(string zipPath, string stagingDir, Version expected, Func<string, Version?> versionOf)
    {
        try
        {
            Directory.CreateDirectory(stagingDir);
            var dst = Path.Combine(stagingDir, ExeName);
            using (var z = ZipFile.OpenRead(zipPath))
            {
                var entries = z.Entries.Where(e => e.FullName.Equals(ExeName, StringComparison.OrdinalIgnoreCase)).ToList();
                if (entries.Count != 1) return (null, "в архиве обновления нет программы");
                var e = entries[0];
                if (e.Length is < UpdateManifest.MinSize or > UpdateManifest.MaxSize) return (null, "программа в архиве неправдоподобного размера");
                e.ExtractToFile(dst, overwrite: true);
            }
            if (!LooksLikeExe(dst)) { TryDelete(dst); return (null, "в архиве не программа Windows"); }
            var v = versionOf(dst);
            if (v is null || AppVersion.Normalize(v) != AppVersion.Normalize(expected))
            {
                TryDelete(dst);
                return (null, $"версия в архиве ({(v is null ? "?" : AppVersion.Display(v))}) не та, что обещана ({AppVersion.Display(expected)})");
            }
            return (dst, null);
        }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException or InvalidDataException)
        {
            return (null, "архив обновления не распаковался: " + x.Message);
        }
    }

    /// <summary>
    /// Поставить новый exe на место текущего: текущий → *.old, новый → на его место. Если второй шаг не удался, текущий
    /// возвращается обратно. Ошибка — понятная причина (например, нет прав на папку программы).
    /// </summary>
    public static string? Swap(string currentExe, string newExe)
    {
        var old = currentExe + OldSuffix;
        try
        {
            if (!File.Exists(newExe)) return "подготовленное обновление пропало";
            if (File.Exists(old)) File.Delete(old); // остался с прошлого обновления
        }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { return "не удалось убрать старую копию: " + x.Message; }

        try { File.Move(currentExe, old); }
        catch (UnauthorizedAccessException) { return "нет прав менять файлы в папке программы. Перенесите её, например, на рабочий стол"; }
        catch (IOException x) { return "не удалось отодвинуть текущую версию: " + x.Message; }

        for (var attempt = 0; ; attempt++)
        {
            try { File.Move(newExe, currentExe); return null; }
            catch (Exception x) when ((x is IOException or UnauthorizedAccessException) && attempt < 3)
            {
                Thread.Sleep(300 * (attempt + 1)); // антивирус проверяет свежий файл
            }
            catch (Exception x) when (x is IOException or UnauthorizedAccessException)
            {
                // откат: прежняя версия на место
                try { if (!File.Exists(currentExe)) File.Move(old, currentExe); }
                catch (Exception y) when (y is IOException or UnauthorizedAccessException)
                {
                    return $"обновление не встало, и прежняя версия не вернулась: переименуйте {Path.GetFileName(old)} обратно в {Path.GetFileName(currentExe)}";
                }
                return "новая версия не встала на место: " + x.Message;
            }
        }
    }

    /// <summary>Вернуть прежнюю версию, если перезапуск не удался (новый процесс не стартовал).</summary>
    public static bool Rollback(string currentExe)
    {
        var old = currentExe + OldSuffix;
        try
        {
            if (!File.Exists(old)) return false;
            if (File.Exists(currentExe)) File.Delete(currentExe);
            File.Move(old, currentExe);
            return true;
        }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { return false; }
    }

    /// <summary>После перезапуска: убрать *.old (прежний процесс мог ещё не выйти — несколько попыток) и папку подготовки.</summary>
    public static void Cleanup(string currentExe, string updateDir)
    {
        var old = currentExe + OldSuffix;
        for (var i = 0; i < 10 && File.Exists(old); i++)
        {
            try { File.Delete(old); }
            catch (Exception x) when (x is IOException or UnauthorizedAccessException) { Thread.Sleep(300); }
        }
        try { if (Directory.Exists(updateDir)) Directory.Delete(updateDir, recursive: true); }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { }
    }

    /// <summary>Файл начинается с «MZ» — заголовок программы Windows.</summary>
    private static bool LooksLikeExe(string path)
    {
        using var fs = File.OpenRead(path);
        return fs.ReadByte() == 'M' && fs.ReadByte() == 'Z';
    }

    private static void TryDelete(string p)
    {
        try { File.Delete(p); } catch (Exception x) when (x is IOException or UnauthorizedAccessException) { }
    }
}
