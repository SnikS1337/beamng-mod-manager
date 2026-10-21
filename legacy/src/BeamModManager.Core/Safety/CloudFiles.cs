namespace BeamModManager.Core.Safety;

/// <summary>
/// Файлы «только в облаке» (OneDrive и похожие): на диске лишь заглушка, а любое чтение скачивает файл целиком.
/// Ради картинки на карточке качать мод на сотни мегабайт нельзя — такие архивы пропускаем, пока их не скачают.
/// </summary>
public static class CloudFiles
{
    private const FileAttributes RecallOnOpen = (FileAttributes)0x40000;       // FILE_ATTRIBUTE_RECALL_ON_OPEN
    private const FileAttributes RecallOnDataAccess = (FileAttributes)0x400000; // FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS

    public static bool IsCloudOnly(FileAttributes a) => (a & (FileAttributes.Offline | RecallOnOpen | RecallOnDataAccess)) != 0;

    public static bool IsCloudOnly(string path)
    {
        try { return IsCloudOnly(File.GetAttributes(path)); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or ArgumentException) { return false; }
    }
}
