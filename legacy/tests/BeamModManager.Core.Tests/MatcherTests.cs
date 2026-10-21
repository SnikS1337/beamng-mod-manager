using BeamModManager.Core.Matching;
using BeamModManager.Core.Models;

namespace BeamModManager.Core.Tests;

public static class MatcherTests
{
    private const long MB = 1024 * 1024;

    private static Listing L(string source, string title) =>
        new(source, new Uri($"https://{source}.example/{Guid.NewGuid():N}"), "id", title);

    public static void Register(TestRunner t)
    {
        t.Group("Объединение дублей");

        // Реальная пара с двух сайтов: Modland и ModsGaming.
        var modlandSienna = L("modland", "Toyota Sienna 2011-2017 2.2") with { SizeBytes = 61 * MB };
        var modsGamingSienna = L("modsgaming", "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE");

        t.Run("одно название без других признаков: только «возможно», не автообъединение", () =>
        {
            var r = DuplicateMatcher.Compare(modlandSienna, modsGamingSienna);
            Assert.Eq(MatchVerdict.Maybe, r.Verdict);
            Assert.True(r.Score < DuplicateMatcher.SameThreshold, $"оценка {r.Score}");
        });

        t.Run("название, автор и размер совпадают: одно и то же", () =>
        {
            var a = modlandSienna with { Author = "Some Author", SizeBytes = 61 * MB };
            var b = modsGamingSienna with { Author = "some author", SizeBytes = 61 * MB + 100_000 };
            Assert.Eq(MatchVerdict.Same, DuplicateMatcher.Compare(a, b).Verdict);
        });

        t.Run("один файл на ModsFire: одно и то же, даже при разных названиях", () =>
        {
            var a = L("modland", "Some Car") with
            {
                Offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/9nDwN00BJ04Ef55"), "9nDwN00BJ04Ef55") },
            };
            var b = L("modsgaming", "Completely Different Title") with
            {
                Offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/download/9nDwN00BJ04Ef55/0fa51")) },
            };
            var r = DuplicateMatcher.Compare(a, b);
            Assert.Eq(MatchVerdict.Same, r.Verdict);
            Assert.Eq(1.0, r.Score);
        });

        t.Run("разные файлы на ModsFire сами по себе ничего не значат", () =>
        {
            var a = L("modland", "Toyota AE86 RLA") with
            {
                Offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/AAAAAAAAAAAA1111"), "AAAAAAAAAAAA1111") },
            };
            var b = L("modsgaming", "Toyota Sienna 2011-2017") with
            {
                Offers = new[] { new Offer(OfferKind.ExternalHost, "modsfire", new Uri("https://modsfire.com/BBBBBBBBBBBB2222"), "BBBBBBBBBBBB2222") },
            };
            Assert.Eq(MatchVerdict.Different, DuplicateMatcher.Compare(a, b).Verdict);
        });

        t.Run("разные названия: разные моды", () =>
            Assert.Eq(MatchVerdict.Different,
                DuplicateMatcher.Compare(L("a", "Toyota AE86 RLA v1.0.7"), L("b", "Toyota Sienna 2011-2017")).Verdict));

        t.Run("Remaster и обычная версия не склеиваются", () =>
            Assert.Eq(MatchVerdict.Different,
                DuplicateMatcher.Compare(L("a", "Honda Accord 2017 Remaster"), L("b", "Honda Accord 2017")).Verdict));

        t.Run("одинаковое название у разных авторов: разные моды", () =>
        {
            var a = L("a", "Toyota AE86") with { Author = "Alice", SizeBytes = 50 * MB };
            var b = L("b", "Toyota AE86") with { Author = "Bob", SizeBytes = 50 * MB };
            Assert.Eq(MatchVerdict.Different, DuplicateMatcher.Compare(a, b).Verdict);
        });

        t.Run("сильно разный размер не позволяет автообъединение", () =>
        {
            var a = L("a", "Toyota AE86") with { Author = "Alice", SizeBytes = 60 * MB };
            var b = L("b", "Toyota AE86") with { Author = "alice", SizeBytes = 80 * MB };
            var r = DuplicateMatcher.Compare(a, b);
            Assert.Eq(MatchVerdict.Maybe, r.Verdict);
            Assert.True(r.Score <= 0.75, $"оценка {r.Score}");
        });

        t.Run("совпадающее превью повышает уверенность", () =>
        {
            var a = L("a", "Toyota AE86") with { PreviewHash = 0xF0F0F0F0F0F0F0F0UL };
            var b = L("b", "Toyota AE86") with { PreviewHash = 0xF0F0F0F0F0F0F0F0UL };
            var withHash = DuplicateMatcher.Compare(a, b).Score;
            var without = DuplicateMatcher.Compare(L("a", "Toyota AE86"), L("b", "Toyota AE86")).Score;
            Assert.True(withHash > without, $"{withHash} должно быть больше {without}");
        });
    }
}
