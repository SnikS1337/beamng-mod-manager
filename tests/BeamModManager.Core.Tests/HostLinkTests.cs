using BeamModManager.Core.Hosts;

namespace BeamModManager.Core.Tests;

public static class HostLinkTests
{
    public static void Register(TestRunner t)
    {
        t.Group("Ссылки на файлообменники");

        t.Run("ModsFire: короткая ссылка", () =>
        {
            var link = HostLinkParser.TryParse("https://modsfire.com/9nDwN00BJ04Ef55");
            Assert.Eq("modsfire", link?.Host);
            Assert.Eq("9nDwN00BJ04Ef55", link?.FileId);
        });

        t.Run("ModsFire: ссылка с /download/ даёт тот же идентификатор", () =>
        {
            var link = HostLinkParser.TryParse("https://modsfire.com/download/9nDwN00BJ04Ef55/0fa51");
            Assert.Eq("9nDwN00BJ04Ef55", link?.FileId);
        });

        t.Run("ModsFire: служебные страницы не файлы", () =>
        {
            Assert.Null(HostLinkParser.TryParse("https://modsfire.com/get-premium?rf=26711"));
            Assert.Null(HostLinkParser.TryParse("https://modsfire.com/"));
            Assert.Null(HostLinkParser.TryParse("https://modsfire.com/login"));
        });

        t.Run("MediaFire: идентификатор из /file/", () =>
        {
            var link = HostLinkParser.TryParse("https://www.mediafire.com/file/abc123xyz/mod.zip/file");
            Assert.Eq("mediafire", link?.Host);
            Assert.Eq("abc123xyz", link?.FileId);
        });

        t.Run("мусор и чужие схемы дают null", () =>
        {
            Assert.Null(HostLinkParser.TryParse("не ссылка"));
            Assert.Null(HostLinkParser.TryParse(null));
            Assert.Null(HostLinkParser.TryParse("ftp://modsfire.com/9nDwN00BJ04Ef55"));
        });

        t.Run("идентификатор регистрозависим", () =>
        {
            var a = HostLinkParser.TryParse("https://modsfire.com/9nDwN00BJ04Ef55");
            var b = HostLinkParser.TryParse("https://modsfire.com/9ndwn00bj04ef55");
            Assert.True(a is not null && b is not null && a.FileId != b.FileId);
        });
    }
}
