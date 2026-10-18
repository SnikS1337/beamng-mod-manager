using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;

namespace BeamModManager.Core.Tests;

/// <summary>Modland через встроенный браузер Edge (2026-10-18): кто что грузит и как это подключено в окне.</summary>
public static class ModlandBrowserTests
{
    private sealed class Tag : IPageFetcher
    {
        private readonly string _name;
        public readonly List<string> Calls = new();
        public Tag(string name) => _name = name;
        public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) { Calls.Add("get " + url.Host); return Task.FromResult(new RawResponse(200, _name)); }
        public Task<RawResponse> PostFormAsync(Uri url, string form, CancellationToken ct) { Calls.Add("post " + url.Host); return Task.FromResult(new RawResponse(200, _name)); }
    }

    private static string AppFile(string name)
    {
        for (var d = new DirectoryInfo(AppContext.BaseDirectory); d is not null; d = d.Parent)
        {
            var p = Path.Combine(d.FullName, "src", "BeamModManager.App", name);
            if (File.Exists(p)) return File.ReadAllText(p);
        }
        throw new FileNotFoundException(name);
    }

    public static void Register(TestRunner t)
    {
        t.Group("Modland через встроенный браузер");

        t.Run("страницы Modland — браузером (оба адреса, регистр не важен), остальные сайты — обычным запросом", async () =>
        {
            var plain = new Tag("plain");
            var browser = new Tag("browser");
            var r = new RoutingPageFetcher(plain, browser, ModlandSource.Hosts);
            Assert.Eq("browser", (await r.GetAsync(new Uri("https://www.modland.net/beamng.drive-mods"), default)).Body);
            Assert.Eq("browser", (await r.GetAsync(new Uri("https://MODLAND.net/search/bmw"), default)).Body);
            Assert.Eq("plain", (await r.GetAsync(new Uri("https://stmods.org/beamng_drive/"), default)).Body);
            Assert.Eq("plain", (await r.PostFormAsync(new Uri("https://stmods.org/search.php"), "a=1", default)).Body);
            Assert.Eq("plain", (await r.GetAsync(new Uri("https://evil-modland.net/"), default)).Body);
            Assert.Eq(2, browser.Calls.Count);
        });

        t.Run("вежливость общая: страница проверки из браузера — тот же «не пускает», пауза и кэш как у всех", async () =>
        {
            var challenge = new Tag("<html><head><title>Just a moment...</title></head><body></body></html>");
            var f = new PoliteFetcher(new RoutingPageFetcher(new Tag("plain"), challenge, ModlandSource.Hosts),
                new PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero }, delay: (_, _) => Task.CompletedTask);
            var res = await f.GetAsync(new Uri("https://www.modland.net/beamng.drive-mods"));
            Assert.Eq(SourceStatus.NeedsHumanCheck, res.Source);
            Assert.Eq(SourceStatus.NeedsHumanCheck, f.StatusOf("www.modland.net"));
            foreach (var h in ModlandSource.Hosts) f.ResetHost(h);
            Assert.Eq(SourceStatus.Ok, f.StatusOf("www.modland.net"));
        });

        t.Run("окно: браузер — только если Edge (WebView2) установлен; «Пройти проверку» в строке сайта; прошли — сайт снова опрашивается", () =>
        {
            var app = AppFile("App.xaml.cs");
            Assert.True(app.Contains("BrowserPages.RuntimeAvailable() ? new BrowserPages(", StringComparison.Ordinal), "без Edge — обычные запросы");
            Assert.True(app.Contains("new RoutingPageFetcher(new HttpPageFetcher(), browser, ModlandSource.Hosts)", StringComparison.Ordinal), "Modland — браузером");
            Assert.True(app.Contains("foreach (var h in ModlandSource.Hosts) fetcher.ResetHost(h);", StringComparison.Ordinal), "прошли проверку — пауза снята");
            var xaml = AppFile("MainWindow.xaml");
            Assert.True(xaml.Contains("Content=\"Пройти проверку\" FontSize=\"12.5\" Margin=\"8,0,0,0\" Click=\"SitePass_Click\"", StringComparison.Ordinal)
                && xaml.Contains("Visibility=\"{Binding ShowRetry, Converter={StaticResource BoolToVis}}\"", StringComparison.Ordinal), "кнопки в строке сайта");
            var pages = AppFile("Services/BrowserPages.cs");
            Assert.True(pages.Contains("CoreWebView2WebResourceContext.Image", StringComparison.Ordinal), "картинки скрытый браузер не качает");
            Assert.True(pages.Contains("core.NewWindowRequested += (_, a) => a.Handled = true;", StringComparison.Ordinal), "всплывающих окон нет");
            Assert.True(AppFile("BeamModManager.App.csproj").Contains("<PackageReference Include=\"Microsoft.Web.WebView2\"", StringComparison.Ordinal), "пакет WebView2");
        });
    }
}
