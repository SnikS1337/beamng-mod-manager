using BeamModManager.Core.Hosts;
using BeamModManager.Core.Text;
using BeamModManager.Core.Versioning;

namespace BeamModManager.Core.Tests;

/// <summary>Регрессионные тесты на ошибки, найденные при ревизии кода.</summary>
public static class BugHuntTests
{
    private const long MB = 1024 * 1024;

    public static void Register(TestRunner t)
    {
        t.Group("Ревизия: размеры и версии");

        t.Run("размер: округление на сайте («316,40 МБ») не считается изменением", () =>
        {
            var real = (long)(316.4037 * MB);
            var shown = (long)(316.40 * MB);
            Assert.False(SizeComparer.Differs(real, shown), "округление до сотых");
            Assert.False(SizeComparer.Differs((long)(1.24 * 1024 * MB), (long)(1.2 * 1024 * MB)), "«1,2 ГБ» вместо 1,24 ГБ");
            Assert.False(SizeComparer.Differs(100, 100), "равны");
        });
        t.Run("размер: заметное изменение — отличие; неизвестное и нулевое — нет", () =>
        {
            Assert.True(SizeComparer.Differs(100 * MB, 130 * MB), "+30%");
            Assert.True(SizeComparer.Differs(300 * 1024, 1024 * 1024), "малый файл заметно вырос");
            Assert.False(SizeComparer.Differs(null, 5), "нет данных");
            Assert.False(SizeComparer.Differs(0, 5), "ноль не размер");
        });
        t.Run("решение об обновлении: одинаковая версия и округлённый размер — обновления нет", () =>
        {
            var real = (long)(316.4037 * MB);
            var shown = (long)(316.40 * MB);
            Assert.Eq(UpdateStatus.None, UpdateDecider.Decide(new VersionSnapshot("5.1", real), new VersionSnapshot("5.1", shown)));
            Assert.Eq(UpdateStatus.Possible, UpdateDecider.Decide(new VersionSnapshot("5.1", real), new VersionSnapshot("5.1", real + 40 * MB)));
        });

        t.Run("версии: beta раньше релиза, rc после beta, alpha перед beta", () =>
        {
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("0.3.1-beta", "0.3.1"));
            Assert.Eq(VersionOrder.Older, VersionComparer.Compare("0.3.1", "0.3.1-beta"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0 beta", "1.0 rc"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0-alpha", "1.0-beta"));
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0-rc1", "1.0-rc2"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.0-beta", "1.0-beta"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.0-beta1", "1.0-beta"));
        });
        t.Run("версии: число важнее метки; слова-«не метки» не считаются ими", () =>
        {
            Assert.Eq(VersionOrder.Newer, VersionComparer.Compare("1.0.9-beta", "1.1.0-alpha"));
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("2.0 Bold Edition", "2.0")); // «b» из Bold меткой не считается
            Assert.Eq(VersionOrder.Same, VersionComparer.Compare("1.2 Adventure", "1.2"));
        });
        t.Run("версии: огромные числа и мусор не вызывают исключений", () =>
        {
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare("1.99999999999999999999", "1.0"));
            Assert.Eq(VersionOrder.Unknown, VersionComparer.Compare(new string('9', 5000) + ".1", "1.0"));
            foreach (var x in new[] { "", " ", "..", "v", "1.", ".1", "1..2", "-1.0" }) VersionComparer.Compare(x, "1.0");
        });

        t.Run("ModsFire: ссылки /d/{id} и /install_mod/{id} дают тот же идентификатор файла, что и /download/", () =>
        {
            var a = HostLinkParser.TryParse("https://modsfire.com/d/9nDwN00BJ04Ef55");
            var b = HostLinkParser.TryParse("https://modsfire.com/install_mod/9nDwN00BJ04Ef55/?manager=ava");
            var c = HostLinkParser.TryParse("https://modsfire.com/9nDwN00BJ04Ef55");
            Assert.Eq("9nDwN00BJ04Ef55", a?.FileId);
            Assert.Eq("9nDwN00BJ04Ef55", b?.FileId);
            Assert.Eq(c?.FileId, a?.FileId);
        });
    }
}
