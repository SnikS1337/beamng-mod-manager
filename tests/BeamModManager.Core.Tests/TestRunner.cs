namespace BeamModManager.Core.Tests;

/// <summary>
/// Мини-раннер без внешних пакетов (NuGet в окружении разработки недоступен).
/// Когда появится доступ к NuGet, тесты переносятся на xunit почти без изменений.
/// </summary>
public sealed class TestRunner
{
    private int _passed;
    private readonly List<string> _failures = new();

    public void Group(string name) => Console.WriteLine($"\n{name}");

    public void Run(string name, Action body)
    {
        try
        {
            body();
            _passed++;
            Console.WriteLine($"  ok    {name}");
        }
        catch (Exception ex)
        {
            _failures.Add(name);
            Console.WriteLine($"  FAIL  {name}\n        {ex.Message}");
        }
    }

    public int Summary()
    {
        Console.WriteLine($"\nПройдено: {_passed}, провалено: {_failures.Count}");
        foreach (var f in _failures) Console.WriteLine($"  - {f}");
        return _failures.Count == 0 ? 0 : 1;
    }
}

public sealed class AssertionException : Exception
{
    public AssertionException(string message) : base(message) { }
}

public static class Assert
{
    public static void True(bool condition, string? message = null)
    {
        if (!condition) throw new AssertionException(message ?? "ожидалось true");
    }

    public static void False(bool condition, string? message = null)
    {
        if (condition) throw new AssertionException(message ?? "ожидалось false");
    }

    public static void Eq<T>(T expected, T actual, string? message = null)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new AssertionException($"{message ?? "значения различаются"}: ожидалось «{expected}», получено «{actual}»");
    }

    public static void Null<T>(T? value) where T : class
    {
        if (value is not null) throw new AssertionException($"ожидалось null, получено «{value}»");
    }
}
