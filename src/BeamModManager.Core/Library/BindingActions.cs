namespace BeamModManager.Core.Library;

/// <summary>Действия пользователя над привязками, которые должны менять несколько вещей разом.</summary>
public static class BindingActions
{
    private static readonly object Gate = new();

    /// <summary>
    /// «Не проверять этот мод»: снимает все привязки вместе с состоянием проверок и пропущенной версией и запоминает отказ,
    /// чтобы подсказки привязки (<see cref="BindingSuggester"/>) не предлагали страницу заново при следующей сверке.
    /// Возвращает, сколько привязок снято. Если мода нет, ничего не делает и возвращает 0.
    /// </summary>
    public static int DeclineChecking(ILibraryStore store, string modId)
    {
        lock (Gate)
        {
            if (store.GetMod(modId) is null) return 0;
            var mine = store.Bindings().Where(b => b.ModId == modId).ToList();
            foreach (var b in mine) store.RemoveBinding(modId, b.Source);
            store.SetDismissedUpdate(modId, null);
            store.SetCheckingDeclined(modId, true);
            return mine.Count;
        }
    }

    /// <summary>Возвращает моду обычное поведение: подсказки снова предлагаются. Привязку это само не создаёт.</summary>
    public static void ResumeChecking(ILibraryStore store, string modId) => store.SetCheckingDeclined(modId, false);
}
