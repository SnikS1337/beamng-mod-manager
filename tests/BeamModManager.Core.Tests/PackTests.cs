using System.IO.Compression;
using BeamModManager.Core.Packs;

namespace BeamModManager.Core.Tests;

public static class PackTests
{
    private static MemoryStream Zip(params string[] entries)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
            foreach (var e in entries)
            {
                var x = z.CreateEntry(e);
                if (!e.EndsWith('/')) using (var w = new StreamWriter(x.Open())) w.Write("x");
            }
        ms.Position = 0;
        return ms;
    }

    private static ArchiveContent C(params string[] entries) => PackContentReader.Read(Zip(entries));
    private static InstalledContent I(string name, bool on, ArchiveContent c) => new(name, on, c);

    public static void Register(TestRunner t)
    {
        t.Group("Паки модов");

        t.Run("одна машина: не пак", () =>
        {
            var c = C("vehicles/sienna/info.json", "vehicles/sienna/a.dae");
            Assert.Eq(1, c.Vehicles.Count);
            Assert.False(c.IsPack);
        });
        t.Run("несколько машин: пак, имена в нижнем регистре", () =>
        {
            var c = C("vehicles/Sienna/a.json", "vehicles/camry/b.json", "vehicles/Camry/c.json");
            Assert.Eq(2, c.Vehicles.Count);
            Assert.True(c.IsPack, "пак");
            Assert.True(c.Vehicles.Contains("sienna"), "регистр");
        });
        t.Run("обёртка в одну папку и обратные слэши", () =>
        {
            var c = C("MyPack/vehicles\\a\\x.json", "MyPack/vehicles/b/y.json", "MyPack/readme.txt");
            Assert.True(c.Vehicles.SetEquals(new[] { "a", "b" }), string.Join(",", c.Vehicles));
        });
        t.Run("машины и карты считаются вместе", () =>
        {
            var c = C("vehicles/a/i.json", "levels/city/info.json");
            Assert.Eq(1, c.Levels.Count);
            Assert.True(c.IsPack, "машина + карта = набор");
        });
        t.Run("файл прямо в vehicles/ машиной не считается; битый архив пуст", () =>
        {
            Assert.Eq(0, C("vehicles/readme.txt", "ui/x.json").Count);
            Assert.Eq(0, PackContentReader.Read(new MemoryStream(new byte[] { 1, 2, 3 })).Count);
        });
        t.Run("запись пустой папки vehicles/name/ даёт машину", () =>
            Assert.True(C("vehicles/empty/").Vehicles.Contains("empty"), "папка"));

        var pack = C("vehicles/a/i.json", "vehicles/b/i.json", "vehicles/c/i.json");
        var singleA = C("vehicles/a/i.json");
        var singleB = C("vehicles/b/i.json");
        var other = C("vehicles/z/i.json");

        t.Run("пересечения нет: ставим свободно", () =>
        {
            var r = PackOverlapAnalyzer.Analyze(pack, new[] { I("z.zip", true, other) });
            Assert.Eq(PackAdvice.InstallFreely, r.Advice);
            Assert.Eq(0, r.Overlaps.Count);
            Assert.Eq(3, r.New.Count);
        });
        t.Run("часть машин уже стоит отдельно: частичное пересечение, одиночки избыточны", () =>
        {
            var r = PackOverlapAnalyzer.Analyze(pack, new[] { I("a.zip", true, singleA), I("b.zip", true, singleB), I("z.zip", true, other) });
            Assert.Eq(PackAdvice.PartialOverlap, r.Advice);
            Assert.Eq(2, r.AlreadyInstalled.Count);
            Assert.True(r.New.SequenceEqual(new[] { "vehicles/c" }), "новое только c");
            Assert.True(r.RedundantSingles.SequenceEqual(new[] { "a.zip", "b.zip" }), "одиночки целиком внутри пака");
        });
        t.Run("всё из пака уже стоит: пак ничего не добавит", () =>
        {
            var r = PackOverlapAnalyzer.Analyze(pack, new[] { I("a.zip", true, singleA), I("bc.zip", true, C("vehicles/b/i.json", "vehicles/c/i.json")) });
            Assert.Eq(PackAdvice.NothingNew, r.Advice);
            Assert.Eq(0, r.New.Count);
        });
        t.Run("выключенный отдельный мод не считается установленным", () =>
        {
            var r = PackOverlapAnalyzer.Analyze(pack, new[] { I("a.zip", false, singleA) });
            Assert.Eq(0, r.AlreadyInstalled.Count);
            Assert.Eq(1, r.Overlaps.Count);
            Assert.Eq(0, r.RedundantSingles.Count);
            Assert.Eq(PackAdvice.InstallFreely, r.Advice);
        });
        t.Run("мод больше пака (есть лишнее) не помечается избыточным", () =>
        {
            var r = PackOverlapAnalyzer.Analyze(pack, new[] { I("ax.zip", true, C("vehicles/a/i.json", "vehicles/x/i.json")) });
            Assert.False(r.Overlaps.Single().FullyInsidePack, "внутри пака не целиком");
            Assert.Eq(0, r.RedundantSingles.Count);
        });
        t.Run("дубли среди включённых модов находятся, выключенные не учитываются", () =>
        {
            var d = PackOverlapAnalyzer.FindDuplicates(new[]
            {
                I("pack.zip", true, pack), I("a.zip", true, singleA), I("b.zip", false, singleB), I("z.zip", true, other),
            });
            Assert.Eq(1, d.Count);
            Assert.True(d["vehicles/a"].SequenceEqual(new[] { "a.zip", "pack.zip" }), "кто дублирует");
        });
    }
}
