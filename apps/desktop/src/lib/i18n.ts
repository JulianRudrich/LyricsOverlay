import de from "../locales/de.json";
import en from "../locales/en.json";

export type MessageKey = keyof typeof en;
export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

const catalogs: Record<"de" | "en", Record<MessageKey, string>> = { de, en };

/** Returns a translator for one language. Unknown placeholders stay visible. */
export function translator(language: "de" | "en"): Translate {
  const catalog = catalogs[language];
  return (key, values) => {
    const template = catalog[key] ?? en[key] ?? key;
    if (!values) return template;
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in values ? String(values[name]) : match,
    );
  };
}

/** Formats a number with the language's decimal separator. */
export function formatNumber(language: "de" | "en", value: number, decimals = 0): string {
  return new Intl.NumberFormat(language === "de" ? "de-DE" : "en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value);
}

/** Formats a signed number of seconds, such as "+0,25 s". */
export function formatOffset(language: "de" | "en", seconds: number): string {
  const sign = seconds > 0 ? "+" : seconds < 0 ? "−" : "±";
  return `${sign}${formatNumber(language, Math.abs(seconds), 2)} s`;
}

export const allKeys = {
  de: Object.keys(de).sort(),
  en: Object.keys(en).sort(),
};
