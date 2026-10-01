// Runs only in the isolated Steam sign-in webview. Account credentials and the
// short-lived web API token stay in Steam's origin; native code receives a result.
async function rbxMakeSpacewarPrivate(expectedSteamId) {
  async function json(url, options = {}, steamResult = false) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 15000);
    try {
      const response = await fetch(url, { ...options, signal: controller.signal, cache: "no-store" });
      if (!response.ok) throw new Error(response.status === 401 ? "signin" : "service");
      if (steamResult && response.headers.get("x-eresult") !== "1") throw new Error("service");
      const data = await response.json();
      if (!data || typeof data !== "object" || Array.isArray(data)) throw new Error("service");
      return data;
    } finally {
      clearTimeout(timeout);
    }
  }

  const config = await json("https://store.steampowered.com/pointssummary/ajaxgetasyncconfig", { credentials: "same-origin" });
  const token = config.data?.webapi_token;
  if (config.success !== 1 || typeof token !== "string" || !token) throw new Error("signin");
  // This is an identity guard, not authentication: Steam verifies the token.
  let steamId;
  try {
    const payload = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
    steamId = JSON.parse(atob(payload)).sub;
  } catch {
    throw new Error("identity");
  }
  if (steamId !== expectedSteamId) throw new Error("account");

  const base = "https://api.steampowered.com/IAccountPrivateAppsService/";
  async function privateApps() {
    const params = new URLSearchParams({ access_token: token, origin: "https://store.steampowered.com", input_json: "{}" });
    const data = await json(`${base}GetPrivateAppList/v1/?${params}`, { credentials: "omit" }, true);
    const body = data.response;
    if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("service");
    // Protobuf JSON omits empty/default fields.
    if (body.private_apps != null && (typeof body.private_apps !== "object" || Array.isArray(body.private_apps))) throw new Error("service");
    const apps = body.private_apps?.appids ?? [];
    if (!Array.isArray(apps) || apps.some(id => !Number.isInteger(id) || id <= 0)) throw new Error("service");
    return apps;
  }
  if ((await privateApps()).includes(480)) return "already-private";

  const params = new URLSearchParams({ access_token: token });
  const form = new FormData();
  form.append("input_json", JSON.stringify({ appids: [480], private: true }));
  await json(`${base}ToggleAppPrivacy/v1/?${params}`, { method: "POST", body: form, credentials: "omit" }, true);
  for (let attempt = 0; attempt < 4; attempt++) {
    if ((await privateApps()).includes(480)) return "private";
    if (attempt < 3) await new Promise(resolve => setTimeout(resolve, 400));
  }
  throw new Error("verify");
}

if (typeof window !== "undefined" && window === window.top
    && location.origin === "https://store.steampowered.com" && location.pathname === "/points/shop/") {
  const config = __RBX_PRIVACY_CONFIG__;
  const finish = result => location.replace(`${config.callback}?result=${encodeURIComponent(result)}`);
  // Wait for Steam's own session refresh/bootstrap before asking for its token.
  window.addEventListener("load", async () => {
    try {
      finish(await rbxMakeSpacewarPrivate(config.steamId));
    } catch (error) {
      const result = ["signin", "identity", "account", "service", "verify"].includes(error?.message)
        ? error.message : "network";
      if (result === "signin") {
        // Authentication, including Steam Guard, is completed by the user on Steam.
        if (sessionStorage.getItem("rbx-privacy-signin")) {
          finish("signin");
        } else {
          sessionStorage.setItem("rbx-privacy-signin", "1");
          location.replace("https://store.steampowered.com/login/?redir=points%2Fshop%2F&redir_ssl=1");
        }
      } else {
        finish(result);
      }
    }
  }, { once: true });
}
