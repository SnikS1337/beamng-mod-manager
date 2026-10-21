using System.Text.RegularExpressions;
using BeamModManager.Core.Settings;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Защиты, которые легко потерять незаметно: предел времени разбора чужих страниц и сохранение каждого значения
/// настроек-перечислений (тема, оформление) между запусками.
/// </summary>
public static class GuardTests
{
    private static string Tmp()
    {
        var d = Path.Combine(Path.GetTempPath(), "bmm-guard-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(d);
        return d;
    }

    public static void Register(TestRunner t)
    {
        t.Group("Защиты");

        t.Run("регулярные выражения без своего предела получают предел ядра (3 с): мусорная страница не вешает разбор", () =>
        {
            _ = new AppSettings(); // ядро загружено — его защита уже стоит
            Assert.Eq(TimeSpan.FromSeconds(3), new Regex("a+").MatchTimeout,
                "общий предел не действует: какой-то Regex создан раньше, чем загрузилось ядро");
        });

        t.Run("каждое значение темы, оформления и других перечислений в настройках переживает перезапуск", () =>
        {
            var p = Path.Combine(Tmp(), "settings.json");
            var st = new SettingsStore(p);
            var props = typeof(AppSettings).GetProperties().Where(x => x.CanWrite && x.PropertyType.IsEnum).ToList();
            Assert.True(props.Count >= 2, "в настройках есть перечисления (оформление, тема фона)");
            foreach (var prop in props)
            foreach (var value in Enum.GetValues(prop.PropertyType))
            {
                var want = new AppSettings();
                prop.SetValue(want, value);
                st.Save(want);
                var got = st.Load();
                Assert.False(got.HadProblems, $"{prop.Name} = {value}: без предупреждений");
                Assert.Eq(value, prop.GetValue(got.Settings), $"{prop.Name} = {value}");
            }
        });
    }
}
