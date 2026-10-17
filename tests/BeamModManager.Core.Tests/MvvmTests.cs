using System.Collections.ObjectModel;
using System.Collections.Specialized;
using BeamModManager.Ui.Mvvm;

namespace BeamModManager.Core.Tests;

/// <summary>
/// Основа логики экранов (Ui/Mvvm): списки карточек приводятся к нужному порядку без пересоздания (на этом держатся
/// анимации переезда и то, что фильтр и поиск показывают ровно нужные карточки), команды и уведомления.
/// </summary>
public static class MvvmTests
{
    private sealed class Item(int id)
    {
        public int Id { get; } = id;
        public override string ToString() => Id.ToString();
    }

    private sealed class Probe : Observable
    {
        private int _value;
        public int Value { get => _value; set => Set(ref _value, value); }
    }

    public static void Register(TestRunner t)
    {
        t.Group("Списки и команды экранов");

        t.Run("список приводится к нужному: тот же порядок, те же экземпляры, при любых перестановках (2000 случаев)", () =>
        {
            var rnd = new Random(17);
            var pool = Enumerable.Range(0, 40).Select(i => new Item(i)).ToArray();
            for (var round = 0; round < 2000; round++)
            {
                var start = pool.OrderBy(_ => rnd.Next()).Take(rnd.Next(0, 41)).ToList();
                var wanted = pool.OrderBy(_ => rnd.Next()).Take(rnd.Next(0, 41)).ToList();
                var target = new ObservableCollection<Item>(start);
                var removed = new List<Item>();
                var added = new List<Item>();
                var reset = false;
                target.CollectionChanged += (_, e) =>
                {
                    if (e.Action == NotifyCollectionChangedAction.Reset) reset = true;
                    if (e.Action == NotifyCollectionChangedAction.Remove) removed.AddRange(e.OldItems!.Cast<Item>());
                    if (e.Action == NotifyCollectionChangedAction.Add) added.AddRange(e.NewItems!.Cast<Item>());
                };

                target.SyncTo(wanted);

                Assert.True(target.Count == wanted.Count && target.Zip(wanted).All(p => ReferenceEquals(p.First, p.Second)),
                    $"случай {round}: [{string.Join(",", start)}] → [{string.Join(",", wanted)}], вышло [{string.Join(",", target)}]");
                Assert.False(reset, "список не пересоздаётся целиком");
                // оставшиеся карточки только переезжают: не исчезают и не появляются заново (иначе мигнут)
                Assert.False(removed.Any(wanted.Contains), $"случай {round}: убрана карточка, которая остаётся");
                Assert.False(added.Any(start.Contains), $"случай {round}: заново вставлена карточка, которая уже была");
            }
        });

        t.Run("список: тот же порядок — ни одного изменения; пустой ↔ полный", () =>
        {
            var items = Enumerable.Range(0, 5).Select(i => new Item(i)).ToList();
            var target = new ObservableCollection<Item>(items);
            var events = 0;
            target.CollectionChanged += (_, _) => events++;
            target.SyncTo(items);
            Assert.Eq(0, events, "ничего не изменилось — окно не трогается");
            target.SyncTo(Array.Empty<Item>());
            Assert.Eq(0, target.Count);
            target.SyncTo(items);
            Assert.True(target.SequenceEqual(items));
        });

        t.Run("команда не выполняется, когда нельзя; «обновить» сообщает окну", () =>
        {
            var can = false;
            var runs = 0;
            var cmd = new RelayCommand(() => runs++, () => can);
            var changed = 0;
            cmd.CanExecuteChanged += (_, _) => changed++;
            cmd.Execute(null);
            Assert.Eq(0, runs, "нельзя — не выполнилась");
            can = true;
            cmd.Refresh();
            Assert.Eq(1, changed);
            Assert.True(cmd.CanExecute(null));
            cmd.Execute(null);
            Assert.Eq(1, runs);
        });

        t.Run("уведомление только при настоящем изменении", () =>
        {
            var p = new Probe();
            var names = new List<string?>();
            p.PropertyChanged += (_, e) => names.Add(e.PropertyName);
            p.Value = 0;
            p.Value = 5;
            p.Value = 5;
            Assert.Eq("Value", string.Join(",", names), "одно уведомление с именем свойства");
        });
    }
}
