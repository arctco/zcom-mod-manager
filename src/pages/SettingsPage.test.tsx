// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AppSettings } from "../types";
import { SettingsPage } from "./SettingsPage";

afterEach(cleanup);

const settings: AppSettings = {
  gamePath: null,
  customExecutablePath: "C:\\Games\\ZeroCompany.exe",
  retocPath: null,
  logLevel: "normal",
  advancedPackageNames: false,
  reducedMotion: false,
  nexusAutoUpdateCheck: false
};

function props(overrides: Partial<Parameters<typeof SettingsPage>[0]> = {}): Parameters<typeof SettingsPage>[0] {
  return {
    settings,
    retoc: { found: true, path: "/bin/retoc", version: "retoc 0.1.5" },
    onChange: vi.fn(),
    onSave: vi.fn(),
    onPickGame: vi.fn(),
    onPickExecutable: vi.fn(),
    onPickRetoc: vi.fn(),
    onOpenLogs: vi.fn(),
    onOpenData: vi.fn(),
    links: { ue4ssDownload: "", nexusGame: "", nexusManager: "", project: "" },
    onOpenLink: vi.fn(),
    nexus: null,
    nexusAccount: null,
    onSaveNexusKey: vi.fn(),
    onClearNexusKey: vi.fn(),
    onToggleNxmHandler: vi.fn(),
    onSetAutoUpdateCheck: vi.fn(),
    ...overrides
  };
}

describe("custom game launcher", () => {
  it("shows, browses, and clears a custom executable", async () => {
    const onPickExecutable = vi.fn();
    const onChange = vi.fn();
    render(<SettingsPage {...props({ onPickExecutable, onChange })} />);

    const input = screen.getByLabelText("Game launch executable or launcher") as HTMLInputElement;
    expect(input.value).toBe("C:\\Games\\ZeroCompany.exe");
    const label = input.closest("label");
    expect(label).not.toBeNull();
    await userEvent.click(within(label!).getByRole("button", { name: "Browse" }));
    await userEvent.click(within(label!).getByRole("button", { name: "Use Steam" }));

    expect(onPickExecutable).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith({ ...settings, customExecutablePath: null });
  });

  it("describes Steam as the default when no custom path is set", () => {
    render(<SettingsPage {...props({ settings: { ...settings, customExecutablePath: null } })} />);
    expect((screen.getByLabelText("Game launch executable or launcher") as HTMLInputElement).value).toBe("Steam default");
    expect(screen.queryByRole("button", { name: "Use Steam" })).toBeNull();
  });
});

describe("Nexus update checking", () => {
  it("saves the moment it is set, without waiting for Save settings", async () => {
    // Every other control in this panel applies at once, and an unsaved toggle
    // was also discarded by the next refresh, so it looked like it never stuck.
    const onSetAutoUpdateCheck = vi.fn().mockResolvedValue(undefined);
    const onChange = vi.fn();
    const onSave = vi.fn();
    render(<SettingsPage {...props({ onSetAutoUpdateCheck, onChange, onSave })} />);
    const check = screen.getByRole("checkbox", { name: /Check installed mods for updates/ });
    expect((check as HTMLInputElement).checked).toBe(false);
    await userEvent.click(check);
    expect(onSetAutoUpdateCheck).toHaveBeenCalledWith(true);
    expect(onSave).not.toHaveBeenCalled();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows the stored state when the page opens", () => {
    render(<SettingsPage {...props({ settings: { ...settings, nexusAutoUpdateCheck: true } })} />);
    const check = screen.getByRole("checkbox", { name: /Check installed mods for updates/ });
    expect((check as HTMLInputElement).checked).toBe(true);
  });

  it("names the account a stored key belongs to after a restart", () => {
    render(<SettingsPage {...props({ nexus: { hasKey: true, accountName: "Arc", premium: true, storage: "keyring", handlerRegistered: true, handlerOwner: null, handlerProblem: null } })} />);
    expect(screen.getByText("Connected as Arc · premium account")).toBeDefined();
  });
});
