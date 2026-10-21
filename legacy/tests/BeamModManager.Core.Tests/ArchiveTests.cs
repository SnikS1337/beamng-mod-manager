using System.IO.Compression;
using System.Text;
using BeamModManager.Core.Safety;

namespace BeamModManager.Core.Tests;

public static class ArchiveTests
{
    private static MemoryStream Zip(params (string Name, int Size)[] entries)
    {
        var ms = new MemoryStream();
        using (var zip = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
        {
            foreach (var (name, size) in entries)
            {
                var e = zip.CreateEntry(name, CompressionLevel.Optimal);
                if (name.EndsWith('/')) continue;
                using var s = e.Open();
                s.Write(new byte[size]);
            }
        }
        ms.Position = 0;
        return ms;
    }

    private static bool Has(InspectionResult r, string code) => r.Findings.Any(f => f.Code == code);

    public static void Register(TestRunner t)
    {
        t.Group("Проверка архивов");

        t.Run("обычный мод: без блокировок и предупреждений", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("vehicles/foo/info.json", 50), ("vehicles/foo/foo.jbeam", 100), ("art/x.dds", 200)));
            Assert.False(r.IsBlocked);
            Assert.False(r.HasWarnings);
            Assert.Eq(3, r.EntryCount);
        });

        t.Run("zip-slip: ../", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("vehicles/a.jbeam", 10), ("../evil.lua", 10)));
            Assert.True(r.IsBlocked);
            Assert.True(Has(r, "zip-slip"));
        });

        t.Run("zip-slip: обратные слэши и абсолютный путь", () =>
        {
            Assert.True(Has(ArchiveInspector.Inspect(Zip(("..\\evil.lua", 10))), "zip-slip"));
            Assert.True(Has(ArchiveInspector.Inspect(Zip(("/etc/evil.lua", 10))), "zip-slip"));
            Assert.True(Has(ArchiveInspector.Inspect(Zip(("C:/evil.lua", 10))), "zip-slip"));
        });

        t.Run("исполняемый файл блокируется", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("vehicles/x/run.exe", 10), ("vehicles/x/a.jbeam", 10)));
            Assert.True(r.IsBlocked);
            Assert.True(Has(r, "executable"));
        });

        t.Run("скрипты .js внутри ui не блокируются", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("ui/modules/app/app.js", 10), ("vehicles/x/a.jbeam", 10)));
            Assert.False(r.IsBlocked);
        });

        t.Run("лишняя корневая папка: предупреждение, не блокировка", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("mymod/vehicles/a.jbeam", 10), ("mymod/info.json", 10)));
            Assert.False(r.IsBlocked);
            Assert.True(Has(r, "nested-root"));
        });

        t.Run("нетипичная структура: предупреждение", () =>
        {
            var r = ArchiveInspector.Inspect(Zip(("readme.txt", 10), ("foo/bar.txt", 10)));
            Assert.False(r.IsBlocked);
            Assert.True(Has(r, "unusual-structure"));
        });

        t.Run("не ZIP: блокировка", () =>
        {
            var r = ArchiveInspector.Inspect(new MemoryStream(Encoding.UTF8.GetBytes("это не архив")));
            Assert.True(r.IsBlocked);
            Assert.True(Has(r, "not-zip"));
        });

        t.Run("слишком много файлов", () =>
        {
            var r = ArchiveInspector.Inspect(
                Zip(("vehicles/a", 1), ("vehicles/b", 1), ("vehicles/c", 1)), new ArchiveLimits { MaxEntries = 2 });
            Assert.True(Has(r, "too-many-entries"));
        });

        t.Run("слишком большой распакованный размер", () =>
        {
            var r = ArchiveInspector.Inspect(
                Zip(("vehicles/a", 5000)), new ArchiveLimits { MaxTotalUncompressedBytes = 1000 });
            Assert.True(Has(r, "too-large"));
        });

        t.Run("архивная бомба: огромная степень сжатия", () =>
        {
            var r = ArchiveInspector.Inspect(
                Zip(("vehicles/zeros.bin", 1_000_000)),
                new ArchiveLimits { RatioCheckMinBytes = 1000, MaxCompressionRatio = 10 });
            Assert.True(Has(r, "compression-bomb"));
        });

        t.Run("пустой архив: предупреждение", () =>
        {
            var r = ArchiveInspector.Inspect(Zip());
            Assert.True(Has(r, "empty"));
        });
    }
}
