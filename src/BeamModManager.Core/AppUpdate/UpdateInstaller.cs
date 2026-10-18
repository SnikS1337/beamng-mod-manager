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
    public const string NewSuffix = ".new";
    public const string BadSuffix = ".bad";

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
    /// Поставить новый exe на место текущего: <see cref="Stage"/>, затем <see cref="Commit"/>. Программа делает эти шаги
    /// в разное время: подготовку — сразу после загрузки, подмену — уже закрыв окно (см. <see cref="Commit"/>).
    /// </summary>
    /// <param name="move">Переименование (для проверок: подставить сбой). По умолчанию File.Move.</param>
    public static string? Swap(string currentExe, string newExe, Action<string, string>? move = null) =>
        Stage(currentExe, newExe) ?? Commit(currentExe, move);

    /// <summary>
    /// Подготовка: новый exe копируется рядом с текущим («*.new»). Папка обновления лежит в профиле, а программа может
    /// быть на другом диске, и перенос между дисками — это долгое копирование; здесь оно делается заранее, текущий exe
    /// не трогается. Здесь же видно, что в папку программы нельзя писать: ошибка — понятная причина.
    /// </summary>
    public static string? Stage(string currentExe, string newExe)
    {
        var staged = currentExe + NewSuffix;
        if (!File.Exists(newExe)) return "подготовленное обновление пропало";
        if (!File.Exists(currentExe)) return "программа не найдена на прежнем месте";
        var copied = Retry(() =>
        {
            TryDeleteFile(staged);
            File.Copy(newExe, staged, overwrite: true);
        });
        if (copied is UnauthorizedAccessException) { TryDelete(staged); return NoRights; }
        if (copied is not null) { TryDelete(staged); return "не удалось положить новую версию рядом с программой: " + copied.Message; }
        // антивирус проверяет свежий файл и держит его открытым: ждём, пока отпустит, до того как отодвигать текущий
        if (Retry(() => { using var _ = new FileStream(staged, FileMode.Open, FileAccess.Read, FileShare.None); }, attempts: 10) is { } held)
        {
            TryDelete(staged);
            return "новую версию держит другая программа (антивирус?): " + held.Message;
        }
        return null;
    }

    /// <summary>
    /// Подмена: два быстрых переименования на одном диске — текущий → «*.old», «*.new» → на его место; если второе не
    /// удалось, текущий возвращается. Программа зовёт это, когда окно уже закрыто и всё сохранено: собранная в один файл
    /// программа догружает свои части из exe по его имени, и после подмены под этим именем уже другая версия.
    /// </summary>
    public static string? Commit(string currentExe, Action<string, string>? move = null)
    {
        move ??= (a, b) => File.Move(a, b);
        var old = currentExe + OldSuffix;
        var staged = currentExe + NewSuffix;
        if (!File.Exists(staged)) return "подготовленное обновление пропало";

        try { if (File.Exists(old)) File.Delete(old); } // остался с прошлого обновления
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { TryDelete(staged); return "не удалось убрать старую копию: " + x.Message; }

        try { move(currentExe, old); }
        catch (UnauthorizedAccessException) { TryDelete(staged); return NoRights; }
        catch (IOException x) { TryDelete(staged); return "не удалось отодвинуть текущую версию: " + x.Message; }

        if (Retry(() => move(staged, currentExe)) is not { } failed) return null;

        // откат: прежняя версия на место
        try { if (!File.Exists(currentExe)) move(old, currentExe); }
        catch (Exception y) when (y is IOException or UnauthorizedAccessException)
        {
            return $"обновление не установилось, и прежняя версия не вернулась: переименуйте {Path.GetFileName(old)} обратно в {Path.GetFileName(currentExe)}";
        }
        TryDelete(staged);
        return "новая версия не установилась: " + failed.Message;
    }

    private const string NoRights = "нет прав менять файлы в папке программы. Перенесите её, например, на рабочий стол";

    /// <summary>До <paramref name="attempts"/> попыток с растущей паузой (антивирус, индексатор). null — получилось.</summary>
    private static Exception? Retry(Action a, int attempts = 4)
    {
        for (var i = 0; ; i++)
        {
            try { a(); return null; }
            catch (Exception x) when (x is IOException or UnauthorizedAccessException)
            {
                if (i >= attempts - 1) return x;
                Thread.Sleep(Math.Min(1000, 150 * (i + 1)));
            }
        }
    }

    /// <summary>
    /// Вернуть прежнюю версию, когда новая не запустилась и уже закрылась (зовёт прежний процесс): новый exe удаляется,
    /// «*.old» встаёт на место. Только что закрывшийся файл может ещё держать антивирус — несколько попыток.
    /// </summary>
    public static bool Rollback(string currentExe)
    {
        var old = currentExe + OldSuffix;
        if (!File.Exists(old)) return false;
        return Retry(() =>
        {
            if (File.Exists(currentExe)) File.Delete(currentExe);
            File.Move(old, currentExe);
        }, attempts: 6) is null;
    }

    /// <summary>
    /// Вернуть прежнюю версию из самой новой, которая не смогла запуститься (зовёт новый процесс). Работающий exe нельзя
    /// удалить, но можно переименовать: он уходит в «*.bad», «*.old» встаёт на место. Сбой второго шага — всё как было.
    /// </summary>
    public static bool RollbackFromRunning(string currentExe)
    {
        var old = currentExe + OldSuffix;
        var bad = currentExe + BadSuffix;
        if (!File.Exists(old) || !File.Exists(currentExe)) return false;
        try
        {
            if (File.Exists(bad)) File.Delete(bad);
            File.Move(currentExe, bad);
        }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { return false; }
        try { File.Move(old, currentExe); return true; }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException)
        {
            try { File.Move(bad, currentExe); } catch (Exception y) when (y is IOException or UnauthorizedAccessException) { }
            return false;
        }
    }

    /// <summary>
    /// Когда версия уже показала окно: убрать «*.old», «*.new», «*.bad» (прежний процесс мог ещё не выйти — несколько
    /// попыток) и папку подготовки. До этого «*.old» нужен: из него возвращаем прежнюю, если новая не запустилась.
    /// </summary>
    public static void Cleanup(string currentExe, string updateDir)
    {
        foreach (var leftover in new[] { currentExe + OldSuffix, currentExe + NewSuffix, currentExe + BadSuffix })
            for (var i = 0; i < 10 && File.Exists(leftover); i++)
            {
                try { File.Delete(leftover); }
                catch (Exception x) when (x is IOException or UnauthorizedAccessException) { Thread.Sleep(300); }
            }
        try { if (Directory.Exists(updateDir)) Directory.Delete(updateDir, recursive: true); }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { }
    }

    /// <summary>Версия, которая не запустилась на этом компьютере: её больше не предлагаем (ждём следующую).</summary>
    public static Version? ReadSkipped(string path)
    {
        try { return File.Exists(path) ? AppVersion.Parse(File.ReadAllText(path)) : null; }
        catch (Exception x) when (x is IOException or UnauthorizedAccessException) { return null; }
    }

    public static void WriteSkipped(string path, Version v)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            File.WriteAllText(path, AppVersion.Display(v));
        }
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

    private static void TryDeleteFile(string p)
    {
        if (File.Exists(p)) File.Delete(p);
    }
}
