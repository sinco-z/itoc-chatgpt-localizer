(() => {
  "use strict";

  const VERSION = "0.1.9-preview.5";
  const LOCALE = "zh-CN";
  const I18N_CONFIG_ID = "72216192";
  const RELOAD_MARKER = "itoc.zh.locale.reload.v1";
  const VOICE_BUTTON_ID = "itoc-voice-typing-button";
  const VOICE_BINDING = "__itocVoiceTyping";

  if (globalThis.__ITOC_ZH_PREVIEW__?.version === VERSION) {
    return globalThis.__ITOC_ZH_PREVIEW__;
  }

  const state = {
    version: VERSION,
    locale: LOCALE,
    patchedClients: 0,
    bridgeAvailable: false,
    settingStatus: "pending",
    settingError: null,
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
  } catch (_) {}

  const waitForBridge = () =>
    new Promise((resolve) => {
      const startedAt = Date.now();
      const timer = setInterval(() => {
        const bridge = globalThis.electronBridge;
        if (bridge && typeof bridge.sendMessageFromView === "function") {
          clearInterval(timer);
          resolve(bridge);
        } else if (Date.now() - startedAt >= 8000) {
          clearInterval(timer);
          resolve(null);
        }
      }, 50);
    });

  const callSettingApi = (bridge, method, params) =>
    new Promise((resolve, reject) => {
      const requestId = `itoc-locale-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      let timeout;
      const cleanup = () => {
        clearTimeout(timeout);
        removeEventListener("message", onMessage);
      };
      const onMessage = (event) => {
        const message = event?.data;
        if (message?.type !== "fetch-response" || message.requestId !== requestId) return;
        cleanup();
        if (
          message.responseType !== "success" ||
          (typeof message.status === "number" && (message.status < 200 || message.status >= 300))
        ) {
          reject(new Error(message.error || `${method} failed`));
          return;
        }
        try {
          resolve(JSON.parse(message.bodyJsonString || "null"));
        } catch (error) {
          reject(error);
        }
      };
      addEventListener("message", onMessage);
      timeout = setTimeout(() => {
        cleanup();
        reject(new Error(`${method} timed out`));
      }, 6000);
      Promise.resolve(
        bridge.sendMessageFromView({
          type: "fetch",
          requestId,
          method: "POST",
          url: `vscode://codex/${method}`,
          // The desktop fetch bridge forwards this JSON body directly to the
          // handler. Its own settings client sends { key, value }, not a
          // React-query-style { params: { key, value } } wrapper.
          body: JSON.stringify(params),
        }),
      ).catch((error) => {
        cleanup();
        reject(error);
      });
    });

  const reloadOnceForLocaleHooks = () => {
    let shouldReload = true;
    try {
      if (sessionStorage.getItem(RELOAD_MARKER) === LOCALE) {
        sessionStorage.removeItem(RELOAD_MARKER);
        shouldReload = false;
      } else {
        sessionStorage.setItem(RELOAD_MARKER, LOCALE);
      }
    } catch (_) {}
    if (shouldReload) setTimeout(() => location.reload(), 100);
  };

  const syncOfficialLocale = async () => {
    const bridge = await waitForBridge();
    state.bridgeAvailable = Boolean(bridge);
    if (!bridge) {
      state.settingStatus = "bridge-unavailable";
      return;
    }
    try {
      const current = await callSettingApi(bridge, "get-setting", { key: "localeOverride" });
      if (current?.value === LOCALE) {
        state.settingStatus = "ready";
        reloadOnceForLocaleHooks();
        return;
      }
      await callSettingApi(bridge, "set-setting", { key: "localeOverride", value: LOCALE });
      const verified = await callSettingApi(bridge, "get-setting", { key: "localeOverride" });
      if (verified?.value !== LOCALE) {
        throw new Error(`localeOverride was not persisted (received ${String(verified?.value)})`);
      }
      state.settingStatus = "updated";
      reloadOnceForLocaleHooks();
    } catch (error) {
      state.settingStatus = "failed";
      state.settingError = String(error?.message || error);
    }
  };

  syncOfficialLocale();

  const requestVoiceTyping = () => {
    const composer =
      document.querySelector("textarea") ||
      document.querySelector('[contenteditable="true"][role="textbox"]') ||
      document.querySelector('[contenteditable="true"]');
    if (!composer) return;
    try {
      composer.focus({ preventScroll: true });
    } catch (_) {
      composer.focus();
    }
    const binding = globalThis[VOICE_BINDING];
    if (typeof binding === "function") binding("request");
  };

  const voiceTypingButtonHost = () => {
    const composer = document.querySelector('[data-codex-composer="true"]');
    const footer = composer?.closest("[data-composer-footer-responsive]");
    if (!footer) return null;
    const footerRect = footer.getBoundingClientRect();
    const buttons = Array.from(footer.querySelectorAll("button"))
      .filter((button) => button.id !== VOICE_BUTTON_ID)
      .filter((button) => {
        const rect = button.getBoundingClientRect();
        return (
          rect.width > 0 &&
          rect.height > 0 &&
          rect.top >= footerRect.top &&
          rect.bottom <= footerRect.bottom + 1
        );
      })
      .sort((left, right) => {
        const a = left.getBoundingClientRect();
        const b = right.getBoundingClientRect();
        return a.right - b.right;
      });
    const sendButton = buttons.at(-1);
    return sendButton?.parentElement ? { sendButton, host: sendButton.parentElement } : null;
  };

  const installVoiceTypingButton = () => {
    if (document.getElementById(VOICE_BUTTON_ID)) return;
    const placement = voiceTypingButtonHost();
    if (!placement) return;
    const button = document.createElement("button");
    button.id = VOICE_BUTTON_ID;
    button.type = "button";
    button.title = "Windows 语音输入（Win+H）";
    button.setAttribute("aria-label", "Windows 语音输入（Win+H）");
    button.innerHTML =
      '<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><rect x="9" y="2" width="6" height="12" rx="3"></rect><path d="M6 11a6 6 0 0 0 12 0M12 17v4M8 21h8"></path></svg>';
    button.className =
      "no-drag cursor-interaction items-center gap-1 border whitespace-nowrap select-none focus:outline-none disabled:cursor-not-allowed disabled:opacity-40 flex rounded-full text-token-text-tertiary enabled:hover:bg-token-list-hover-background border-transparent h-token-button-composer px-2 py-0 text-sm leading-[18px] aspect-square shrink-0 items-center justify-center !px-0";
    const icon = button.firstElementChild;
    if (icon) icon.setAttribute("class", "icon-xs text-token-text-primary");
    button.addEventListener("click", requestVoiceTyping);
    placement.host.insertBefore(button, placement.sendButton);
  };

  installVoiceTypingButton();
  setInterval(installVoiceTypingButton, 1000);

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
