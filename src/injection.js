(() => {
  "use strict";

  const VERSION = "0.1.0-preview.1";
  const LOCALE = "zh-CN";
  // This dynamic-config identifier is an undocumented implementation detail.
  // Keep it isolated so a future official app update can disable this preview
  // without changing the launcher or touching user data.
  const I18N_DYNAMIC_CONFIG = "72216192";

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

  const localizeConfig = (config) => {
    if (!config || typeof config !== "object") return config;
    const value = config.value;
    if (!value || typeof value !== "object") return config;

    const localizedValue = {
      ...value,
      enable_i18n: true,
      locale_source: "SYSTEM",
    };

    try {
      config.value = localizedValue;
      if (config.value === localizedValue) return config;
    } catch (_) {
      // Some Statsig result objects are immutable.
    }
    return { ...config, value: localizedValue };
  };

  const patchClient = (client) => {
    if (!client || typeof client !== "object") return false;
    if (client.__itocZhI18nPatch === VERSION) return true;
    if (typeof client.getDynamicConfig !== "function") return false;

    const original = client.getDynamicConfig;
    client.getDynamicConfig = function itocLocalizedDynamicConfig(name, ...args) {
      const result = original.call(this, name, ...args);
      return String(name) === I18N_DYNAMIC_CONFIG ? localizeConfig(result) : result;
    };
    client.__itocZhI18nPatch = VERSION;

    try {
      localizeConfig(
        client.getDynamicConfig(I18N_DYNAMIC_CONFIG, { disableExposureLog: true }),
      );
    } catch (_) {
      // The wrapper still applies to the app's next dynamic-config read.
    }
    return true;
  };

  const statsigClients = () => {
    const roots = [globalThis.__STATSIG__, globalThis.Statsig, globalThis.statsig]
      .filter((item) => item && typeof item === "object");
    const clients = [];
    for (const root of roots) {
      clients.push(root, root.firstInstance);
      try {
        if (typeof root.instance === "function") clients.push(root.instance());
      } catch (_) {
        // Ignore a partially initialized singleton.
      }
      if (root.instances && typeof root.instances === "object") {
        clients.push(...Object.values(root.instances));
      }
    }
    return clients.filter(
      (client, index, all) => client && typeof client === "object" && all.indexOf(client) === index,
    );
  };

  const patchAvailableClients = () => {
    let patched = 0;
    for (const client of statsigClients()) {
      if (patchClient(client)) patched += 1;
    }
    return patched;
  };

  const state = {
    version: VERSION,
    locale: LOCALE,
    patchedClients: patchAvailableClients(),
    installedAt: new Date().toISOString(),
  };
  globalThis.__ITOC_ZH_PREVIEW__ = state;

  let attempts = 0;
  const timer = setInterval(() => {
    attempts += 1;
    state.patchedClients = Math.max(state.patchedClients, patchAvailableClients());
    if (state.patchedClients > 0 || attempts >= 150) clearInterval(timer);
  }, 100);

  return state;
})();

