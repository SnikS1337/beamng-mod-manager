using System.IO.Compression;
using BeamModManager.Core.Previews;

namespace BeamModManager.Core.Tests;

public static class PreviewTests
{
    internal static byte[] Png(int w, int h)
    {
        var b = new byte[64];
        new byte[] { 0x89, (byte)'P', (byte)'N', (byte)'G', 13, 10, 26, 10, 0, 0, 0, 13, (byte)'I', (byte)'H', (byte)'D', (byte)'R' }.CopyTo(b, 0);
        b[16] = (byte)(w >> 24); b[17] = (byte)(w >> 16); b[18] = (byte)(w >> 8); b[19] = (byte)w;
        b[20] = (byte)(h >> 24); b[21] = (byte)(h >> 16); b[22] = (byte)(h >> 8); b[23] = (byte)h;
        return b;
    }

    // JPEG с блоком EXIF перед кадром, как у фотографий с камер и из игры.
    internal static byte[] Jpg(int w, int h, int exif = 300)
    {
        var ms = new MemoryStream();
        ms.Write(new byte[] { 0xFF, 0xD8, 0xFF, 0xE1, (byte)((exif + 2) >> 8), (byte)(exif + 2) });
        ms.Write(new byte[exif]);
        ms.Write(new byte[] { 0xFF, 0xDB, 0, 4, 0, 0 });
        ms.Write(new byte[] { 0xFF, 0xC0, 0, 17, 8, (byte)(h >> 8), (byte)h, (byte)(w >> 8), (byte)w, 3 });
        ms.Write(new byte[9]);
        ms.Write(new byte[] { 0xFF, 0xD9 });
        return ms.ToArray();
    }

    private static MemoryStream Zip(params (string Name, byte[] Data)[] files)
    {
        var ms = new MemoryStream();
        using (var z = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
            foreach (var (n, d) in files)
            {
                using var s = z.CreateEntry(n).Open();
                s.Write(d);
            }
        ms.Position = 0;
        return ms;
    }

    private static readonly byte[] Txt = "x"u8.ToArray();

    public static void Register(TestRunner t)
    {
        t.Group("Картинки модов из архива");

        t.Run("BMW M2 (настоящее оглавление): default.jpg, а не картинки конфигураций и не bake-текстуры", () =>
        {
            var p = PreviewPicker.Pick(Zip(
                ("vehicles/infm2/ADRO Style 2.jpg", Jpg(1280, 720)), ("vehicles/infm2/ADRO Style 2.pc", Txt),
                ("vehicles/infm2/ADRO.jpg", Jpg(1280, 720)), ("vehicles/infm2/ADRO.pc", Txt),
                ("vehicles/infm2/bakes/infm2_bake_brake.png", Png(1024, 1024)),
                ("vehicles/infm2/default.jpg", Jpg(1920, 1080)),
                ("vehicles/infm2/infm2.jbeam", Txt), ("art/sound/x.wav", Txt)));
            Assert.Eq(PreviewSource.VehicleDefault, p.Source);
            Assert.Eq("vehicles/infm2/default.jpg", p.Entry);
            Assert.Eq((1920, 1080), (p.Width, p.Height));
            Assert.Eq(ArchiveContent.Vehicle, p.Content);
        });

        t.Run("W164 (default.jpg нет): картинка конфигурации рядом с .pc, текстуры из TXT/ мимо", () =>
        {
            var p = PreviewPicker.Pick(Zip(
                ("vehicles/ml55/TXT/baba.png", Png(2048, 2048)),
                ("vehicles/ml55/ML63Stil.jpg", Jpg(800, 450)), ("vehicles/ml55/ML63Stil.pc", Txt),
                ("vehicles/ml55/ML300CDI.jpg", Jpg(800, 450)), ("vehicles/ml55/ML300CDI.pc", Txt),
                ("vehicles/ml55/random.jpg", Jpg(4000, 3000))));
            Assert.Eq(PreviewSource.VehicleConfig, p.Source);
            Assert.Eq("vehicles/ml55/ML300CDI.jpg", p.Entry);
        });

        t.Run("Mudstone Valley (карта из репозитория): картинка сайта из mod_info, а не облака и блики уровня", () =>
        {
            var p = PreviewPicker.Pick(Zip(
                ("levels/mudstone_valley/art/skies/clouds_01.png", Png(2048, 2048)),
                ("levels/mudstone_valley/art/lights/corona.png", Png(512, 512)),
                ("levels/mudstone_valley/info.json", Txt),
                ("mod_info/M7P2C4JGW/info.json", Txt),
                ("mod_info/M7P2C4JGW/icon.jpg", Jpg(256, 256)),
                ("mod_info/M7P2C4JGW/images/1.jpg", Jpg(1280, 720))));
            Assert.Eq(PreviewSource.ModInfo, p.Source);
            Assert.Eq("mod_info/M7P2C4JGW/images/1.jpg", p.Entry);
            Assert.Eq(ArchiveContent.Level, p.Content);
        });

        t.Run("карта без mod_info: превью из папки уровня, текстуры из art/ мимо", () =>
        {
            var p = PreviewPicker.Pick(Zip(
                ("levels/canyon/art/road/t_road_wet_d.png", Png(2048, 2048)),
                ("levels/canyon/canyon_preview1.jpg", Jpg(1920, 1080)),
                ("levels/canyon/main.level.json", Txt)));
            Assert.Eq(PreviewSource.LevelPreview, p.Source);
            Assert.Eq("levels/canyon/canyon_preview1.jpg", p.Entry);
        });

        t.Run("Traffic Reborn (картинок нет совсем): заглушка, тип по содержимому — скрипт", () =>
        {
            var p = PreviewPicker.Pick(Zip(("lua/ge/extensions/traffic.lua", Txt), ("scripts/x/modScript.lua", Txt),
                ("settings/x.json", Txt), ("info.json", Txt), ("art/x.dds", Txt)));
            Assert.False(p.Found, "картинки нет");
            Assert.Eq(PreviewSource.None, p.Source);
            Assert.Eq(ArchiveContent.Script, p.Content);
            Assert.True(p.Warning is null, "это не ошибка");
        });

        t.Run("скриптовый мод со значком приложения: берётся app.png", () =>
        {
            var p = PreviewPicker.Pick(Zip(("ui/modules/apps/Speedo/app.js", Txt), ("ui/modules/apps/Speedo/app.png", Png(128, 128))));
            Assert.Eq(PreviewSource.AppIcon, p.Source);
        });

        t.Run("картинка в корне скриптового мода (не меньше 200 px) подходит, мелкие и текстуры нет", () =>
        {
            var p = PreviewPicker.Pick(Zip(("lua/x.lua", Txt), ("icon_small.png", Png(64, 64)),
                ("textures/big.png", Png(2048, 2048)), ("cover.png", Png(900, 500))));
            Assert.Eq(PreviewSource.OtherImage, p.Source);
            Assert.Eq("cover.png", p.Entry);
        });

        t.Run("битая картинка (не PNG/JPEG внутри) пропускается, берётся следующая", () =>
        {
            var p = PreviewPicker.Pick(Zip(("vehicles/a/default.jpg", Txt), ("vehicles/a/sport.jpg", Jpg(640, 360)), ("vehicles/a/sport.pc", Txt)));
            Assert.Eq("vehicles/a/sport.jpg", p.Entry);
        });

        t.Run("повреждённый архив и не-архив: без исключения, с пояснением", () =>
        {
            var p = PreviewPicker.Pick(new MemoryStream("PK\u0003\u0004 обрезано"u8.ToArray()));
            Assert.False(p.Found, "нет картинки");
            Assert.True(p.Warning is not null, "есть пояснение");
            var missing = PreviewPicker.Pick(Path.Combine(Path.GetTempPath(), "нет-такого-" + Guid.NewGuid() + ".zip"));
            Assert.True(missing.Warning is not null, "файла нет");
        });

        t.Run("обратная косая черта и ведущий слэш в именах записей не мешают", () =>
        {
            var p = PreviewPicker.Pick(Zip(("/vehicles\\b\\default.png", Png(512, 288))));
            Assert.Eq(PreviewSource.VehicleDefault, p.Source);
        });

        t.Run("Extract достаёт ровно байты картинки из файла на диске", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-pv-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            var zip = Path.Combine(dir, "car.zip");
            var img = Jpg(1280, 720);
            using (var z = Zip(("vehicles/car/default.jpg", img))) File.WriteAllBytes(zip, z.ToArray());
            var p = PreviewPicker.Pick(zip);
            Assert.True(PreviewPicker.Extract(zip, p.Entry!)!.SequenceEqual(img), "те же байты");
            Assert.True(PreviewPicker.Extract(zip, "нет/такой.jpg") is null, "нет записи");
        });

        t.Run("заголовки: PNG, JPEG с EXIF, мусор", () =>
        {
            Assert.Eq((300, 200), ImageHeader.Parse(Png(300, 200))!.Value);
            Assert.Eq((4000, 3000), ImageHeader.Parse(Jpg(4000, 3000, 60000))!.Value);
            Assert.True(ImageHeader.Parse(new byte[] { 0xFF, 0xD8, 0xFF }) is null, "обрезанный JPEG");
            Assert.True(ImageHeader.Parse("GIF89a"u8) is null, "не поддерживаем");
        });

        t.Run("текстуры узнаются по папке и имени", () =>
        {
            foreach (var x in new[] { "art/a.png", "vehicles/a/TXT/b.png", "x/t_rocks_eca_nm.normal.png", "v/a/bakes/z.png", "v/a/Meshpart3_diff.png", "v/a/paint/cristal.png" })
                Assert.True(PreviewPicker.LooksLikeTexture(x), x);
            foreach (var x in new[] { "vehicles/a/default.jpg", "levels/c/c_preview.jpg", "cover.png", "vehicles/LXRS14/nissansilvias14ultrared713653.jpg" })
                Assert.False(PreviewPicker.LooksLikeTexture(x), x);
        });

        t.Group("Кэш картинок");

        t.Run("картинка и метка «нет картинки» запоминаются; изменённый архив — новый ключ", () =>
        {
            var c = new PreviewCache(Path.Combine(Path.GetTempPath(), "bmm-pc-" + Guid.NewGuid().ToString("N")));
            var a = new ArchiveStamp("Car.zip", 100, new DateTime(2026, 10, 15, 10, 0, 0, DateTimeKind.Utc));
            Assert.Eq(CacheState.Missing, c.State(a));
            c.PutImage(a, new byte[] { 1, 2, 3 });
            Assert.Eq(CacheState.Image, c.State(a));
            Assert.Eq(CacheState.Image, c.State(a with { FileName = "car.zip" }));
            Assert.Eq(CacheState.Missing, c.State(a with { Size = 101 }));
            Assert.Eq(CacheState.Missing, c.State(a with { LastWriteUtc = a.LastWriteUtc.AddSeconds(1) }));
            c.PutNoImage(a);
            Assert.Eq(CacheState.NoImage, c.State(a));
            Assert.False(File.Exists(c.ImagePath(a)), "старая картинка убрана");
        });

        t.Run("уборка оставляет только живые архивы и чужие файлы не трогает", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-pc-" + Guid.NewGuid().ToString("N"));
            var c = new PreviewCache(dir);
            var live = new ArchiveStamp("a.zip", 1, DateTime.UnixEpoch);
            var gone = new ArchiveStamp("b.zip", 1, DateTime.UnixEpoch);
            c.PutImage(live, new byte[] { 1 });
            c.PutImage(gone, new byte[] { 1 });
            File.WriteAllText(Path.Combine(dir, "readme.txt"), "");
            File.WriteAllText(Path.Combine(dir, "x.jpg.123.tmp"), "");
            Assert.Eq(2, c.Prune(new[] { live }));
            Assert.Eq(CacheState.Image, c.State(live));
            Assert.Eq(CacheState.Missing, c.State(gone));
            Assert.True(File.Exists(Path.Combine(dir, "readme.txt")), "чужой файл цел");
        });
    }
}

public static class PreviewLoaderTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Картинки в фоне");

        t.Run("первый раз читает архив и уменьшает, второй — только кэш; без картинок — метка", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-pl-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            var car = Path.Combine(dir, "car.zip");
            var script = Path.Combine(dir, "script.zip");
            using (var z = new ZipArchive(File.Create(car), ZipArchiveMode.Create))
                using (var s = z.CreateEntry("vehicles/car/default.jpg").Open()) s.Write(PreviewTests.Jpg(1920, 1080));
            using (var z = new ZipArchive(File.Create(script), ZipArchiveMode.Create))
                using (var s = z.CreateEntry("lua/x.lua").Open()) s.Write("x"u8);

            var shrinks = 0;
            var loader = new BeamModManager.Ui.Previews.PreviewLoader(new PreviewCache(Path.Combine(dir, "cache")), b => { shrinks++; return new byte[] { 9 }; });
            var p = loader.Ensure(car);
            Assert.True(p is not null && File.ReadAllBytes(p).SequenceEqual(new byte[] { 9 }), "в кэше уменьшенная");
            Assert.Eq(p, loader.Ensure(car));
            Assert.Eq(1, shrinks);
            Assert.True(loader.Ensure(script) is null, "картинок нет");
            Assert.True(loader.Ensure(Path.Combine(dir, "нет.zip")) is null, "файла нет");

            var ready = new List<string>();
            loader.FillAsync(new[] { ("car", car), ("script", script) }, (k, _) => { lock (ready) ready.Add(k); }).Wait();
            Assert.True(ready.SequenceEqual(new[] { "car" }), "сообщили только о найденной");
        });

        t.Run("картинка не декодировалась: метку «нет картинки» не ставим, попробуем снова", () =>
        {
            var dir = Path.Combine(Path.GetTempPath(), "bmm-pl-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            var car = Path.Combine(dir, "car.zip");
            using (var z = new ZipArchive(File.Create(car), ZipArchiveMode.Create))
                using (var s = z.CreateEntry("vehicles/car/default.png").Open()) s.Write(PreviewTests.Png(800, 600));
            var cache = new PreviewCache(Path.Combine(dir, "cache"));
            Assert.True(new BeamModManager.Ui.Previews.PreviewLoader(cache, _ => null).Ensure(car) is null, "не вышло");
            Assert.Eq(CacheState.Missing, cache.State(ArchiveStamp.Of(car)));
        });
    }
}
