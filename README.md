# BeamNG Mod Manager

Менеджер модов для BeamNG.drive (Steam, Windows) для личного использования.
Ведёт библиотеку установленных модов, следит за обновлениями, ищет по нескольким
сайтам сразу и по желанию помогает скачивать.

> **Статус:** фундамент. Кода пока нет, есть план, структура и правила. Подробности в
> [docs/PLAN.md](docs/PLAN.md).

## Принципы

- **Менеджер первичен, загрузка опциональна.** Всё, что связано со скачиванием, включается
  отдельной настройкой «Разрешить загрузку модов» и выключено по умолчанию.
- **Не обходим платные стены, капчи, таймеры и рекламу.** Все клики на странице
  файлообменника делает сам пользователь.
- **Платный мод показываем, но не скачиваем:** метка и ссылка на страницу автора.
- **Точность важнее полноты при объединении дублей.**
- **Один адаптер на каждый сайт.** Сбой одного источника не ломает остальные.
- **Служебные файлы игры (`db.json`) пока только читаем.**

## Стек

.NET 8 (ядро, библиотека классов), WPF + WPF-UI (интерфейс), SQLite (FTS5), AngleSharp
(разбор страниц), WebView2 (окно загрузки), SharpCompress (RAR/7z).

## Структура

```
src/
  BeamModManager.Core/       ядро без интерфейса: адаптеры, дубли, обновления, установка, SQLite
  BeamModManager.App/        WPF-интерфейс и окно загрузки (WebView2), только Windows
tests/
  BeamModManager.Core.Tests/ тесты ядра
  fixtures/                  сохранённые HTML-страницы сайтов для тестов адаптеров
docs/
  PLAN.md                    план проекта
  PHASE0.md                  чек-лист проверок, которые нужны на Windows
```

Ядро не зависит от Windows: его можно разрабатывать и тестировать на Mac и Linux на
фальшивой папке `mods`. Интерфейс и окно загрузки собираются и запускаются только на Windows.

## Первые шаги (когда появится .NET 8 SDK)

```powershell
dotnet new sln -n BeamModManager
dotnet new classlib -n BeamModManager.Core -o src/BeamModManager.Core
dotnet new xunit -n BeamModManager.Core.Tests -o tests/BeamModManager.Core.Tests
dotnet sln add src/BeamModManager.Core tests/BeamModManager.Core.Tests
dotnet add tests/BeamModManager.Core.Tests reference src/BeamModManager.Core
```

Проект `BeamModManager.App` (WPF) создаётся позже, на Windows.

## Лицензия

Приватный репозиторий для личного использования, лицензия не выбрана (все права защищены).
