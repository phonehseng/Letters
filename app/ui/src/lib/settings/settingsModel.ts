// what the Settings window edits, without any dom: the stored choices
// (shell-core/src/settings_store), reading an older file, switching the
// AI Tools provider, checking what was typed, and the privacy summary.
// secrets are never part of this model: an ai key goes straight from its
// field to the system's password store.

export const CURRENT_VERSION = 1;

export interface LocalServerConfig {
  kind: "localServer";
  address: string;
  model: string | null;
  contextTokens: number;
  embeddings: boolean;
  thinkingTokens: number;
  readTimeoutSeconds: number;
}

export interface RemoteApiConfig {
  kind: "remoteApi";
  address: string;
  model: string;
  embeddingModel: string | null;
  contextTokens: number;
  thinkingTokens: number;
  keyName: string;
  readTimeoutSeconds: number;
}

export type ProviderConfig = LocalServerConfig | RemoteApiConfig;

export interface AppSettings {
  version: number;
  general: { defaultZoom: string; openInNewWindows: boolean };
  files: { document: "docx" | "odt"; wordExportStyle: "exactMatch" | "easyToEdit"; picture: "png" | "jpeg" | "tiff" };
  scanning: { ocrLanguage: string };
  ai: { enabled: boolean; provider: ProviderConfig | null };
  aiServer: { executable: string; modelFile: string };
  /** anything a newer Letters wrote; kept and written back */
  [other: string]: unknown;
}

export function defaultSettings(): AppSettings {
  return {
    version: CURRENT_VERSION,
    general: { defaultZoom: "fitWidth", openInNewWindows: false },
    files: { document: "docx", wordExportStyle: "exactMatch", picture: "png" },
    scanning: { ocrLanguage: "eng" },
    ai: { enabled: false, provider: null },
    aiServer: { executable: "", modelFile: "" },
  };
}

function pick<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function section(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

/**
 * reads whatever the shell stored, of any version: missing parts get
 * their defaults, choices outside the lists fall back, an older file is
 * brought up to the current version, and unknown parts are kept.
 */
export function fromStored(raw: unknown): AppSettings {
  const stored = section(raw);
  const base = defaultSettings();
  const general = section(stored.general);
  const files = section(stored.files);
  const scanning = section(stored.scanning);
  const ai = section(stored.ai);
  const server = section(stored.aiServer);
  return {
    ...stored,
    version: Math.max(Number(stored.version) || 0, CURRENT_VERSION),
    general: {
      defaultZoom: typeof general.defaultZoom === "string" ? general.defaultZoom : base.general.defaultZoom,
      openInNewWindows: general.openInNewWindows === true,
    },
    files: {
      document: pick(files.document, ["docx", "odt"], "docx"),
      wordExportStyle: pick(files.wordExportStyle, ["exactMatch", "easyToEdit"], "exactMatch"),
      picture: pick(files.picture, ["png", "jpeg", "tiff"], "png"),
    },
    scanning: { ocrLanguage: typeof scanning.ocrLanguage === "string" && scanning.ocrLanguage ? scanning.ocrLanguage : "eng" },
    // a provider is only kept when the switch was really on or a provider was chosen; the switch starts off
    ai: { enabled: ai.enabled === true, provider: providerFrom(ai.provider) },
    aiServer: {
      executable: typeof server.executable === "string" ? server.executable : "",
      modelFile: typeof server.modelFile === "string" ? server.modelFile : "",
    },
  };
}

function providerFrom(value: unknown): ProviderConfig | null {
  const stored = section(value);
  if (stored.kind === "localServer") return { ...newProvider("localServer"), ...stored } as LocalServerConfig;
  if (stored.kind === "remoteApi") return { ...newProvider("remoteApi"), ...stored } as RemoteApiConfig;
  return null;
}

/** a provider of a kind with the engine's defaults and nothing filled in. */
export function newProvider(kind: ProviderConfig["kind"]): ProviderConfig {
  if (kind === "localServer") {
    return { kind, address: "", model: null, contextTokens: 4096, embeddings: false, thinkingTokens: 0, readTimeoutSeconds: 300 };
  }
  return { kind, address: "", model: "", embeddingModel: null, contextTokens: 8192, thinkingTokens: 0, keyName: "letters-ai-remote", readTimeoutSeconds: 120 };
}

/** switches the provider kind, keeping the address and context size typed so far. */
export function withProviderKind(settings: AppSettings, kind: ProviderConfig["kind"]): AppSettings {
  const before = settings.ai.provider;
  if (before?.kind === kind) return settings;
  const provider = newProvider(kind);
  if (before) {
    provider.address = before.address;
    provider.contextTokens = before.contextTokens;
  }
  return { ...settings, ai: { ...settings.ai, provider } };
}

/** what is missing or wrong in the AI Tools section, one sentence each. */
export function aiProblems(settings: AppSettings, hasKey: boolean): string[] {
  if (!settings.ai.enabled) return [];
  const provider = settings.ai.provider;
  if (!provider) return ["Choose where AI Tools run."];
  const problems: string[] = [];
  const address = provider.address.trim();
  if (!address) problems.push("Type the server’s address.");
  if (provider.kind === "remoteApi") {
    if (address && !address.startsWith("https://")) problems.push("An online service needs an address that starts with https://.");
    if (!provider.model.trim()) problems.push("Type the model name exactly as the service spells it.");
    if (!hasKey) problems.push("Paste the key for this service.");
  } else if (address && !/^https?:\/\//.test(address)) {
    problems.push("The address starts with http:// or https://.");
  }
  if (!(provider.contextTokens >= 512 && provider.contextTokens <= 1_000_000)) problems.push("Choose a context size between 512 and 1,000,000 tokens.");
  return problems;
}

/** the privacy summary: everything that can leave this computer, and when. */
export interface PrivacyItem {
  what: string;
  when: string;
}

export function privacyItems(): PrivacyItem[] {
  return [
    {
      what: "Font downloads",
      when: "Only when you choose to download a font in the font library: Letters asks the font’s download address for the font file. Nothing about your documents is sent.",
    },
    {
      what: "Google Drive",
      when: "Only when you choose Upload in Save to Google Drive and confirm: the one file is sent to your Google Drive. Signing in happens in your browser.",
    },
    {
      what: "AI Tools",
      when: "Off until you turn them on. With a model on this computer nothing leaves. With an online service, Letters shows what it would send and where before each request, and sends only after you choose Send.",
    },
    {
      what: "Everything else",
      when: "Stays on this computer. Letters has no telemetry: it doesn’t report how you use it, crashes or anything else.",
    },
  ];
}

/** the zoom choices of General. */
export const ZOOM_CHOICES = [
  { value: "fitWidth", label: "Fit Width" },
  { value: "100", label: "100%" },
  { value: "125", label: "125%" },
  { value: "150", label: "150%" },
] as const;

/** a few common text recognition languages; the code is what the OCR engine reads. */
export const OCR_LANGUAGES = [
  { value: "eng", label: "English" },
  { value: "fra", label: "French" },
  { value: "deu", label: "German" },
  { value: "spa", label: "Spanish" },
  { value: "ita", label: "Italian" },
  { value: "por", label: "Portuguese" },
] as const;
