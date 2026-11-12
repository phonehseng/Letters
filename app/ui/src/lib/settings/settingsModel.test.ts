// tests of the Settings model: reading any stored version, the AI Tools section, the privacy summary.

import { describe, expect, it } from "vitest";
import { aiProblems, CURRENT_VERSION, defaultSettings, fromStored, newProvider, privacyItems, withProviderKind } from "./settingsModel";

describe("reading the stored settings", () => {
  it("nothing stored means the defaults, with AI Tools off", () => {
    const settings = fromStored({});
    expect(settings).toEqual(defaultSettings());
    expect(settings.ai.enabled).toBe(false);
    expect(settings.ai.provider).toBeNull();
  });

  it("an older file is brought up to date and keeps what it had", () => {
    const settings = fromStored({ general: { openInNewWindows: true } });
    expect(settings.version).toBe(CURRENT_VERSION);
    expect(settings.general.openInNewWindows).toBe(true);
    expect(settings.files.document).toBe("docx");
  });

  it("a newer version's parts survive", () => {
    const settings = fromStored({ version: 4, templates: { folder: "C:/T" } });
    expect(settings.version).toBe(4);
    expect(settings.templates).toEqual({ folder: "C:/T" });
  });

  it("choices outside the lists fall back, damaged parts too", () => {
    const settings = fromStored({ files: { document: "wpd", picture: "tiff" }, general: "oops", ai: { enabled: "yes" } });
    expect(settings.files.document).toBe("docx");
    expect(settings.files.picture).toBe("tiff");
    expect(settings.general).toEqual(defaultSettings().general);
    expect(settings.ai.enabled).toBe(false);
  });

  it("a stored provider gets any fields it lacks", () => {
    const settings = fromStored({ ai: { enabled: true, provider: { kind: "localServer", address: "http://127.0.0.1:8080" } } });
    expect(settings.ai.provider).toEqual({ ...newProvider("localServer"), address: "http://127.0.0.1:8080" });
  });
});

describe("the AI Tools section", () => {
  const on = (provider: ReturnType<typeof newProvider> | null) => ({ ...defaultSettings(), ai: { enabled: true, provider } });

  it("switching the kind keeps the address", () => {
    const local = on({ ...newProvider("localServer"), address: "https://box.example.org" });
    const remote = withProviderKind(local, "remoteApi");
    expect(remote.ai.provider?.kind).toBe("remoteApi");
    expect(remote.ai.provider?.address).toBe("https://box.example.org");
  });

  it("says what is missing, one sentence each", () => {
    expect(aiProblems(defaultSettings(), false)).toEqual([]);
    expect(aiProblems(on(null), false)).toEqual(["Choose where AI Tools run."]);
    const remote = on({ ...newProvider("remoteApi"), address: "http://api.example.com/v1" });
    expect(aiProblems(remote, false)).toEqual([
      "An online service needs an address that starts with https://.",
      "Type the model name exactly as the service spells it.",
      "Paste the key for this service.",
    ]);
    const ready = on({ ...newProvider("remoteApi"), address: "https://api.example.com/v1", model: "my-remote-model" });
    expect(aiProblems(ready, true)).toEqual([]);
  });

  it("the privacy summary names every way out and that there is no telemetry", () => {
    const items = privacyItems();
    expect(items.map((item) => item.what)).toEqual(["Font downloads", "Google Drive", "AI Tools", "Everything else"]);
    expect(items[3].when).toContain("no telemetry");
  });
});
