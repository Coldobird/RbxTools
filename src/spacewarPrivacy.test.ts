import { afterEach, describe, expect, it, vi } from "vitest";
import { setUiTestActive } from "./uiTestView";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
import { getSpacewarPrivacy, setSpacewarPrivacy } from "./api";

afterEach(() => { setUiTestActive(false); vi.clearAllMocks(); });

describe("Spacewar native privacy commands", () => {
  it("reads the actual setting without a mutation or sign-in dialog", async () => {
    mocks.invoke.mockResolvedValueOnce(true);
    expect(await getSpacewarPrivacy()).toBe(true);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_spacewar_privacy");
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it.each([true, false])("requests explicit private=%s and returns Steam's verified setting", async (privateGame) => {
    mocks.invoke.mockResolvedValueOnce(privateGame);
    expect(await setSpacewarPrivacy(privateGame)).toBe(privateGame);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("set_spacewar_privacy", { private: privateGame });
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("propagates verification failures rather than reporting success", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("Steam privacy verification failed"));
    await expect(setSpacewarPrivacy(true)).rejects.toThrow("verification failed");
  });
});
