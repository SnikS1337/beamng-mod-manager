/**
 * Кнопка «Пройти проверку» сайта. Окно проверки одно на программу: пока проходят
 * проверку одного сайта, кнопки остальных недоступны, но «Проходим проверку…» пишет
 * только нажатая — раньше надпись менялась у обоих сайтов сразу.
 */
export function passButton(
  passing: string | null,
  site: string,
  name: string,
): { text: string; disabled: boolean } {
  return {
    text: passing === site ? "Проходим проверку…" : `Пройти проверку ${name}`,
    disabled: passing !== null,
  };
}
