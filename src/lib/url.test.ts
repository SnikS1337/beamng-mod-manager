import { describe, expect, it } from "vitest";

import { safeExternalUrl } from "./url";

describe("safeExternalUrl", () => {
  it("пропускает только http и https", () => {
    expect(safeExternalUrl("https://modland.net/mod/1")).toBe(
      "https://modland.net/mod/1",
    );
    expect(safeExternalUrl("http://example.org/page")).toBe(
      "http://example.org/page",
    );
  });

  it("отклоняет опасные и локальные схемы", () => {
    for (const value of [
      "javascript:alert(1)",
      "file:///C:/Windows/System32/calc.exe",
      "data:text/html,<script>alert(1)</script>",
      "mailto:test@example.org",
      "//evil.example/path",
      "not a url",
      "",
    ]) {
      expect(safeExternalUrl(value)).toBeNull();
    }
  });
});
