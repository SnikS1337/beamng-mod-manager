/**
 * Что сделать перед закрытием главного окна. Обработчик закрытия у окна один: было
 * два независимых (раздел «Настройки» и вопрос про загрузки), и каждый сам закрывал
 * окно после себя — «Не закрывать» не помогало, окно закрывал второй.
 */
type Hook = () => Promise<void> | void;

const hooks = new Set<Hook>();

/** Добавить дело перед закрытием. Возвращает функцию, которая его убирает. */
export function beforeClose(hook: Hook): () => void {
  hooks.add(hook);
  return () => {
    hooks.delete(hook);
  };
}

/** Выполнить всё по очереди; сбой одного не мешает остальным. */
export async function runCloseHooks(): Promise<void> {
  for (const hook of [...hooks]) {
    try {
      await hook();
    } catch {
      // Окно всё равно закрывается: дело не удалось — остальные важнее.
    }
  }
}
