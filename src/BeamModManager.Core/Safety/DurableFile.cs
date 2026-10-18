using System.Text;

namespace BeamModManager.Core.Safety;

/// <summary>
/// Запись временного файла перед заменой основного (2026-10-18). Обычная запись оставляет данные в кэше Windows, а замена
/// файла записывается сразу: пропало питание между ними — на месте настроек или библиотеки пустой файл, программа
/// откладывает его в .bad и начинает с чистого листа. Здесь данные уходят на диск до замены.
/// </summary>
public static class DurableFile
{
    private static readonly Encoding Utf8 = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false); // как File.WriteAllText

    public static void WriteAllText(string path, string text)
    {
        using var fs = new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.None, 4096, FileOptions.None);
        var bytes = Utf8.GetBytes(text);
        fs.Write(bytes, 0, bytes.Length);
        fs.Flush(flushToDisk: true);
    }
}
