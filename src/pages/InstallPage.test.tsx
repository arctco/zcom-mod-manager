// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ModPreview, PreviewType } from "../types";
import { InstallPage } from "./InstallPage";

afterEach(cleanup);

const preview = (stagingId: string, name: string, modType: PreviewType = "ue4ss"): ModPreview => ({
  stagingId, sourcePath: "/downloads/TrueLightShadows.zip", name, version: null, author: null,
  description: null, modType, files: [`${name}/Scripts/main.lua`], warnings: [], valid: true,
  verification: "not-required", verificationDetails: null, packageCount: 0, packageNames: [],
  compatibility: "unknown", compatibilityMessage: "Unknown", testedBuilds: [], conflicts: [], replaces: null,
  recommendedPriority: null, loadOrderSupported: false, loadOrderSupportReason: null,
  optionLabel: null
});

function props(overrides: Partial<Parameters<typeof InstallPage>[0]> = {}): Parameters<typeof InstallPage>[0] {
  return {
    previews: [], names: {}, loading: false, download: null, advanced: false, installing: null,
    onAdvanced: vi.fn(), onName: vi.fn(), onChooseFile: vi.fn(), onChooseFolder: vi.fn(),
    onInstall: vi.fn(), onInstallAll: vi.fn(), onInstallRuntime: vi.fn(), onCancel: vi.fn(), ...overrides
  };
}

describe("install preview", () => {
  it("offers every mod an archive contains", () => {
    render(<InstallPage {...props({ previews: [preview("a", "ShadowsCore"), preview("b", "ShadowsTweaks")] })} />);
    expect(screen.getByText("2 components found in this download")).toBeDefined();
    const fields = screen.getAllByLabelText("Mod name") as HTMLInputElement[];
    expect(fields.map(field => field.value)).toEqual(["ShadowsCore", "ShadowsTweaks"]);
    expect(screen.getAllByRole("button", { name: "Install" })).toHaveLength(2);
  });

  it("offers one action for every required component in a bundle", async () => {
    const onInstallAll = vi.fn();
    const mods = [preview("a", "Squad Six - Runtime"), preview("b", "Squad Six - Core", "iostore")];
    render(<InstallPage {...props({ previews: mods, onInstallAll })} />);
    await userEvent.click(screen.getByRole("button", { name: "Install all components" }));
    expect(onInstallAll).toHaveBeenCalledWith(mods);
  });

  it("labels mutually selectable packaged folders clearly", () => {
    const big = { ...preview("a", "Blackmarket Discounts — 25%", "pak"), optionLabel: "25% (Big Cheat)" };
    const free = { ...preview("b", "Blackmarket Discounts — Free", "pak"), optionLabel: "Free (Mega Cheat)" };
    render(<InstallPage {...props({ previews: [big, free] })} />);
    expect(screen.getByText("2 packaged options found")).toBeDefined();
    expect(screen.getByText("ARCHIVE OPTION · 25% (Big Cheat)")).toBeDefined();
    expect(screen.getByText("ARCHIVE OPTION · Free (Mega Cheat)")).toBeDefined();
    expect(screen.getByText(/alternatives may conflict/i)).toBeDefined();
  });

  it("reports an edited name and installs the mod it belongs to", async () => {
    const onName = vi.fn();
    const onInstall = vi.fn();
    const mods = [preview("a", "ShadowsCore"), preview("b", "ShadowsTweaks")];
    render(<InstallPage {...props({ previews: mods, names: { b: "Shadow Tweaks" }, onName, onInstall })} />);
    const fields = screen.getAllByLabelText("Mod name") as HTMLInputElement[];
    expect(fields[1].value).toBe("Shadow Tweaks");
    await userEvent.type(fields[0], "!");
    expect(onName).toHaveBeenCalledWith("a", "ShadowsCore!");
    await userEvent.click(screen.getAllByRole("button", { name: "Install" })[1]);
    expect(onInstall).toHaveBeenCalledWith(mods[1]);
  });

  it("sends a runtime package to the UE4SS installer instead of the library", async () => {
    const onInstallRuntime = vi.fn();
    const runtime = preview("r", "UE4SS For Star Wars Zero Company", "ue4ss-runtime");
    render(<InstallPage {...props({ previews: [runtime], onInstallRuntime })} />);
    expect(screen.queryByLabelText("Mod name")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Install UE4SS runtime" }));
    expect(onInstallRuntime).toHaveBeenCalledWith(runtime);
  });

  it("names the payloads it accepts before anything is chosen", () => {
    render(<InstallPage {...props()} />);
    expect(screen.getByText(/UE4SS Lua or DLL mod/)).toBeDefined();
  });
});

describe("upgrades", () => {
  it("offers to replace the installed version instead of reporting a conflict", async () => {
    const onInstall = vi.fn();
    const upgrade: ModPreview = {
      ...preview("a", "ZCUnlocked"),
      replaces: { modId: "old", name: "ZC Unlocked", version: "1.2", reason: "It uses the same UE4SS mod folder." }
    };
    render(<InstallPage {...props({ previews: [upgrade], onInstall })} />);
    expect(screen.getByText("Replaces ZC Unlocked 1.2")).toBeDefined();
    await userEvent.click(screen.getByRole("button", { name: "Replace installed version" }));
    expect(onInstall).toHaveBeenCalledWith(upgrade);
  });

  it("offers to update every component when a bundle matches old installs", () => {
    const core = {
      ...preview("core", "Squad Six - Core", "iostore"),
      replaces: { modId: "old-core", name: "Squad Six - Core", version: "1.0.1", reason: "It ships the same container files." }
    };
    const runtime = {
      ...preview("runtime", "Squad Six - Runtime"),
      replaces: { modId: "old-runtime", name: "Squad Six - Runtime", version: "1.0.1", reason: "It uses the same UE4SS mod folder." }
    };
    render(<InstallPage {...props({ previews: [core, runtime] })} />);
    expect(screen.getByRole("button", { name: "Update all components" })).toBeDefined();
  });
});
