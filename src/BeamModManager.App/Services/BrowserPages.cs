using System.IO;
using System.Net.Http;
using System.Text.Json;
using System.Threading.Channels;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;
using BeamModManager.Core.Net;
using BeamModManager.Core.Sources;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.Wpf;

namespace BeamModManager.App.Services;

/// <summary>
/// Страницы сайта за защитой Cloudflare (Modland) — встроенным браузером Edge (WebView2), 2026-10-18. Простой запрос
/// программы такой сайт не пускает; браузер пускает, а если просит подтвердить, что вы человек, проверку проходит сам человек
/// в окне программы (<see cref="PassCheckAsync"/>): куки браузера лежат в папке программы и общие для обоих окон.
/// Капчу программа не решает и не обходит. Браузер скрытый, живёт в потоке окна, страницы грузит по одной; картинки,
/// шрифты и видео не качает — нужен только текст страницы. Паузы, кэш и «вы человек?» — снаружи, в PoliteFetcher.
/// </summary>
public sealed class BrowserPages : IPageFetcher
{
    /// <summary>Сколько ждать загрузки страницы.</summary>
    private static readonly TimeSpan LoadTimeout = TimeSpan.FromSeconds(40);

    /// <summary>Cloudflare часто снимает проверку сам за несколько секунд и перезагружает страницу: ждём, но недолго.</summary>
    private static readonly TimeSpan SelfClearWait = TimeSpan.FromSeconds(12);

    private readonly Dispatcher _ui;
    private readonly string _dataDir;
    private readonly SemaphoreSlim _one = new(1, 1);
    private Task<CoreWebView2Environment>? _env;
    private Window? _host;
    private WebView2? _view;
    private Channel<CoreWebView2NavigationCompletedEventArgs>? _done;

    public BrowserPages(Dispatcher ui, string dataDir)
    {
        _ui = ui;
        _dataDir = dataDir;
    }

    /// <summary>Встроенный браузер Edge (WebView2) установлен. На Windows 10 и 11 обычно да.</summary>
    public static bool RuntimeAvailable()
    {
        try { return !string.IsNullOrEmpty(CoreWebView2Environment.GetAvailableBrowserVersionString()); }
        catch (Exception e) when (e is not OutOfMemoryException) { return false; }
    }

    public Task<RawResponse> GetAsync(Uri url, CancellationToken ct) => OnUiAsync(() => LoadAsync(url, ct), ct);

    private async Task<T> OnUiAsync<T>(Func<Task<T>> work, CancellationToken ct)
    {
        await _one.WaitAsync(ct).ConfigureAwait(false);
        try { return await _ui.InvokeAsync(work).Task.Unwrap().ConfigureAwait(false); }
        finally { _one.Release(); }
    }

    private Task<CoreWebView2Environment> EnvironmentAsync()
    {
        Directory.CreateDirectory(_dataDir);
        return _env ??= CoreWebView2Environment.CreateAsync(null, _dataDir);
    }

    /// <summary>Скрытое окно с браузером: за краем экрана, без панели задач и Alt+Tab, фокус не забирает.</summary>
    private async Task<CoreWebView2> ViewAsync()
    {
        if (_view?.CoreWebView2 is { } ready) return ready;
        var env = await EnvironmentAsync();
        _view = new WebView2();
        _host = new Window
        {
            Title = "Modland", Width = 1024, Height = 768, Left = -32000, Top = -32000,
            WindowStyle = WindowStyle.None, ResizeMode = ResizeMode.NoResize, ShowInTaskbar = false, ShowActivated = false,
            Content = _view,
        };
        _host.SourceInitialized += (_, _) => Native.MakeToolWindow(new WindowInteropHelper(_host).Handle);
        _host.Show();
        await _view.EnsureCoreWebView2Async(env);
        var core = _view.CoreWebView2;
        core.Settings.AreDevToolsEnabled = false;
        core.Settings.AreDefaultContextMenusEnabled = false;
        core.Settings.IsStatusBarEnabled = false;
        core.NewWindowRequested += (_, a) => a.Handled = true; // всплывающие окна сайта не открываем
        foreach (var kind in new[] { CoreWebView2WebResourceContext.Image, CoreWebView2WebResourceContext.Media, CoreWebView2WebResourceContext.Font })
            core.AddWebResourceRequestedFilter("*", kind);
        core.WebResourceRequested += (_, a) => a.Response = env.CreateWebResourceResponse(null, 404, "Not Found", "");
        // все завершения загрузки — в очередь: проверка Cloudflare может перезагрузить страницу раньше, чем мы спросим
        _done = Channel.CreateUnbounded<CoreWebView2NavigationCompletedEventArgs>();
        core.NavigationCompleted += (_, a) => _done.Writer.TryWrite(a);
        return core;
    }

    private async Task<RawResponse> LoadAsync(Uri url, CancellationToken ct)
    {
        CoreWebView2 core;
        try { core = await ViewAsync(); }
        catch (Exception e) when (e is not OperationCanceledException and not OutOfMemoryException)
        {
            throw new InvalidOperationException("встроенный браузер Edge (WebView2) не запустился", e);
        }
        while (_done!.Reader.TryRead(out _)) { } // хвосты прошлой страницы
        using var stop = ct.Register(() => _ui.BeginInvoke(() => { try { core.Stop(); } catch (InvalidOperationException) { } }));
        core.Navigate(url.AbsoluteUri);
        CoreWebView2NavigationCompletedEventArgs done;
        // не загрузилась вовремя — как таймаут обычного запроса: PoliteFetcher повторит и скажет «сайт не ответил вовремя»
        try { done = await NextAsync(LoadTimeout, ct); }
        catch (TimeoutException) { core.Stop(); throw new TaskCanceledException("timeout"); }
        var html = await HtmlAsync(core);
        for (var i = 0; i < 3 && ChallengePageDetector.IsChallenge(html); i++)
        {
            CoreWebView2NavigationCompletedEventArgs again;
            try { again = await NextAsync(SelfClearWait, ct); }
            catch (TimeoutException) { break; } // сама не снялась: нужна проверка человеком
            done = again;
            html = await HtmlAsync(core);
        }
        if (done.HttpStatusCode == 0 && !done.IsSuccess) throw Failure(done.WebErrorStatus, ct);
        return new RawResponse(done.HttpStatusCode == 0 ? 200 : done.HttpStatusCode, html);
    }

    private async Task<CoreWebView2NavigationCompletedEventArgs> NextAsync(TimeSpan timeout, CancellationToken ct)
    {
        using var limit = CancellationTokenSource.CreateLinkedTokenSource(ct);
        limit.CancelAfter(timeout);
        try { return await _done!.Reader.ReadAsync(limit.Token); }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested) { throw new TimeoutException(); }
    }

    private static async Task<string> HtmlAsync(CoreWebView2 core)
    {
        var json = await core.ExecuteScriptAsync("document.documentElement ? document.documentElement.outerHTML : ''");
        return JsonSerializer.Deserialize<string>(json) ?? "";
    }

    /// <summary>Сбой загрузки браузера — тем же исключением, что у обычного запроса: PoliteFetcher повторит и назовёт причину.</summary>
    private static Exception Failure(CoreWebView2WebErrorStatus status, CancellationToken ct) => status switch
    {
        CoreWebView2WebErrorStatus.OperationCanceled when ct.IsCancellationRequested => new OperationCanceledException(ct),
        CoreWebView2WebErrorStatus.HostNameNotResolved => new HttpRequestException(HttpRequestError.NameResolutionError, "host not resolved"),
        CoreWebView2WebErrorStatus.Timeout => new TaskCanceledException("timeout"),
        CoreWebView2WebErrorStatus.CertificateCommonNameIsIncorrect or CoreWebView2WebErrorStatus.CertificateExpired
            or CoreWebView2WebErrorStatus.ClientCertificateContainsErrors or CoreWebView2WebErrorStatus.CertificateRevoked
            or CoreWebView2WebErrorStatus.CertificateIsInvalid => new HttpRequestException(HttpRequestError.SecureConnectionError, status.ToString()),
        _ => new HttpRequestException(HttpRequestError.ConnectionError, status.ToString()),
    };

    /// <summary>
    /// Окно проверки «вы человек?»: сайт в окне программы, человек проходит проверку, окно закрывается само, как только
    /// пришла обычная страница сайта. true — прошли; false — закрыли окно сами или браузер не запустился.
    /// </summary>
    public async Task<bool> PassCheckAsync(Window owner, Uri start, bool dark)
    {
        CoreWebView2Environment env;
        try { env = await EnvironmentAsync(); }
        catch (Exception e) when (e is not OutOfMemoryException) { return false; }
        var view = new WebView2();
        var hint = new System.Windows.Controls.TextBlock
        {
            Text = "Подтвердите на сайте, что вы человек. Окно закроется само, как только сайт пустит",
            Margin = new Thickness(16, 10, 16, 10), TextWrapping = TextWrapping.Wrap, FontSize = 13,
            Foreground = dark ? System.Windows.Media.Brushes.WhiteSmoke : System.Windows.Media.Brushes.Black,
        };
        var layout = new System.Windows.Controls.DockPanel { LastChildFill = true };
        System.Windows.Controls.DockPanel.SetDock(hint, System.Windows.Controls.Dock.Top);
        layout.Children.Add(hint);
        layout.Children.Add(view);
        var w = new Window
        {
            Title = "Проверка Modland", Owner = owner, Width = 980, Height = 720, MinWidth = 520, MinHeight = 420,
            WindowStartupLocation = WindowStartupLocation.CenterOwner, ShowInTaskbar = false, Content = layout,
            Background = dark ? new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(0x15, 0x1B, 0x2A)) : System.Windows.Media.Brushes.White,
        };
        w.SourceInitialized += (_, _) => Native.SetDarkFrame(w, dark);
        var passed = false;
        w.Loaded += async (_, _) =>
        {
            try
            {
                await view.EnsureCoreWebView2Async(env);
                var core = view.CoreWebView2;
                core.NewWindowRequested += (_, a) => { a.Handled = true; core.Navigate(a.Uri); };
                var site = start.Host.StartsWith("www.", StringComparison.OrdinalIgnoreCase) ? start.Host[4..] : start.Host;
                core.NavigationCompleted += async (_, a) =>
                {
                    if (passed) return;
                    // пускает: обычная страница этого сайта (не страница проверки и не ошибка)
                    var ok = a.HttpStatusCode is > 0 and < 400 || (a.HttpStatusCode == 0 && a.IsSuccess);
                    if (!ok || !Uri.TryCreate(core.Source, UriKind.Absolute, out var at)
                        || !at.Host.EndsWith(site, StringComparison.OrdinalIgnoreCase)) return;
                    string html;
                    try { html = await HtmlAsync(core); }
                    catch (Exception e) when (e is InvalidOperationException or System.Runtime.InteropServices.COMException) { return; }
                    if (html.Length < 200 || ChallengePageDetector.IsChallenge(html)) return;
                    passed = true;
                    hint.Text = "Готово: сайт пускает программу";
                    await Task.Delay(700);
                    if (w.IsVisible) w.Close();
                };
                core.Navigate(start.AbsoluteUri);
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                hint.Text = "Встроенный браузер Edge (WebView2) не запустился. Закройте окно и попробуйте ещё раз";
            }
        };
        var closed = new TaskCompletionSource();
        w.Closed += (_, _) => { view.Dispose(); closed.TrySetResult(); };
        w.Show();
        await closed.Task;
        return passed;
    }
}
