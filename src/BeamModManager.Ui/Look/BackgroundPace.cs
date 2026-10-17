namespace BeamModManager.Ui.Look;

/// <summary>
/// Частота фонового движения (пятна, полоса, огоньки) под частоту экрана (2026-10-17, после боя: «анимации дёрганые»).
/// Фону хватает 24–36 кадров в секунду, но кадры должны ложиться на кадры экрана ровно: 24 на экране 60 Гц — это то
/// 2, то 3 кадра экрана на кадр фона, и плывущие пятна (и всё стекло, сквозь которое они видны) идут рывками.
/// Берётся целая доля частоты экрана, ближайшая к 30 (при равенстве — меньшая, она дешевле): 60 → 30, 144 → 24,
/// 120 → 30, 165 → 33, 75 → 25. Частота экрана неизвестна или ни одна доля не подходит — 30.
/// </summary>
public static class BackgroundPace
{
    public const int Preferred = 30, Min = 20, Max = 36;

    public static int Fps(int screenHz)
    {
        if (screenHz < Min || screenHz > 1000) return Preferred;
        var best = 0;
        for (var d = 1; d <= screenHz / Min; d++)
        {
            if (screenHz % d != 0) continue;
            var r = screenHz / d;
            if (r < Min || r > Max) continue;
            if (best == 0 || Math.Abs(r - Preferred) < Math.Abs(best - Preferred)
                || (Math.Abs(r - Preferred) == Math.Abs(best - Preferred) && r < best)) best = r;
        }
        return best == 0 ? Preferred : best;
    }
}
