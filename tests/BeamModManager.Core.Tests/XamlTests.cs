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

        t.Run("серый налёт выключенного мода ведёт SleepFade, а не триггер (иначе карточки «засыпают» заново при прокрутке)", () =>
        {
            var main = Read("MainWindow.xaml");
            Assert.False(main.Contains("Storyboard.TargetName=\"Sleep\"", StringComparison.Ordinal), "анимация налёта в триггере");
            Assert.True(main.Contains("c:SleepFade.Awake=\"{Binding Enabled}\"", StringComparison.Ordinal), "налёт привязан к Enabled");
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

        t.Run("значки левой панели — только контур: заливок нет, у выбранного линия толще", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            var theme = Read("Theme.xaml");
            var keys = Keys(theme);
            foreach (var name in new[] { "NavGarage", "NavUpdates", "NavSearch", "NavSettings" })
            {
                var rb = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == name);
                Assert.False(rb.Attributes().Any(a => a.Name.LocalName.StartsWith("NavGlyph.", StringComparison.Ordinal)), $"{name}: заливка значка");
                var key = Regex.Match((string?)rb.Attribute("Tag") ?? "", @"StaticResource (\w+)").Groups[1].Value;
                Assert.True(keys.Contains(key), $"{name}: контур {key} есть в теме");
            }
            Assert.False(keys.Any(k => k.StartsWith("Icon", StringComparison.Ordinal) && k.EndsWith("Fill", StringComparison.Ordinal)), "заливки значков в теме");
            Assert.True(theme.Contains("<c:NavGlyph", StringComparison.Ordinal), "пункт рисует NavGlyph");
            var code = File.ReadAllText(Path.Combine(AppDir()!, "Controls", "NavGlyph.cs"));
            var m = Regex.Match(code, @"Thin = ([0-9.]+), Bold = ([0-9.]+)");
            Assert.True(m.Success, "толщины линии");
            var thin = double.Parse(m.Groups[1].Value, System.Globalization.CultureInfo.InvariantCulture);
            var bold = double.Parse(m.Groups[2].Value, System.Globalization.CultureInfo.InvariantCulture);
            Assert.True(bold > thin && bold <= 2.6, $"выбранный толще, но не жирно ({thin} → {bold})");
            Assert.False(code.Contains("DrawGeometry(fill", StringComparison.Ordinal) || Regex.IsMatch(code, @"DrawGeometry\((?!null)"), "значок что-то заливает");
        });

        t.Run("островок после боя: прозрачнее в тёмном, с фиолетовым кантом в светлом; «Сбросить» в настройках нет", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            foreach (var name in new[] { "TopBar", "TitleCapsule" })
            {
                var g = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == name);
                Assert.Eq("True", (string?)g.Attribute("Iridescent"), $"{name}: кант");
                var tint = (string?)g.Attribute("Tint") ?? "";
                Assert.True(tint.Length == 9 && Convert.ToInt32(tint.Substring(1, 2), 16) <= 0x5A, $"{name}: подложка не плотнее 0,35 ({tint})");
                var b = double.Parse((string?)g.Attribute("Brightness") ?? "0", System.Globalization.CultureInfo.InvariantCulture);
                Assert.True(b >= 0.72, $"{name}: яркость под стеклом {b}");
            }
            Assert.False(Read("MainWindow.xaml").Contains("Content=\"Сбросить\"", StringComparison.Ordinal), "ссылка «Сбросить»");
        });

        t.Run("верх «островки»: заголовок на фоне, капсула заголовка и островок с фильтрами и поиском в одном ряду", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            XElement El(string n) => xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == n);
            var row = El("TopRow");
            foreach (var n in new[] { "TitleBlock", "TitleCapsule", "TopBar", "FilterLayer", "SearchLayer", "SearchBox", "SearchButton", "SearchClose", "FilterBox", "FilterDrop" })
                Assert.True(El(n).Ancestors().Contains(row), $"{n} — в верхнем ряду");
            Assert.Eq("Glass", El("TopBar").Name.LocalName, "островок — стекло");
            Assert.Eq("Glass", El("TitleCapsule").Name.LocalName, "капсула заголовка — стекло");
            Assert.Eq("{Binding ElementName=Backdrop}", (string?)El("TitleCapsule").Attribute("Source"), "капсула над фоном, не над сеткой");
            Assert.Eq("False", (string?)El("TitleBlock").Attribute("IsHitTestVisible"), "за заголовок окно можно тащить");
            Assert.Eq("Collapsed", (string?)El("SearchLayer").Attribute("Visibility"), "поиск свёрнут при запуске");
            Assert.Eq("True", (string?)El("TopBar").Attribute("WindowChrome.IsHitTestVisibleInChrome"), "островок нажимается, хоть и в полосе заголовка окна");
            var code = Read("MainWindow.xaml.cs");
            foreach (var m in new[] { "void OpenSearch(", "void CloseSearch(", "void PlaceIsland(", "void ApplyTitleShrink(", "void TitleFollowScroll(" })
                Assert.True(code.Contains(m, StringComparison.Ordinal), m);
        });

        t.Run("форма капли 4: панель — капсула, капля с одинаковым отступом со всех сторон, пункты ровно под каплей", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            XElement El(string n) => xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == n);
            double D(XElement e, string a) => double.Parse((string)e.Attribute(a)!, System.Globalization.CultureInfo.InvariantCulture);
            var nav = El("Nav"); var drop = El("NavDrop");
            double navW = D(nav, "Width"), navH = D(nav, "Height"), dw = D(drop, "Width"), dh = D(drop, "Height");
            Assert.Eq(navW / 2, D(nav, "Corner"), "левая панель — целая капсула");
            Assert.Eq(dh / 2, D(drop, "Corner"), "капля — капсула");
            var inset = D(drop, "Canvas.Left");
            Assert.Eq(inset, navW - inset - dw, "отступ слева и справа одинаковый");
            var items = new[] { "NavGarage", "NavUpdates", "NavSearch", "NavSettings" }.Select(El).ToList();
            var tops = items.Select(i => D(i, "Canvas.Top")).ToList();
            Assert.Eq(inset, tops[0], "сверху тот же отступ");
            var theme = Read("Theme.xaml");
            var itemH = double.Parse(Regex.Match(theme, @"x:Key=""NavItem""[\s\S]*?Property=""Height"" Value=""([\d.]+)""").Groups[1].Value, System.Globalization.CultureInfo.InvariantCulture);
            Assert.Eq(dh, itemH, "пункт той же высоты, что капля: капля ложится ровно под него");
            Assert.Eq(inset, navH - tops[^1] - itemH, "снизу тот же отступ");
            for (var i = 1; i < tops.Count; i++) Assert.True(tops[i] - tops[i - 1] >= itemH, $"пункты {i - 1} и {i} не наезжают");
            var move = xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == "NavDropMove");
            Assert.Eq(tops[0], D(move, "Y"), "капля при запуске — под первым пунктом");
            var code = Read("MainWindow.xaml.cs");
            Assert.True(Regex.IsMatch(code, @"NavDropTop\s*=\s*" + tops[0].ToString(System.Globalization.CultureInfo.InvariantCulture) + @"\b"), "пружины капли начинают с первого пункта");
            Assert.True(Regex.IsMatch(code, @"NavDropH\s*=\s*" + dh.ToString(System.Globalization.CultureInfo.InvariantCulture) + @"\b"), "высота капли в коде — как в разметке");
        });

        t.Run("капли варианта «В»: одно тело в пути (голова и хвост), плашка в покое, линза в пути, радужная кромка", () =>
        {
            var code = Read("MainWindow.xaml.cs");
            Assert.True(code.Contains("DropBody.Of(_navDrop.Value, _navTrail.Value", StringComparison.Ordinal), "левая панель — оболочка головы и хвоста");
            Assert.True(code.Contains("DropBody.Of(_filterX.Value, _filterTrail.Value", StringComparison.Ordinal), "фильтры — так же");
            foreach (var pair in new[] { ("_navDrop", "_navTrail"), ("_filterX", "_filterTrail") })
            {
                var leads = Regex.Matches(code, @"_springs\.To\(" + pair.Item1 + @",").Count;
                var trails = Regex.Matches(code, @"_springs\.To\(" + pair.Item2 + @",").Count;
                Assert.Eq(leads, trails, $"{pair.Item1}: каждой цели головы — та же цель хвоста (иначе капля застрянет растянутой)");
            }
            var drop = Read("Controls/Droplet.cs");
            Assert.True(drop.Contains("MotionProperty", StringComparison.Ordinal) && drop.Contains("class RestPaint", StringComparison.Ordinal), "плашка в покое и переход к линзе");
            Assert.True(drop.Contains("_glass.Visibility = lensVis", StringComparison.Ordinal), "пока капля стоит, линзы нет (сцена не копируется)");
            Assert.True(drop.Contains("Children = { _clip, _tint, _under, _labels }", StringComparison.Ordinal) && drop.Contains("Effect = _refraction", StringComparison.Ordinal),
                "в линзе преломляется и копия подписей (2026-10-18)");
            var xaml = Read("MainWindow.xaml");
            foreach (var (name, labels, front) in new[] { ("FilterDrop", "FilterList", "FilterFront"), ("NavDrop", "NavItems", "NavFront"), ("CapDrop", "CapButtons", "CapFront") })
            {
                var at = xaml.IndexOf($"x:Name=\"{name}\"", StringComparison.Ordinal);
                var tag = xaml[at..xaml.IndexOf('>', at)];
                Assert.True(tag.Contains($"Labels=\"{{Binding ElementName={labels}}}\"", StringComparison.Ordinal) && tag.Contains($"Front=\"{{Binding ElementName={front}}}\"", StringComparison.Ordinal),
                    $"{name}: подписи {labels} преломляются, линза поверх них в {front}");
                Assert.True(xaml.IndexOf($"x:Name=\"{front}\"", StringComparison.Ordinal) > xaml.IndexOf($"x:Name=\"{labels}\"", StringComparison.Ordinal), $"{front} — над {labels}");
            }
            Assert.True(code.Contains("b.Within(-3, FilterList.ActualWidth + 3", StringComparison.Ordinal) && code.Contains("b.Within(NavDropTop", StringComparison.Ordinal),
                "капли не уходят за крайний пункт");
            var sw = Read("Controls/GlassSwitch.cs");
            Assert.True(sw.Contains("var grow = 1 + 0.6 * lift", StringComparison.Ordinal), "ручка переключателя в пути выше дорожки");
            Assert.True(sw.Contains("радужная кромка", StringComparison.Ordinal), "у ручки радужная кромка");
        });

        t.Run("что нового (CHANGES.md): у каждой версии раздел по строке на изменение, новые сверху; выпуск берёт его", () =>
        {
            var root = Path.GetFullPath(Path.Combine(AppDir()!, "..", ".."));
            var lines = File.ReadAllLines(Path.Combine(root, "CHANGES.md"));
            var versions = new List<Version>();
            var items = 0;
            foreach (var l in lines)
            {
                var h = Regex.Match(l, @"^##\s+v?(\S+)\s*$");
                if (h.Success)
                {
                    if (versions.Count > 0) Assert.True(items > 0, $"раздел {versions[^1]} пустой");
                    var v = Version.Parse(h.Groups[1].Value.Split('.').Length == 2 ? h.Groups[1].Value + ".0" : h.Groups[1].Value);
                    if (versions.Count > 0) Assert.True(v < versions[^1], $"{v}: версии сверху вниз от новой к старой");
                    versions.Add(v);
                    items = 0;
                    continue;
                }
                if (versions.Count == 0 || l.Trim().Length == 0) continue;
                Assert.True(l.StartsWith("- ", StringComparison.Ordinal), $"строка изменения начинается с «- »: {l}");
                Assert.True(l.Length <= 90 && !l.TrimEnd().EndsWith('.'), $"суперкратко и без точки: {l}");
                items++;
            }
            Assert.True(versions.Count > 0 && items > 0, "есть хотя бы одна версия с изменениями");
            var ci = File.ReadAllText(Path.Combine(root, ".github", "workflows", "windows.yml"));
            Assert.True(ci.Contains("Get-Content CHANGES.md", StringComparison.Ordinal) && ci.Contains("--notes $notes", StringComparison.Ordinal)
                && ci.Contains("Get-Content notes.md", StringComparison.Ordinal), "выпуск пишет в описание раздел своей версии");
        });

        t.Run("значок программы: в exe и у окна, в ICO все размеры от 16 до 256 (мелкие нарисованы отдельно)", () =>
        {
            var proj = Read("BeamModManager.App.csproj");
            Assert.True(proj.Contains("<ApplicationIcon>app.ico</ApplicationIcon>", StringComparison.Ordinal), "значок у exe");
            Assert.True(proj.Contains("<Resource Include=\"app.ico\"/>", StringComparison.Ordinal), "значок вложен в программу для окна");
            Assert.True(Read("MainWindow.xaml").Contains("Icon=\"/app.ico\"", StringComparison.Ordinal), "значок у окна");
            var ico = File.ReadAllBytes(Path.Combine(AppDir()!, "app.ico"));
            Assert.True(ico.Length > 6 && ico[0] == 0 && ico[1] == 0 && ico[2] == 1 && ico[3] == 0, "заголовок ICO");
            var count = ico[4] | ico[5] << 8;
            var sizes = new HashSet<int>();
            for (var i = 0; i < count; i++)
            {
                var e = 6 + i * 16;
                var w = ico[e] == 0 ? 256 : ico[e];
                var h = ico[e + 1] == 0 ? 256 : ico[e + 1];
                Assert.Eq(w, h, "квадратные");
                var len = BitConverter.ToInt32(ico, e + 8); var off = BitConverter.ToInt32(ico, e + 12);
                Assert.True(len > 0 && off >= 6 + count * 16 && off + len <= ico.Length, $"картинка {w} лежит в файле");
                sizes.Add(w);
            }
            foreach (var need in new[] { 16, 20, 24, 32, 40, 48, 64, 256 }) Assert.True(sizes.Contains(need), $"есть {need} × {need}");
        });

        t.Run("окно первым делом включает защиту ядра от зависания разбора страниц (до любого Regex в процессе)", () =>
        {
            var code = Read("App.xaml.cs");
            var body = Regex.Match(code, @"static App\(\)\s*\{(?<b>.*?)\n    \}", RegexOptions.Singleline);
            Assert.True(body.Success, "статический конструктор App найден");
            var first = body.Groups["b"].Value.Split('\n').Select(l => l.Trim()).First(l => l.Length > 0 && !l.StartsWith("//", StringComparison.Ordinal));
            Assert.True(first.StartsWith("System.Runtime.CompilerServices.RuntimeHelpers.RunModuleConstructor(typeof(BeamModManager.Core.", StringComparison.Ordinal), first);
        });

        t.Run("островок посередине окна: ряд от левой панели до кнопок окна, центр по высоте вровень с ними", () =>
        {
            var xaml = XDocument.Parse(Read("MainWindow.xaml"));
            XElement El(string n) => xaml.Descendants().Single(e => (string?)e.Attribute(X + "Name") == n);
            double[] Th(XElement e) => ((string)e.Attribute("Margin")!).Split(',').Select(v => double.Parse(v, System.Globalization.CultureInfo.InvariantCulture)).ToArray();
            double D(XElement e, string a) => double.Parse((string)e.Attribute(a)!, System.Globalization.CultureInfo.InvariantCulture);
            var row = Th(El("TopRow"));
            var cap = El("Caption");
            var capM = Th(cap);
            // правый край ряда — перед кнопками окна, а не перед правой панелью: островок от неё не зависит
            Assert.Eq(D(cap, "Width") + capM[2] + 16, row[2], "ряд кончается за 16 px до кнопок окна");
            var bar = El("TopBar");
            Assert.Eq("Left", (string?)bar.Attribute("HorizontalAlignment"), "место островка ставит код (IslandLayout), не разметка");
            var barMid = row[1] + Th(bar)[1] + D(bar, "Height") / 2;
            Assert.Eq(capM[1] + D(cap, "Height") / 2, barMid, "центр островка вровень с кнопками окна");
            var capsule = El("TitleCapsule");
            Assert.Eq(barMid, row[1] + Th(capsule)[1] + D(capsule, "Height") / 2, "и капсула заголовка");
            var inspectorTop = Th(El("Inspector"))[1];
            Assert.True(row[1] + Th(bar)[1] + D(bar, "Height") <= inspectorTop - 4, "островок не касается правой панели, если над ней");
            var code = Read("MainWindow.xaml.cs");
            Assert.True(code.Contains("IslandLayout.Place(Root.ActualWidth", StringComparison.Ordinal), "островок ставится по ширине окна");
            Assert.False(Regex.IsMatch(code, @"TopRow\.Margin\s*="), "ряд не двигается вслед за правой панелью");
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
