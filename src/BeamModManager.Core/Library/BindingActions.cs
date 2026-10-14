namespace BeamModManager.Core.Library;

/// <summary>Что снято при «Не проверять этот мод»: нужно, чтобы кнопка «Отменить» в тосте вернула всё как было.</summary>
public sealed record DeclineUndo(string ModId, IReadOnlyList<ModBinding> Bindings, IReadOnlyList<CheckState> Checks, string? DismissedUpdate);

/// <summary>Действия пользователя над привязками, которые должны менять несколько вещей разом.</summary>
public static class BindingActions
{
    private static readonly object Gate = new();

    /// <summary>
    /// «Не проверять этот мод»: снимает все привязки вместе с состоянием проверок и пропущенной версией и запоминает отказ,
    /// чтобы подсказки привязки (<see cref="BindingSuggester"/>) не предлагали страницу заново при следующей сверке.
    /// Возвращает снятое для «Отменить» (<see cref="Undo"/>) или null, если мода нет.
    /// </summary>
    public static DeclineUndo? Decline(ILibraryStore store, string modId)
    {
        lock (Gate)
        {
            if (store.GetMod(modId) is null) return null;
            var mine = store.Bindings().Where(b => b.ModId == modId).ToList();
            var checks = store.Checks().Where(c => c.ModId == modId).ToList();
            var dismissed = store.GetDismissedUpdate(modId);
            foreach (var b in mine) store.RemoveBinding(modId, b.Source);
            store.SetDismissedUpdate(modId, null);
            store.SetCheckingDeclined(modId, true);
            return new DeclineUndo(modId, mine, checks, dismissed);
        }
    }

    /// <summary>То же, что <see cref="Decline"/>, но возвращает только число снятых привязок (0, если мода нет).</summary>
    public static int DeclineChecking(ILibraryStore store, string modId) => Decline(store, modId)?.Bindings.Count ?? 0;

    /// <summary>
    /// «Отменить» после «Не проверять этот мод»: возвращает привязки, состояние проверок и пропущенную версию и снимает отказ.
    /// Если мод тем временем удалён или у него уже есть новая привязка к тому же сайту, она не перезаписывается.
    /// Возвращает false, если мода больше нет.
    /// </summary>
    public static bool Undo(ILibraryStore store, DeclineUndo undo)
    {
        lock (Gate)
        {
            if (store.GetMod(undo.ModId) is null) return false;
            var now = store.Bindings().Where(b => b.ModId == undo.ModId).Select(b => b.Source).ToHashSet(StringComparer.OrdinalIgnoreCase);
            foreach (var b in undo.Bindings.Where(b => !now.Contains(b.Source))) store.UpsertBinding(b);
            foreach (var c in undo.Checks.Where(c => !now.Contains(c.Source))) store.SaveCheck(c);
            if (store.GetDismissedUpdate(undo.ModId) is null && undo.DismissedUpdate is not null) store.SetDismissedUpdate(undo.ModId, undo.DismissedUpdate);
            store.SetCheckingDeclined(undo.ModId, false);
            return true;
        }
    }

    /// <summary>Возвращает моду обычное поведение: подсказки снова предлагаются. Привязку это само не создаёт.</summary>
    public static void ResumeChecking(ILibraryStore store, string modId) => store.SetCheckingDeclined(modId, false);
}
