// Встраивается в страницу мода в окне скачивания (см. download_window.rs).
// Окно скачивания не открывает новых окон: ссылка «Download» с target=_blank
// (ModsGaming), кнопка «Continue to External Site» (Modland) и window.open вели
// в никуда — новое окно запрещено, а в своё программа не переходила. Такие переходы
// к файлу делаем в этом же окне. Список файловых хостингов подставляет программа.
(() => {
  const FILE_HOSTS = __FILE_HOSTS__;
  const downloadPath =
    /\/download_file\.php|\/download(?:[/?]|$)|\/d\/[a-z0-9]+|\/load\/0-0-\d+-\d+-20(?:[/?#]|$)/i;
  const downloadWords = /download|скачать|continue to external|перейти к загрузке/i;

  const fileHost = (host) => {
    const name = String(host || "").toLowerCase().replace(/^www\./, "");
    return FILE_HOSTS.some((known) => name === known || name.endsWith("." + known));
  };

  /** Куда вести в этом окне вместо нового, или null — оставить как есть. */
  const target = (href, words) => {
    let next;
    try { next = new URL(href, location.href); } catch { return null; }
    if (next.protocol !== "https:" && next.protocol !== "http:") return null;
    if (fileHost(next.host)) return next.href;
    const sameSite = next.host.replace(/^www\./, "") === location.host.replace(/^www\./, "");
    if (!sameSite) return null;
    if (downloadPath.test(next.pathname + next.search)) return next.href;
    // Переход через свою страницу-переадресацию на файловый хостинг (?url=modsfire…).
    for (const value of next.searchParams.values()) {
      try { if (fileHost(new URL(value).host)) return next.href; } catch {}
    }
    if (words && downloadWords.test(words)) return next.href;
    return null;
  };

  document.addEventListener("click", (event) => {
    const anchor = event.target && event.target.closest
      ? event.target.closest("a[target=_blank], a[target=_new]")
      : null;
    if (!anchor) return;
    const words = (anchor.textContent || "") + " " + (anchor.getAttribute("title") || "");
    const next = target(anchor.getAttribute("href") || anchor.href, words);
    if (!next) return;
    event.preventDefault();
    location.href = next;
  }, true);

  // Кнопка-зеркало ModHub — форма POST с target=_blank. Новое окно открыло бы адрес
  // без данных формы, поэтому такие формы своего сайта отправляем в это же окно.
  document.addEventListener("click", (event) => {
    const button = event.target && event.target.closest
      ? event.target.closest("button, input[type=submit]")
      : null;
    const form = button && button.form;
    if (!form) return;
    let action;
    try { action = new URL(form.getAttribute("action") || location.href, location.href); } catch { return; }
    if (action.host !== location.host) return;
    if (!downloadPath.test(action.pathname + action.search)) return;
    if (button.getAttribute("formtarget") === "_blank") button.setAttribute("formtarget", "_self");
    if (form.getAttribute("target") === "_blank") form.setAttribute("target", "_self");
  }, true);

  // window.open к файлу — тоже в этом окне.
  const open = window.open;
  window.open = function (url, name, features) {
    const next = url ? target(String(url), "") : null;
    if (next) {
      location.href = next;
      return null;
    }
    return open.apply(window, arguments);
  };
})();
