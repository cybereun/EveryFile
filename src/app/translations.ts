export type Locale = "en" | "ko";

export const defaultLocale: Locale = "ko";

export const productTranslations: Record<Locale, { tagline: string }> = {
  ko: {
    tagline: "\uD30C\uC77C\uC744 \uCC3E\uB294 \uAC00\uC7A5 \uBE60\uB978 \uBC29\uBC95",
  },
  en: {
    tagline: "The fastest way to find files.",
  },
};
