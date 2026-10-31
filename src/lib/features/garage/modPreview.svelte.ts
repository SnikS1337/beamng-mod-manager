/** Что показать картинкой мода в панели. */
export interface PreviewSource {
  id: string | null;
  /** Картинка уже есть в данных мода. */
  known: string | null;
  /** Дополнение для машин игры: картинки нет, показываем значок. */
  skip: boolean;
}

/**
 * Картинка мода в панели: известная сразу или запрошенная у программы.
 *
 * Раньше эффект панели и писал картинку, и читал её («уже есть — не запрашиваем»),
 * то есть зависел от собственной записи: пришёл ответ → эффект сбрасывал картинку →
 * снова запрос → снова ответ… Окно зависало, а память WebView росла до гигабайтов
 * (после «Вернуть» у копии мода). Теперь панель передаёт сюда только данные мода
 * (`untrack`), а решение «запрашивать или нет» картинку не читает.
 */
export class ModPreview {
  url = $state<string | null>(null);
  private generation = 0;

  constructor(private readonly load: (id: string) => Promise<string | null>) {}

  /** Мод в панели сменился или обновился. Возвращает запрос, если он ушёл. */
  update(source: PreviewSource): Promise<void> | null {
    const generation = ++this.generation;
    this.url = source.skip ? null : source.known;
    if (!source.id || source.skip || source.known) return null;
    return this.load(source.id)
      .then((url) => {
        // Ответ для прежнего мода не должен лечь на новый.
        if (generation === this.generation) this.url = url;
      })
      .catch(() => {});
  }
}
