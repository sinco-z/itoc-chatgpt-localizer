(() => {
  "use strict";

  const VERSION = "0.1.4-preview.1";
  const LOCALE = "zh-CN";
  const I18N_CONFIG_ID = "72216192";

  if (globalThis.__ITOC_ZH_PREVIEW__?.version === VERSION) {
    return globalThis.__ITOC_ZH_PREVIEW__;
  }

  const state = {
    version: VERSION,
    locale: LOCALE,
    patchedClients: 0,
    installedAt: new Date().toISOString(),
  };
  globalThis.__ITOC_ZH_PREVIEW__ = state;

  const defineLocale = (target, key, value) => {
    try {
      Object.defineProperty(target, key, {
        configurable: true,
        get: () => value,
      });
    } catch (_) {}
  };

  defineLocale(Navigator.prototype, "language", LOCALE);
  defineLocale(Navigator.prototype, "languages", [LOCALE, "zh", "en-US", "en"]);

  try {
    document.documentElement?.setAttribute("lang", LOCALE);
    localStorage.setItem("localeOverride", LOCALE);
  } catch (_) {}

  const enableI18n = (config) => {
    if (!config || typeof config !== "object") return config;
    const value = config.value && typeof config.value === "object" ? config.value : {};
    try {
      config.value = { ...value, enable_i18n: true, locale_source: "SYSTEM" };
    } catch (_) {}

    if (typeof config.get === "function" && !config.__itocZhGetPatched) {
      const originalGet = config.get.bind(config);
      config.get = (key, fallback) => {
        if (key === "enable_i18n") return true;
        if (key === "locale_source") return "SYSTEM";
        return originalGet(key, fallback);
      };
      config.__itocZhGetPatched = VERSION;
    }
    return config;
  };

  const patchClient = (client) => {
    if (!client || typeof client !== "object" || typeof client.getDynamicConfig !== "function") {
      return false;
    }
    if (client.__itocZhClientPatched === VERSION) return true;

    const originalGetDynamicConfig = client.getDynamicConfig.bind(client);
    client.getDynamicConfig = (name, options) => {
      const result = originalGetDynamicConfig(name, options);
      return String(name) === I18N_CONFIG_ID ? enableI18n(result) : result;
    };
    client.__itocZhClientPatched = VERSION;
    try {
      enableI18n(client.getDynamicConfig(I18N_CONFIG_ID, { disableExposureLog: true }));
    } catch (_) {}
    state.patchedClients += 1;
    return true;
  };

  const patchRoot = (root) => {
    if (!root || typeof root !== "object") return;
    const candidates = [];
    try {
      candidates.push(root.firstInstance);
      if (typeof root.instance === "function") candidates.push(root.instance());
      if (root.instances && typeof root.instances === "object") {
        candidates.push(...Object.values(root.instances));
      }
    } catch (_) {}
    candidates.forEach(patchClient);

    for (const key of ["firstInstance", "instance"]) {
      const marker = `__itocZhSetter_${key}`;
      if (root[marker]) continue;
      let current;
      try {
        current = root[key];
        Object.defineProperty(root, key, {
          configurable: true,
          get: () => current,
          set: (next) => {
            current = next;
            try {
              patchClient(key === "instance" && typeof next === "function" ? next.call(root) : next);
            } catch (_) {}
          },
        });
        root[marker] = VERSION;
      } catch (_) {}
    }
  };

  const installRootCapture = () => {
    const descriptor = Object.getOwnPropertyDescriptor(globalThis, "__STATSIG__");
    if (descriptor && descriptor.configurable === false) {
      patchRoot(globalThis.__STATSIG__);
      return;
    }
    let current = globalThis.__STATSIG__;
    patchRoot(current);
    try {
      Object.defineProperty(globalThis, "__STATSIG__", {
        configurable: true,
        get: () => current,
        set: (next) => {
          current = next;
          patchRoot(next);
        },
      });
    } catch (_) {}
  };

  installRootCapture();
  let attempts = 0;
  const timer = setInterval(() => {
    attempts += 1;
    patchRoot(globalThis.__STATSIG__);
    if (attempts >= 200) clearInterval(timer);
  }, 50);

  return state;
})();
