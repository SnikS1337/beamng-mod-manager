import { describe, expect, it } from "vitest";
import { coverFlight } from "./coverFlight.svelte";

describe("обложка переезжает в панель", () => {
  it("лететь неоткуда — панель просто открывается, имён нет", async () => {
    let opened = false;
    await coverFlight.open("car", false, () => (opened = true));
    expect(opened).toBe(true);
    expect(coverFlight.from).toBeNull();
    expect(coverFlight.to).toBeNull();
  });

  it("имя сначала у карточки, в новом виде — только у панели, после — ни у кого", async () => {
    let seen: [string | null, string | null] | null = null;
    await coverFlight.open("car", true, () => {
      seen = [coverFlight.from, coverFlight.to];
    });
    expect(seen).toEqual([null, "car"]);
    expect(coverFlight.to).toBeNull();
  });

  it("помнит обложки карточек, забывает пустые и не копит без края", () => {
    coverFlight.remember("a", "http://bmm.localhost/preview/a.png");
    expect(coverFlight.known("a")).toBe("http://bmm.localhost/preview/a.png");
    coverFlight.remember("a", null);
    expect(coverFlight.known("a")).toBeNull();
    expect(coverFlight.known(null)).toBeNull();
    for (let index = 0; index < 450; index += 1) coverFlight.remember(`m${index}`, `u${index}`);
    expect(coverFlight.known("m0")).toBeNull();
    expect(coverFlight.known("m449")).toBe("u449");
  });
});
