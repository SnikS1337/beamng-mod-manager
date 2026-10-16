using System.Reflection;
using System.Text.RegularExpressions;
using System.Xml.Linq;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Разметка окна без Windows: окно здесь не собрать, а ошибки в XAML всплывают только на сборочной машине или при
/// запуске. Проверяется то, что ломалось или было решено 2026-10-16: ключи палитр, обработчики, имена в анимациях,
/// привязки к удалённым настройкам, иней убран, слои наведения карточек вне общего кэша.
/// </summary>
public static class XamlTests
{
    private static readonly XNamespace X = "http://schemas.microsoft.com/winfx/2006/xaml";

    /// <summary>Папка src/BeamModManager.App рядом с тестами (сборка из репозитория); null — тесты запущены не из него.</summary>
    private static string? AppDir()
    {
        for (var d = new DirectoryInfo(AppContext.BaseDirectory); d is not null; d = d.Parent)
        {
            var p = Path.Combine(d.FullName, "src", "BeamModManager.App");
            if (File.Exists(Path.Combine(p, "MainWindow.xaml"))) return p;
        }
        return null;
    }

    private static string Read(string name) => File.ReadAllText(Path.Combine(AppDir()!, name));

    private static HashSet<string> Keys(string xaml) =>
        Regex.Matches(xaml, @"x:Key=""([^""{}]+)""").Select(m => m.Groups[1].Value).ToHashSet(StringComparer.Ordinal);

    public static void Register(TestRunner t)
    {
        t.Group("Разметка окна (XAML)");
        if (AppDir() is null)
        {
            t.Run("исходники окна не найдены рядом с тестами — проверки разметки пропущены", () => { });
            return;
        }

        t.Run("тёмная и светлая палитры с одинаковыми ключами", () =>
        {
            var dark = Keys(Read("Palette.Dark.xaml"));
            var light = Keys(Read("Palette.Light.xaml"));
            var onlyDark = dark.Except(light).ToList();
            var onlyLight = light.Except(dark).ToList();
            Assert.True(onlyDark.Count == 0 && onlyLight.Count == 0,
                $"только в тёмной: {string.Join(", ", onlyDark)}; только в светлой: {string.Join(", ", onlyLight)}");
            Assert.True(dark.Contains("PrimaryGlow") && dark.Contains("GlassShadow"), "свечение кнопки и тень стекла — из палитры");
        });

        t.Run("каждый DynamicResource окна и темы где-то объявлен", () =>
        {
            var known = new HashSet<string>(StringComparer.Ordinal);
            foreach (var f in new[] { "Palette.Dark.xaml", "Theme.xaml", "MainWindow.xaml", "App.xaml" })
                if (File.Exists(Path.Combine(AppDir()!, f))) known.UnionWith(Keys(Read(f)));
            foreach (Match m in Regex.Matches(Read("MainWindow.xaml.cs"), @"Resources\[""(\w+)""\]")) known.Add(m.Groups[1].Value);
            var missing = new List<string>();
            foreach (var f in new[] { "Theme.xaml", "MainWindow.xaml" })
                foreach (Match m in Regex.Matches(Read(f), @"\{DynamicResource ([\w.]+)\}"))
                    if (!known.Contains(m.Groups[1].Value)) missing.Add($"{f}: {m.Groups[1].Value}");
            Assert.True(missing.Count == 0, "не объявлены: " + string.Join(", ", missing.Distinct()));
        });

        t.Run("обработчики событий из разметки окна есть в его коде", () =>
        {
            var code = Read("MainWindow.xaml.cs");
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            var missing = new List<string>();
            var n = 0;
            foreach (var a in xaml.Descendants().Attributes())
            {
                if (a.Name.Namespace != XNamespace.None || !Regex.IsMatch(a.Value, @"^[A-Z]\w*_[A-Z]\w*$")) continue;
                if (a.Parent!.Name.LocalName.Contains('.')) continue; // свойство-элемент, а не событие
                n++;
                if (!Regex.IsMatch(code, $@"void {Regex.Escape(a.Value)}\(")) missing.Add($"{a.Name.LocalName}=\"{a.Value}\"");
            }
            Assert.True(n > 20, $"обработчиков найдено мало ({n}) — разбор разметки сломался");
            Assert.True(missing.Count == 0, "нет в коде: " + string.Join(", ", missing));
        });

        t.Run("тема без кода: в Theme.xaml нет обработчиков событий", () =>
        {
            var xaml = XDocument.Parse(Read("Theme.xaml"));
            var events = xaml.Descendants().Attributes()
                .Where(a => a.Name.Namespace == XNamespace.None && Regex.IsMatch(a.Value, @"^[A-Z]\w*_[A-Z]\w*$"))
                .Select(a => $"{a.Name.LocalName}=\"{a.Value}\"").ToList();
            Assert.True(events.Count == 0, string.Join(", ", events));
        });

        t.Run("анимации ссылаются на имена из своего шаблона или окна", () =>
        {
            foreach (var f in new[] { "MainWindow.xaml", "Theme.xaml" })
            {
                var xaml = XDocument.Parse(Read(f));
                var bad = new List<string>();
                foreach (var a in xaml.Descendants().Attributes().Where(a => a.Name.LocalName == "Storyboard.TargetName"))
                {
                    // имя ищется в ближайшем шаблоне (у шаблона своя область имён), а вне шаблонов — во всём файле
                    var scope = a.Parent!.Ancestors().FirstOrDefault(e => e.Name.LocalName is "ControlTemplate" or "DataTemplate")
                                ?? xaml.Root!;
                    if (!scope.Descendants().Any(e => (string?)e.Attribute(X + "Name") == a.Value || (string?)e.Attribute("Name") == a.Value))
                        bad.Add(a.Value);
                }
                Assert.True(bad.Count == 0, $"{f}: нет имён {string.Join(", ", bad.Distinct())}");
            }
        });

        t.Run("переключатели и плитки привязаны к существующим свойствам (удалённые настройки инея не остались)", () =>
        {
            var props = typeof(BeamModManager.Ui.Settings.SettingsViewModel).Assembly.GetTypes()
                .SelectMany(tp => tp.GetProperties(BindingFlags.Public | BindingFlags.Instance)).Select(p => p.Name).ToHashSet();
            var bad = new List<string>();
            foreach (var f in new[] { "MainWindow.xaml", "Theme.xaml" })
                foreach (Match m in Regex.Matches(Read(f), @"Is(?:Checked|Enabled)=""\{Binding (\w+)\}"""))
                    if (!props.Contains(m.Groups[1].Value)) bad.Add(m.Groups[1].Value);
            Assert.True(bad.Count == 0, "нет свойств: " + string.Join(", ", bad));
            var main = Read("MainWindow.xaml");
            Assert.True(main.Contains("{Binding IsBeautiful}") && main.Contains("{Binding IsFast}"), "плитки «Красиво» и «Быстро»");
        });

        t.Run("иней и «Искры льда» убраны из окна; подписи «Спокойные цвета» нет", () =>
        {
            var all = Read("MainWindow.xaml") + Read("Theme.xaml") + Read("MainWindow.xaml.cs");
            foreach (var w in new[] { "FrostOpacity", "CardFrost", "IceReach", " Ice=\"", "FrostSparks", "FrostStrength", "Искры льда\"", "Иней на стекле" })
                Assert.False(all.Contains(w, StringComparison.Ordinal), $"осталось «{w}»");
            Assert.False(all.Contains("Спокойные цвета", StringComparison.Ordinal), "подпись под темами фона");
        });

        t.Run("наведение на карточку не перерисовывает кэш карточки: свечения и кромки вне общего кэша", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            var checkedLayers = 0;
            foreach (var name in new[] { "GlowHover", "GlowPick", "RimHover", "RimBright" })
                foreach (var el in xaml.Descendants().Where(e => (string?)e.Attribute(X + "Name") == name))
                {
                    checkedLayers++;
                    var cachedParent = el.Ancestors().FirstOrDefault(p => p.Elements().Any(c => c.Name.LocalName.EndsWith(".CacheMode", StringComparison.Ordinal)));
                    Assert.True(cachedParent is null,
                        $"{name} внутри кэшированного {cachedParent?.Name.LocalName} «{(string?)cachedParent?.Attribute(X + "Name")}»: каждый кадр анимации перерисовывал бы всю карточку");
                }
            Assert.True(checkedLayers >= 6, $"слоёв наведения найдено {checkedLayers}");
            foreach (var glow in xaml.Descendants().Where(e => ((string?)e.Attribute(X + "Name"))?.StartsWith("Glow", StringComparison.Ordinal) == true))
                Assert.True(glow.Elements().Any(c => c.Name.LocalName.EndsWith(".CacheMode", StringComparison.Ordinal)),
                    $"{(string?)glow.Attribute(X + "Name")}: размытая тень свечения кэширована сама");
        });

        t.Run("сквозь левую панель и капсулу окна видно только фон, а не сетку карточек", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            foreach (var name in new[] { "Nav", "Caption", "CheckPanel" })
            {
                var g = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == name);
                Assert.Eq("{Binding ElementName=Backdrop}", (string?)g.Attribute("Source"), name);
            }
        });

        t.Run("капля кнопок окна целиком внутри капсулы (не режет блик кромки)", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            var cap = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == "Caption");
            var drop = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == "CapDrop");
            double D(XElement e, string a) => double.Parse((string)e.Attribute(a)!, System.Globalization.CultureInfo.InvariantCulture);
            var top = D(drop, "Canvas.Top");
            Assert.True(top >= 2 && top + D(drop, "Height") <= D(cap, "Height") - 2, "по высоте отступ не меньше 2 px");
            Assert.True(D(drop, "Width") <= 44 - 4, "уже кнопки на 4 px");
        });
    }
}
