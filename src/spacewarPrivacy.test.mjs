import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { describe, expect, it, vi } from "vitest";

const script = readFileSync(new URL("../src-tauri/src/spacewar_privacy.js", import.meta.url), "utf8");
const steamId = "76561198000000000";
const token = (id = steamId) => `header.${Buffer.from(JSON.stringify({ sub: id })).toString("base64url")}.signature`;
const reply = (body, result = "1", status = 200) => new Response(JSON.stringify(body), { status, headers: { "x-eresult": result } });

function setup(responses, account = steamId) {
  const fetch = vi.fn(async () => {
    const response = responses.shift();
    if (!response) throw new Error("Unexpected request");
    return response;
  });
  const context = { fetch, AbortController, URLSearchParams, FormData, atob, setTimeout, clearTimeout };
  runInNewContext(script, context);
  const run = runInNewContext("rbxMakeSpacewarPrivate", context);
  return { fetch, run: () => run(account) };
}
const config = (id = steamId) => reply({ success: 1, data: { webapi_token: token(id) } });
const apps = (ids) => reply({ response: { private_apps: { appids: ids } } });

describe("automatic Spacewar privacy", () => {
  it("changes only AppID 480 to private and verifies it with a fresh read", async () => {
    const { fetch, run } = setup([config(), apps([730]), reply({ response: {} }), apps([730, 480])]);
    expect(await run()).toBe("private");
    const calls = fetch.mock.calls;
    expect(calls.map(([url]) => new URL(url).pathname)).toEqual([
      "/pointssummary/ajaxgetasyncconfig", "/IAccountPrivateAppsService/GetPrivateAppList/v1/",
      "/IAccountPrivateAppsService/ToggleAppPrivacy/v1/", "/IAccountPrivateAppsService/GetPrivateAppList/v1/",
    ]);
    expect(calls[2][1].method).toBe("POST");
    expect(JSON.parse(calls[2][1].body.get("input_json"))).toEqual({ appids: [480], private: true });
  });

  it("does not mutate an already-private game", async () => {
    const { fetch, run } = setup([config(), apps([480, 730])]);
    expect(await run()).toBe("already-private");
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("refuses to modify a different Steam account", async () => {
    const { fetch, run } = setup([config("76561198000000001")]);
    await expect(run()).rejects.toThrow("account");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("requires Steam authentication before reading or changing privacy", async () => {
    const { fetch, run } = setup([reply({ success: 8, data: {} })]);
    await expect(run()).rejects.toThrow("signin");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("rejects a failed Steam result even when HTTP succeeds", async () => {
    const { fetch, run } = setup([config(), apps([]), reply({ response: {} }, "15")]);
    await expect(run()).rejects.toThrow("service");
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("does not report success when the readback still lists Spacewar as public", async () => {
    const { run } = setup([config(), apps([]), reply({ response: {} }), apps([]), apps([]), apps([]), apps([])]);
    await expect(run()).rejects.toThrow("verify");
  });

  it("rejects a malformed privacy list before any mutation", async () => {
    const { fetch, run } = setup([config(), reply({ response: { private_apps: { appids: "480" } } })]);
    await expect(run()).rejects.toThrow("service");
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});
