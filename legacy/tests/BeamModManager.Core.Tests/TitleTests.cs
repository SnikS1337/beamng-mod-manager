using BeamModManager.Core.Text;

namespace BeamModManager.Core.Tests;

public static class TitleTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Нормализация названий");

        t.Run("ModsGaming: служебные слова и версии убираются", () =>
            Assert.Eq("toyota sienna 2011 2017",
                TitleNormalizer.Normalize("МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE")));

        t.Run("Modland: версия без v в конце убирается", () =>
            Assert.Eq("toyota sienna 2011 2017", TitleNormalizer.Normalize("Toyota Sienna 2011-2017 2.2")));

        t.Run("[PACK] и Update 2! убираются", () =>
            Assert.Eq("bmw m3 e92", TitleNormalizer.Normalize("BMW M3 E92 [PACK] Update 2!")));

        t.Run("год в скобках сохраняется", () =>
            Assert.Eq("autobello aria 1972",
                TitleNormalizer.Normalize("МОД Autobello Aria (1972) v1.0.4 (0.39.x) ДЛЯ BEAMNG.DRIVE")));

        t.Run("метка [PAID] убирается", () =>
            Assert.Eq("daxworx plate pack", TitleNormalizer.Normalize("Daxworx Plate Pack [PAID]")));

        t.Run("пустая строка даёт пустой результат", () =>
            Assert.Eq("", TitleNormalizer.Normalize("   ")));

        t.Run("Remaster отличает мод: сходство 0.75", () =>
            Assert.Eq(0.75, TitleNormalizer.Similarity("Honda Accord 2017 Remaster v1.0", "Honda Accord 2017")));

        t.Run("одинаковые названия с разной оболочкой: сходство 1", () =>
            Assert.Eq(1.0, TitleNormalizer.Similarity(
                "МОД Nissan Kicks 2021-2024 v1.1 (0.39.x) ДЛЯ BEAMNG.DRIVE", "2021-2024 Nissan Kicks 1.1")));

        t.Group("Разбор заголовков");

        t.Run("ModsGaming: Toyota AE86 RLA", () =>
        {
            var p = TitleParser.Parse("МОД Toyota AE86 RLA v1.0.7 (0.39.x) ДЛЯ BEAMNG.DRIVE");
            Assert.Eq("Toyota AE86 RLA", p.Name);
            Assert.Eq("1.0.7", p.ModVersion);
            Assert.Eq("0.39.x", p.GameVersion);
            Assert.False(p.HasPaidMarker);
        });

        t.Run("ModsGaming: Remaster остаётся в названии", () =>
        {
            var p = TitleParser.Parse("МОД Honda Accord 2017 v1.0 Remaster (0.39.x) ДЛЯ BEAMNG.DRIVE");
            Assert.Eq("Honda Accord 2017 Remaster", p.Name);
            Assert.Eq("1.0", p.ModVersion);
            Assert.Eq("0.39.x", p.GameVersion);
        });

        t.Run("год в скобках не принимается за версию игры", () =>
        {
            var p = TitleParser.Parse("МОД Autobello Aria (1972) v1.0.4 (0.39.x) ДЛЯ BEAMNG.DRIVE");
            Assert.Eq("Autobello Aria (1972)", p.Name);
            Assert.Eq("1.0.4", p.ModVersion);
            Assert.Eq("0.39.x", p.GameVersion);
        });

        t.Run("кириллическое название", () =>
        {
            var p = TitleParser.Parse("МОД ВАЗ 2106 v6.0 (0.38.x) ДЛЯ BEAMNG.DRIVE");
            Assert.Eq("ВАЗ 2106", p.Name);
            Assert.Eq("6.0", p.ModVersion);
            Assert.Eq("0.38.x", p.GameVersion);
        });

        t.Run("версия в конце без v", () =>
        {
            var p = TitleParser.Parse("Toyota Sienna 2011-2017 2.2");
            Assert.Eq("Toyota Sienna 2011-2017", p.Name);
            Assert.Eq("2.2", p.ModVersion);
        });

        t.Run("метка PAID определяется и убирается из названия", () =>
        {
            var p = TitleParser.Parse("Daxworx Plate Pack [PAID]");
            Assert.True(p.HasPaidMarker);
            Assert.Eq("Daxworx Plate Pack", p.Name);
        });

        t.Run("V8 в названии не считается версией", () =>
        {
            var p = TitleParser.Parse("Ford V8 Coupe v1.2");
            Assert.Eq("Ford V8 Coupe", p.Name);
            Assert.Eq("1.2", p.ModVersion);
        });
    }
}
