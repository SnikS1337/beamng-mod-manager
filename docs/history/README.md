# История проекта

Здесь — материалы прошлых этапов. **Инструкциями их не считать:** они описывают
старую C#/WPF-версию или переезд на Rust/Tauri в момент написания. Как устроено
сейчас — в [README](../../README.md), [INFO](../../INFO.md), [STATUS](../STATUS.md),
[BUILD](../BUILD.md) и [DECISIONS](../DECISIONS.md).

Документы оставлены, потому что объясняют, *почему* многое сделано именно так:
решения переезжали в новую версию вместе с поведением.

| Файл | Что внутри |
| --- | --- |
| [PLAN.md](PLAN.md) | План проекта C#/WPF-версии |
| [PHASE0.md](PHASE0.md) | Что проверялось на Windows в самом начале; откуда взялись фикстуры сайтов |
| [AUDIT.md](AUDIT.md) | Сквозной аудит C#/WPF-версии по темам (состояние, файлы, сайты) |
| [TIME.md](TIME.md) | Как менеджер работает со временем (сроки проверок, часовые пояса) |
| [UPDATE_CHECK_EDGE_CASES.md](UPDATE_CHECK_EDGE_CASES.md) | Краевые случаи проверки обновлений и принятые по ним решения |
| [WINDOWS_SCENARIOS.md](WINDOWS_SCENARIOS.md) | Сценарии ручной проверки на Windows для C#/WPF-версии |
| [DESIGN.md](DESIGN.md), [DESIGN_COMPONENTS.md](DESIGN_COMPONENTS.md), [DESIGN_FLOWS.md](DESIGN_FLOWS.md), [DESIGN_MOTION.md](DESIGN_MOTION.md) | Дизайн C#/WPF-версии: «жидкое стекло», компоненты, сценарии, движение |
| [POC.md](POC.md) | Пробная версия на Rust + Tauri 2 + Svelte 5 перед переездом |
| [specs/](specs/) | Проект переезда на Rust + Tauri |
| [plans/](plans/) | Планы этапов переезда M0–M3 |
| [M1_STATUS.md](M1_STATUS.md) … [M7_STATUS.md](M7_STATUS.md) | Состояние этапов M1–M7 на дату в заголовке |

Исходники макетов и демо-картинок C#/WPF-версии лежат в `legacy/design/`.
