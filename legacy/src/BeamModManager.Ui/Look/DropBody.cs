namespace BeamModManager.Ui.Look;

/// <summary>
/// Капля в пути — одно тело (вариант «В», утверждён 2026-10-17): «голова» (<c>lead</c>) догоняет новый пункт быстро,
/// «хвост» (<c>trail</c>) — медленнее; капля рисуется оболочкой от хвоста до головы, поэтому в пути она тянется, а не рвётся
/// на две. Поперёк она при этом чуть сжимается (объём «сохраняется»), а пока едет — становится линзой
/// (<see cref="Motion"/>) и приподнимается (<see cref="Lift"/>).
/// </summary>
/// <param name="Start">Начало оболочки по оси движения (верх или левый край).</param>
/// <param name="Length">Длина оболочки по оси движения: размер капли плюс расстояние между головой и хвостом.</param>
/// <param name="Across">Масштаб поперёк движения: 1 в покое, до 0,84 при сильном растяжении.</param>
/// <param name="Motion">0 — капля стоит (плашка), 1 — в пути (линза).</param>
/// <param name="Lift">Масштаб «подъёма» в пути: 1 в покое, до 1,08.</param>
public readonly record struct DropBody(double Start, double Length, double Across, double Motion, double Lift)
{
    public const double MaxSquash = 0.16, MaxLift = 0.08, MaxBulge = 0.06;

    public static DropBody Of(double lead, double trail, double size, double leadVelocity)
    {
        lead = Finite(lead); trail = Finite(trail);
        size = Math.Max(1, Finite(size));
        var sep = Math.Abs(lead - trail);
        var m = MotionOf(sep, leadVelocity);
        return new DropBody(Math.Min(lead, trail), size + sep, 1 - MaxSquash * Math.Min(1, sep / size), m, 1 + MaxLift * m);
    }

    /// <summary>
    /// Капля упирается в стенку островка, а не уходит за неё (2026-10-18: из крайнего правого фильтра в крайний левый перелёт
    /// пружины выносил каплю за край, и там её обрезало). То, что на экране (с учётом масштаба <paramref name="scale"/> вокруг
    /// середины), обрезается стенками <paramref name="min"/>–<paramref name="max"/>: дальний край остаётся, ближний прижат к стенке,
    /// капля короче — сплющилась об неё — и поперёк чуть толще (до <see cref="MaxBulge"/>).
    /// </summary>
    public DropBody Within(double min, double max, double scale)
    {
        if (!double.IsFinite(min) || !double.IsFinite(max) || max - min < 1) return this;
        scale = double.IsFinite(scale) && scale > 0 ? scale : 1;
        var c = Start + Length / 2;
        var half = Length * scale / 2;
        var lo = Math.Max(c - half, min);
        var hi = Math.Min(c + half, max);
        if (hi - lo >= 2 * half - 1e-9) return this;
        if (hi - lo < 1)
        {
            // целиком за стенкой (не бывает, но мало ли): прижимаем к ближней
            var w = Math.Min(2 * half, max - min);
            if (c < min) { lo = min; hi = min + w; } else { hi = max; lo = max - w; }
        }
        var length = (hi - lo) / scale;
        // толще ровно настолько, насколько «объём» ужат (корень — мягче), но не больше предела
        var bulge = Math.Min(1 + MaxBulge, Math.Sqrt(2 * half / Math.Max(1, hi - lo)));
        return this with { Start = (lo + hi) / 2 - length / 2, Length = length, Across = Across * bulge };
    }

    /// <summary>Насколько капля «в пути»: от разрыва головы и хвоста и от скорости; стоит — 0.</summary>
    public static double MotionOf(double separation, double velocity)
    {
        var m = Math.Abs(Finite(separation)) / 10 + Math.Abs(Finite(velocity)) / 600;
        return Math.Clamp(m, 0, 1);
    }

    private static double Finite(double v) => double.IsFinite(v) ? v : 0;
}
