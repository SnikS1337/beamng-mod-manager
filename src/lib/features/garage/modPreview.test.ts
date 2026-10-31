import { describe, expect, it } from "vitest";
import { ModPreview } from "./modPreview.svelte";

describe("картинка мода в панели", () => {
  it("запрашивается один раз; ответ не вызывает новый запрос", async () => {
    let calls = 0;
    const preview = new ModPreview(async () => {
      calls += 1;
      return "http://bmm.localhost/preview/a.jpg";
    });
    await preview.update({ id: "weather_fx_modland.zip", known: null, skip: false });
    expect(preview.url).toBe("http://bmm.localhost/preview/a.jpg");
    expect(calls).toBe(1);
  });

  it("известная картинка — без запроса; у дополнения — без картинки", () => {
    let calls = 0;
    const preview = new ModPreview(async () => {
      calls += 1;
      return "x";
    });
    expect(preview.update({ id: "a", known: "http://k", skip: false })).toBeNull();
    expect(preview.url).toBe("http://k");
    expect(preview.update({ id: "b", known: "http://k", skip: true })).toBeNull();
    expect(preview.url).toBeNull();
    expect(calls).toBe(0);
  });

  it("поздний ответ для прежнего мода не ложится на новый", async () => {
    const answers: Array<(url: string) => void> = [];
    const preview = new ModPreview(
      (id) => new Promise((resolve) => answers.push((url) => resolve(`${id}:${url}`))),
    );
    const first = preview.update({ id: "old", known: null, skip: false });
    const second = preview.update({ id: "new", known: null, skip: false });
    answers[1]!("fresh");
    answers[0]!("late");
    await Promise.all([first, second]);
    expect(preview.url).toBe("new:fresh");
  });
});
