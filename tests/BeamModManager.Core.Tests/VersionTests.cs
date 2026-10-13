using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Tests;

public static class VersionTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Сравнение версий");

        t.Run("1.3 против v1.3.2: на сайте новее", () =>
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.3", "v1.3.2")));

        t.Run("1.0 и 1.0.0 одинаковы", () =>
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.0", "1.0.0")));

        t.Run("на сайте старее", () =>
            Assert.Eq(VersionOrder.Older, VersionComparer.Compare("1.2", "1.1.9")));

        t.Run("числа сравниваются как числа, а не как строки", () =>
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.9", "1.10")));

        t.Run("v2 и v3", () =>
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("v2", "v3")));

        t.Run("«Update 2!» не разбирается", () =>
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare("Update 2!", "3")));

        t.Run("пустая версия не разбирается", () =>
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare(null, "1.0")));

        t.Group("Решение об обновлении");

        t.Run("версия выросла: вероятно обновление", () =>
            Assert.Eq(UpdateStatus.Likely,
                UpdateDecider.Decide(new VersionSnapshot("1.0", 100), new VersionSnapshot("1.1", 100))));

        t.Run("версия та же, размер изменился: возможно", () =>
            Assert.Eq(UpdateStatus.Possible,
                UpdateDecider.Decide(new VersionSnapshot("1.0", 100), new VersionSnapshot("1.0", 120))));

        t.Run("всё то же: обновления нет", () =>
            Assert.Eq(UpdateStatus.None,
                UpdateDecider.Decide(new VersionSnapshot("1.0", 100, "abc"), new VersionSnapshot("1.0", 100, "abc"))));

        t.Run("версия не разобрана, размер изменился: возможно", () =>
            Assert.Eq(UpdateStatus.Possible,
                UpdateDecider.Decide(new VersionSnapshot("Update 2!", 100), new VersionSnapshot("Update 3!", 150))));

        t.Run("версия не разобрана, ничего не изменилось: обновления нет", () =>
            Assert.Eq(UpdateStatus.None,
                UpdateDecider.Decide(new VersionSnapshot("Update 2!", 100), new VersionSnapshot("Update 2!", 100))));

        t.Run("на сайте версия старее: не предлагаем «обновление»", () =>
            Assert.Eq(UpdateStatus.None,
                UpdateDecider.Decide(new VersionSnapshot("2.0", 100), new VersionSnapshot("1.5", 90))));

        t.Run("изменился только хеш описания: возможно", () =>
            Assert.Eq(UpdateStatus.Possible,
                UpdateDecider.Decide(new VersionSnapshot("1.0", null, "aaa"), new VersionSnapshot("1.0", null, "bbb"))));
    }
}
