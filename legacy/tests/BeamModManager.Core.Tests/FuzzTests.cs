using System.Diagnostics;
using BeamModManager.Core.Models;
using BeamModManager.Core.Sources.BeamNg;
using BeamModManager.Core.Sources.Modland;
using BeamModManager.Core.Sources.ModsFire;
using BeamModManager.Core.Sources.ModsGaming;
using BeamModManager.Core.Sources.StMods;

namespace BeamModManager.Core.Tests;

/// <summary>Парсеры на искажённых страницах: обрезанных, с мусором, огромных. Не должно быть исключений и зависаний.</summary>
public static class FuzzTests
{
    private sealed class Trap : BeamModManager.Core.Net.IPageFetcher
    {
        private readonly string _body;
        public Trap(string body) { _body = body; }
        public Task<BeamModManager.Core.Net.RawResponse> GetAsync(Uri url, CancellationToken ct) => Task.FromResult(new BeamModManager.Core.Net.RawResponse(200, _body));
    }

    private static string Fx(string path)
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !Directory.Exists(Path.Combine(dir, "tests", "fixtures"))) dir = Path.GetDirectoryName(dir);
        return File.ReadAllText(Path.Combine(dir!, "tests", "fixtures", path));
    }

    private const string JunkChars = "<>\"'&;/=\0\uFFFD\n";
    private static readonly Listing Basis = new("x", new Uri("https://x.test/a"), "a", "A");

    private static readonly (string Name, string File, Action<string> Run)[] Targets =
    {
        ("modland list", "modland/list-real.html", h => ModlandParser.ParseList(h)),
        ("modland detail", "modland/mod-paid-real.html", h => ModlandParser.ParseDetail(h, Basis)),
        ("modland pack", "modland/pack-bmw-kofi-discord-real.html", h => ModlandParser.ParseDetail(h, Basis)),
        ("modsgaming list", "modsgaming/list-real.html", h => ModsGamingParser.ParseList(h)),
        ("modsgaming search", "modsgaming/search-real.html", h => ModsGamingParser.ParseSearch(h)),
        ("modsgaming detail", "modsgaming/mod-real.html", h => ModsGamingParser.ParseDetail(h, Basis)),
        ("stmods list", "stmods/list-real.html", h => StModsParser.ParseList(h)),
        ("stmods detail", "stmods/mod-real.html", h => StModsParser.ParseDetail(h, Basis)),
        ("modsfire 1", "modsfire/first-step-real.html", h => { ModsFireParser.ParseFile(h); ModsFireParser.DetectStep(h); }),
        ("modsfire 2", "modsfire/second-step-real.html", h => { ModsFireParser.ParseFile(h); ModsFireParser.DetectStep(h); }),
        ("beamng list", "beamng/list-real.html", h => BeamNgRepoParser.ParseList(h)),
        ("beamng detail", "beamng/mod-real.html", h => BeamNgRepoParser.ParseDetail(h, Basis)),
    };

    public static void Register(TestRunner t)
    {
        t.Group("Устойчивость разбора страниц");

        t.Run("обрезанные страницы: без исключений", () =>
        {
            foreach (var (name, file, run) in Targets)
            {
                var html = Fx(file);
                for (var i = 1; i <= 40; i++)
                {
                    var cut = html.Substring(0, html.Length * i / 41);
                    try { run(cut); }
                    catch (Exception e) { Assert.True(false, $"{name}, обрезка {i}/41: {e.GetType().Name}: {e.Message}"); }
                }
            }
        });

        t.Run("случайный мусор вместо символов: без исключений", () =>
        {
            var rng = new Random(12345);
            foreach (var (name, file, run) in Targets)
            {
                var html = Fx(file);
                for (var k = 0; k < 25; k++)
                {
                    var chars = html.ToCharArray();
                    for (var m = 0; m < 60; m++) chars[rng.Next(chars.Length)] = JunkChars[rng.Next(JunkChars.Length)];
                    try { run(new string(chars)); }
                    catch (Exception e) { Assert.True(false, $"{name}, прогон {k}: {e.GetType().Name}: {e.Message}"); }
                }
            }
        });

        t.Run("пустая строка, один тег, незакрытые кавычки", () =>
        {
            foreach (var (name, _, run) in Targets)
                foreach (var h in new[] { "", " ", "<", "<a href=\"", "<li class=\"resourceListItem\" id=\"resource-1\">", "<div class=\"post-medium\"><a href=\"", "<h3 class=\"title\"><a href=\"resources/x/\">" })
                {
                    try { run(h); }
                    catch (Exception e) { Assert.True(false, $"{name}, «{h}»: {e.GetType().Name}: {e.Message}"); }
                }
        });

        t.Run("большие и патологические страницы разбираются быстро (нет катастрофического перебора)", () =>
        {
            var cases = new List<(string, string)>
            {
                ("повтор открывающих", string.Concat(Enumerable.Repeat("<div class=\"post post-medium\"><h2 class=\"post-title\"><a href=\"/x\">", 5000))),
                ("много карточек без закрытия", string.Concat(Enumerable.Repeat("<li class=\"resourceListItem\" id=\"resource-1\"><h3 class=\"title\">", 5000))),
                ("длинный текст без тегов", new string('a', 2_000_000)),
                ("много <a без >", string.Concat(Enumerable.Repeat("<a href=\"x\" ", 20000))),
                ("много пробелов", "<a" + new string(' ', 500_000) + "href=\"x\">"),
                ("вложенные теги", string.Concat(Enumerable.Repeat("<span>", 50000))),
            };
            var slow = new List<string>();
            foreach (var (name, file, run) in Targets)
                foreach (var (cn, html) in cases)
                {
                    var sw = Stopwatch.StartNew();
                    try { run(html); }
                    catch (System.Text.RegularExpressions.RegexMatchTimeoutException) { /* допустимо: общий предел сработал, адаптер превратит в отказ сайта */ }
                    catch (Exception e) { slow.Add($"{name} / {cn}: {e.GetType().Name}"); continue; }
                    if (sw.ElapsedMilliseconds > 8000) slow.Add($"{name} / {cn}: {sw.ElapsedMilliseconds} мс");
                }
            Assert.True(slow.Count == 0, string.Join("; ", slow));
        });

        t.Run("адаптеры: страница-ловушка превращается в отказ сайта, а не в зависание или падение", async () =>
        {
            var trap = string.Concat(Enumerable.Repeat("<a href=\"x\" ", 20000));
            var opts = new BeamModManager.Core.Net.PoliteFetchOptions { MinDelay = TimeSpan.Zero, Jitter = TimeSpan.Zero };
            var fetcher = new BeamModManager.Core.Net.PoliteFetcher(new Trap(trap), opts, delay: (_, _) => Task.CompletedTask);
            var sites = new BeamModManager.Core.Sources.IModSource[]
            {
                new BeamModManager.Core.Sources.ModlandSource(fetcher), new BeamModManager.Core.Sources.StModsSource(fetcher),
                new BeamModManager.Core.Sources.ModsGamingSource(fetcher), new BeamModManager.Core.Sources.BeamNgRepoSource(fetcher),
            };
            foreach (var site in sites)
            {
                var sw = Stopwatch.StartNew();
                try { await site.GetListingAsync(new Uri("https://x.test/a/b")); }
                catch (BeamModManager.Core.Sources.SourceException e) { Assert.Eq(BeamModManager.Core.Sources.SourceStatus.Unavailable, e.Status); }
                Assert.True(sw.ElapsedMilliseconds < 15000, $"{site.Id}: {sw.ElapsedMilliseconds} мс");
            }
        });

        t.Run("страница, увеличенная в 20 раз, разбирается быстро", () =>
        {
            foreach (var (name, file, run) in Targets)
            {
                var html = string.Concat(Enumerable.Repeat(Fx(file), 20));
                var sw = Stopwatch.StartNew();
                run(html);
                Assert.True(sw.ElapsedMilliseconds < 4000, $"{name}: {sw.ElapsedMilliseconds} мс");
            }
        });
    }
}
