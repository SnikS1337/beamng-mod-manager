using System.Collections.ObjectModel;
using BeamModManager.Core.Library;
using BeamModManager.Core.Models;
using BeamModManager.Ui.Garage;
using BeamModManager.Ui.Mvvm;
using BeamModManager.Ui.Text;

namespace BeamModManager.Ui.Updates;

/// <summary>Подсказка в окне «Указать страницу»: запись на сайте, похожая на мод.</summary>
public sealed class PageSuggestion : Observable
{
    internal PageSuggestion(BindingSuggestion s)
    {
        Listing = s.Listing;
        Site = SiteNames.Of(s.Listing.Source);
        Title = s.Listing.Title;
        var bits = new List<string>();
        if (s.Listing.ModVersion is { } v) bits.Add("версия " + v);
        if (s.Listing.SizeBytes is { } b) bits.Add(Ru.Size(b));
        if (s.Listing.Author is { } a) bits.Add(a);
        Details = string.Join(" · ", bits);
    }

    public Listing Listing { get; }
    public string Site { get; }
    public string Title { get; }
    /// <summary>«версия 2.2 · 88 МБ · автор»; пусто, если сайт ничего не сказал.</summary>
    public string Details { get; }

    private bool _isSelected;
    public bool IsSelected { get => _isSelected; set => Set(ref _isSelected, value); }
}

/// <summary>
/// Окно «Указать страницу мода» (DESIGN_FLOWS 6.4) и «Найти замену» (6.5): ссылка, которую вставил человек, или одна из
/// подсказок поиска по названию. Ничего не привязывается без выбора; главная кнопка доступна после проверки ссылки
/// или выбора подсказки.
/// </summary>
public sealed class PageBindViewModel : Observable
{
    private readonly IUpdatesBackend _backend;
    private readonly string? _replaceSource;
    private int _linkSerial;

    /// <param name="replaceSource">Сайт пропавшей страницы (режим «Найти замену»); null — обычное «Указать страницу».</param>
    public PageBindViewModel(IUpdatesBackend backend, ModCard card, string? replaceSource)
    {
        _backend = backend;
        Card = card;
        _replaceSource = replaceSource;
    }

    public ModCard Card { get; }
    public bool Replacing => _replaceSource is not null;
    public string Title => Replacing ? "Страница пропала. Вот похожие моды" : "Где следить за модом";
    public string Lead => Replacing
        ? "Выберите похожую запись или вставьте ссылку на новую страницу мода."
        : "Вставьте ссылку на страницу мода или выберите одну из похожих записей.";
    public string ApplyLabel => Replacing ? "Следить здесь" : "Следить на этой странице";

    public ObservableCollection<PageSuggestion> Suggestions { get; } = new();

    private bool _searching;
    public bool Searching { get => _searching; private set => Set(ref _searching, value); }

    private string? _searchNote;
    /// <summary>«Ищем похожие на сайтах…», «Похожих не нашли» или причина, почему поиск не удался.</summary>
    public string? SearchNote { get => _searchNote; private set => Set(ref _searchNote, value); }

    private string _link = "";
    public string LinkText
    {
        get => _link;
        set
        {
            if (!Set(ref _link, value ?? "")) return;
            _linkSerial++;
            LinkResult = null;
            LinkNote = null;
            if (_link.Trim().Length > 0) foreach (var s in Suggestions) s.IsSelected = false;
            Raise(nameof(CanApply));
        }
    }

    private PageCheck? _linkResult;
    private PageCheck? LinkResult { get => _linkResult; set { _linkResult = value; Raise(nameof(LinkOk)); Raise(nameof(CanApply)); } }
    public bool LinkOk => _linkResult?.Ok == true;

    private bool _checkingLink;
    public bool CheckingLink { get => _checkingLink; private set => Set(ref _checkingLink, value); }

    private string? _linkNote;
    /// <summary>Под полем: «Modland · Toyota Sienna v2.2», «Проверяем ссылку…» или почему не подходит.</summary>
    public string? LinkNote { get => _linkNote; private set => Set(ref _linkNote, value); }

    public PageSuggestion? Selected => Suggestions.FirstOrDefault(s => s.IsSelected);

    public bool CanApply => LinkOk || (Selected is not null && _link.Trim().Length == 0);

    /// <summary>Выбрать подсказку (радио): ссылка в поле при этом стирается.</summary>
    public void Select(PageSuggestion s)
    {
        foreach (var x in Suggestions) x.IsSelected = ReferenceEquals(x, s);
        if (_link.Length > 0) { _link = ""; _linkSerial++; LinkResult = null; LinkNote = null; Raise(nameof(LinkText)); }
        Raise(nameof(Selected));
        Raise(nameof(CanApply));
    }

    /// <summary>Подсказки поиска по названию (до трёх). Окно зовёт при открытии.</summary>
    public async Task LoadAsync(CancellationToken ct = default)
    {
        Searching = true;
        SearchNote = "Ищем похожие на сайтах…";
        try
        {
            var found = await _backend.SuggestAsync(Card.Id, Replacing, ct);
            Suggestions.Clear();
            foreach (var s in found.Take(3)) Suggestions.Add(new PageSuggestion(s));
            SearchNote = Suggestions.Count == 0 ? "Похожих не нашли. Вставьте ссылку на страницу мода сами" : null;
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested) { }
        catch (Exception e) when (e is not OutOfMemoryException)
        {
            SearchNote = "Поиск не удался. Можно вставить ссылку на страницу сами";
        }
        finally
        {
            Searching = false;
        }
    }

    /// <summary>Проверить ссылку из поля (окно зовёт после паузы в наборе). Устаревший ответ (поле уже другое) не применяется.</summary>
    public async Task CheckLinkAsync(CancellationToken ct = default)
    {
        var text = _link;
        var serial = _linkSerial;
        if (text.Trim().Length == 0) { LinkResult = null; LinkNote = null; return; }
        CheckingLink = true;
        LinkNote = "Проверяем ссылку…";
        PageCheck r;
        try { r = await _backend.CheckPageAsync(text, ct); }
        catch (OperationCanceledException) when (ct.IsCancellationRequested) { return; }
        catch (Exception e) when (e is not OutOfMemoryException) { r = new PageCheck(null, "Не удалось открыть страницу"); }
        finally { if (serial == _linkSerial) CheckingLink = false; }
        if (serial != _linkSerial) return;
        LinkResult = r;
        LinkNote = r.Listing is { } l
            ? $"{SiteNames.Of(l.Source)} · {l.Title}{(l.ModVersion is { } v && !l.Title.Contains(v, StringComparison.OrdinalIgnoreCase) ? " · версия " + v : "")}"
            : r.Problem;
    }

    /// <summary>«Следить на этой странице». Возвращает текст тоста или null, если ничего не выбрано (или мода уже нет).</summary>
    public string? Apply()
    {
        Listing? chosen;
        bool verified;
        if (LinkOk) { chosen = _linkResult!.Listing; verified = true; }
        else if (Selected is { } s && _link.Trim().Length == 0) { chosen = s.Listing; verified = false; }
        else return null;
        if (!_backend.Bind(Card.Id, chosen!, _replaceSource, verified)) return null;
        return $"Следим на {SiteNames.Of(chosen!.Source)}";
    }
}
