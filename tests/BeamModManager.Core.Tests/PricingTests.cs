using BeamModManager.Core.Models;
using BeamModManager.Core.Pricing;

namespace BeamModManager.Core.Tests;

public static class PricingTests
{
    private static Uri U(string s) => new(s);

    public static void Register(TestRunner t)
    {
        t.Group("Определение платного");

        t.Run("метка PAID в названии", () =>
            Assert.Eq(PriceStatus.Paid, PaidDetector.Detect("Daxworx Plate Pack PAID", null, null)));

        t.Run("метка [Premium] в названии", () =>
            Assert.Eq(PriceStatus.Paid, PaidDetector.Detect("Some Car [Premium]", null, null)));

        t.Run("тег Premium", () =>
            Assert.Eq(PriceStatus.Paid, PaidDetector.Detect("Some Car", new[] { "Premium" }, null)));

        t.Run("только ссылка на Patreon", () =>
            Assert.Eq(PriceStatus.Paid,
                PaidDetector.Detect("Some Car", null, new[] { U("https://www.patreon.com/someauthor") })));

        t.Run("ModsFire плюс Patreon: Patreon считается поддержкой автора", () =>
            Assert.Eq(PriceStatus.Free, PaidDetector.Detect("Some Car", null,
                new[] { U("https://www.patreon.com/someauthor"), U("https://modsfire.com/9nDwN00BJ04Ef55") })));

        t.Run("ссылка на ModsFire без меток: бесплатно", () =>
            Assert.Eq(PriceStatus.Free,
                PaidDetector.Detect("Some Car", null, new[] { U("https://modsfire.com/9nDwN00BJ04Ef55") })));

        t.Run("нет ссылок и меток: неизвестно", () =>
            Assert.Eq(PriceStatus.Unknown, PaidDetector.Detect("Some Car", null, null)));

        t.Run("неизвестный хост: неизвестно", () =>
            Assert.Eq(PriceStatus.Unknown,
                PaidDetector.Detect("Some Car", null, new[] { U("https://example.org/file.zip") })));

        t.Run("Limited Premium в названии комплектации не делает мод платным", () =>
            Assert.Eq(PriceStatus.Unknown, PaidDetector.Detect("Toyota Sienna Limited Premium", null, null)));

        t.Run("поддомен Patreon", () =>
            Assert.Eq(PriceStatus.Paid,
                PaidDetector.Detect("Some Car", null, new[] { U("https://author.patreon.com/posts/1") })));
    }
}
