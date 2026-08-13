(() => {
  "use strict";

  const VERSION = "0.1.1-preview.1";
  const LOCALE = "zh-CN";

  if (globalThis.__ITOC_ZH_PREVIEW__?.version === VERSION) {
    return globalThis.__ITOC_ZH_PREVIEW__;
  }

  const defineLocale = (target, key, value) => {
    try {
      Object.defineProperty(target, key, {
        configurable: true,
        get: () => value,
      });
    } catch (_) {
      // A frozen Navigator implementation must not stop the remaining patch.
    }
  };

  defineLocale(Navigator.prototype, "language", LOCALE);
  defineLocale(Navigator.prototype, "languages", [LOCALE, "zh", "en-US"]);

  try {
    document.documentElement?.setAttribute("lang", LOCALE);
    localStorage.setItem("localeOverride", LOCALE);
  } catch (_) {
    // Storage may be unavailable during very early document initialization.
  }

  const state = {
    version: VERSION,
    locale: LOCALE,
    installedAt: new Date().toISOString(),
  };
  globalThis.__ITOC_ZH_PREVIEW__ = state;

  return state;
})();
