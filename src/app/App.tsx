import { useState } from "react";
import { defaultLocale, productTranslations, type Locale } from "./translations";

export function App() {
  const [locale, setLocale] = useState<Locale>(defaultLocale);
  const { tagline } = productTranslations[locale];

  return (
    <main>
      <label htmlFor="language">Language</label>
      <select
        id="language"
        aria-label="Language"
        value={locale}
        onChange={(event) => setLocale(event.target.value as Locale)}
      >
        <option value="ko">{"\uD55C\uAD6D\uC5B4"}</option>
        <option value="en">English</option>
      </select>
      <h1>EveryFile</h1>
      <p>{tagline}</p>
    </main>
  );
}
