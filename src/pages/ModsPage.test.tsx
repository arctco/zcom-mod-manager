// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ConflictGroup, LoadOrderEntry, LoadOrderPreview, LoadOrderState, ModSummary, ModUpdate, ModUpdateReport } from "../types";
import { dropOrder, ModsPage, moveOrder, winnerFor } from "./ModsPage";

afterEach(cleanup);
window.requestAnimationFrame = callback => { callback(0); return 0; };

const entry = (id: string, enabled = true): LoadOrderEntry => ({
  id, name: id[0].toUpperCase() + id.slice(1), modType: "iostore", runtimeKind: null, enabled,
  priority: id === "alpha" ? 2 : 1, supported: true, supportReason: null,
  applied: true, activeConflictCount: 1, potentialConflictCount: 1
});

const conflict: ConflictGroup = {
  id: "overlap-1", memberIds: ["alpha", "bravo"], packageCount: 2,
  active: true, potential: false, winnerId: "alpha"
};

const loadOrder: LoadOrderState = {
  ue4ssEntries: [],
  entries: [
    entry("alpha"),
    entry("bravo"),
    {
      id: "legacy", name: "Legacy PAK", modType: "pak", runtimeKind: null, enabled: true,
      priority: 3, supported: false,
      supportReason: "PAK-only ordering did not pass the runtime capability test.",
      applied: false, activeConflictCount: 0, potentialConflictCount: 0
    }
  ],
  activeConflicts: [conflict],
  potentialConflicts: [],
  unapplied: false
};

function props(overrides: Partial<Parameters<typeof ModsPage>[0]> = {}): Parameters<typeof ModsPage>[0] {
  return {
    mods: [], loadOrder, orderPreview: null, busy: null, orderBusy: false,
    onInstall: vi.fn(), onToggle: vi.fn(), onUninstall: vi.fn(), onVerify: vi.fn(), onRename: vi.fn(),
    onOpenInstalled: vi.fn(), onOpenSource: vi.fn(), onBrowseNexus: vi.fn(),
    onPreviewOrder: vi.fn(), onApplyOrder: vi.fn(), onApplyUe4ssOrder: vi.fn(), onCancelOrder: vi.fn(),
    updates: null, checkingUpdates: false, canCheckUpdates: true, directDownload: false,
    onCheckUpdates: vi.fn(), onUpdateMod: vi.fn(), onLinkMod: vi.fn(), onSetModChecked: vi.fn(), onOpenModPage: vi.fn(), onSetHidden: vi.fn(),
    ...overrides
  };
}

const installed = (id: string, modType: ModSummary["modType"]): ModSummary => ({
  id, name: id, version: null, modType, enabled: true, installedAt: "2026-08-30T00:00:00Z",
  installedBuild: null, packageCount: 0, conflictCount: 0, potentialConflictCount: 0,
  loadPriority: null, nexusModId: null, nexusUrl: null, nexusIgnored: false, hidden: false, files: []
});

async function openLoadOrder() {
  await userEvent.click(screen.getByRole("tab", { name: "Load order" }));
}

describe("load-order helpers", () => {
  it("moves entries with keyboard-button semantics", () => {
    expect(moveOrder(["alpha", "bravo", "charlie"], "bravo", -1)).toEqual(["bravo", "alpha", "charlie"]);
    expect(moveOrder(["alpha", "bravo"], "alpha", -1)).toEqual(["alpha", "bravo"]);
  });

  it("supports dropping before or after a row, including the final position", () => {
    expect(dropOrder(["alpha", "bravo", "charlie"], "alpha", "charlie", true))
      .toEqual(["bravo", "charlie", "alpha"]);
    expect(dropOrder(["alpha", "bravo", "charlie"], "charlie", "alpha", false))
      .toEqual(["charlie", "alpha", "bravo"]);
  });

  it("uses the highest enabled row as the draft winner", () => {
    const entries = [entry("alpha"), entry("bravo")];
    expect(winnerFor(conflict, ["alpha", "bravo"], entries)).toBe("alpha");
    expect(winnerFor(conflict, ["bravo", "alpha"], entries)).toBe("bravo");
    expect(winnerFor(conflict, ["alpha", "bravo"], [entry("alpha", false), entry("bravo")])).toBe("bravo");
  });

  it("does not claim a winner when a conflicting layout is unsupported", () => {
    const unsupported = { ...entry("bravo"), supported: false, supportReason: "Not verified" };
    expect(winnerFor(conflict, ["alpha"], [entry("alpha"), unsupported])).toBeNull();
  });
});

describe("load-order interface", () => {
  it("switches tabs with pointer and arrow keys while retaining keyboard focus", async () => {
    render(<ModsPage {...props()} />);
    const loadTab = screen.getByRole("tab", { name: "Load order" });
    await userEvent.click(loadTab);
    expect(loadTab.getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(loadTab, { key: "ArrowLeft" });
    const libraryTab = screen.getByRole("tab", { name: "Library" });
    expect(libraryTab.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(libraryTab);
  });

  it("updates the draft winner, previews button ordering, and discards without applying", async () => {
    const onPreviewOrder = vi.fn();
    const onCancelOrder = vi.fn();
    render(<ModsPage {...props({ onPreviewOrder, onCancelOrder })} />);
    await openLoadOrder();
    onCancelOrder.mockClear();

    await userEvent.click(screen.getByRole("button", { name: "Move Bravo up" }));
    expect(screen.getByText("Bravo wins")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Review changes" }));
    expect(onPreviewOrder).toHaveBeenCalledWith(["bravo", "alpha"]);

    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(onCancelOrder).toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Review changes" })).toBeNull();
  });

  it("supports pointer drag ordering and keeps unsupported layouts explanatory", async () => {
    const onPreviewOrder = vi.fn();
    render(<ModsPage {...props({ onPreviewOrder })} />);
    await openLoadOrder();
    expect(screen.getByText("Not orderable yet")).toBeTruthy();
    expect(screen.getByText("Legacy PAK")).toBeTruthy();

    const bravo = screen.getByRole("button", { name: "Drag Bravo" });
    const alpha = screen.getByRole("button", { name: "Drag Alpha" }).closest("article")!;
    vi.spyOn(alpha, "getBoundingClientRect").mockReturnValue({
      top: 0, bottom: 100, height: 100, left: 0, right: 500, width: 500, x: 0, y: 0,
      toJSON: () => ({})
    });
    const dataTransfer = { setData: vi.fn(), getData: vi.fn(() => "bravo"), effectAllowed: "none" };
    fireEvent.dragStart(bravo, { dataTransfer });
    fireEvent.drop(alpha, { clientY: 25, dataTransfer });
    await userEvent.click(screen.getByRole("button", { name: "Review changes" }));
    expect(onPreviewOrder).toHaveBeenCalledWith(["bravo", "alpha"]);
  });

  it("renders a filename-only review and calls apply or back explicitly", async () => {
    const preview: LoadOrderPreview = {
      orderedModIds: ["bravo", "alpha"],
      moves: [{ modId: "bravo", from: "Bravo_0001_P.utoc", to: "Bravo_0002_P.utoc" }],
      activeConflicts: [{ ...conflict, winnerId: "bravo" }], potentialConflicts: [],
      winnerChanges: [{ conflictId: conflict.id, fromModId: "alpha", toModId: "bravo" }]
    };
    const onApplyOrder = vi.fn();
    const onCancelOrder = vi.fn();
    render(<ModsPage {...props({ orderPreview: preview, onApplyOrder, onCancelOrder })} />);
    await openLoadOrder();
    expect(screen.getByText("Bravo_0001_P.utoc")).toBeTruthy();
    expect(screen.queryByText(/SWZeroCompany\/Content/)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Apply order" }));
    expect(onApplyOrder).toHaveBeenCalledWith(["bravo", "alpha"]);
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(onCancelOrder).toHaveBeenCalled();
  });
});

describe("load-order scope", () => {
  it("says where the mods it does not list are ordered instead", async () => {
    render(<ModsPage {...props({ mods: [installed("a", "ue4ss"), installed("b", "ue4ss"), installed("c", "gamedir")] })} />);
    await openLoadOrder();
    expect(screen.getByText("Ordered elsewhere")).toBeDefined();
    expect(screen.getByText("2 UE4SS mods")).toBeDefined();
    expect(screen.getByText("1 game-folder mod")).toBeDefined();
  });

  it("explains the empty view rather than looking like the mods were lost", () => {
    const empty: LoadOrderState = { entries: [], ue4ssEntries: [], activeConflicts: [], potentialConflicts: [], unapplied: false };
    render(<ModsPage {...props({ loadOrder: empty, mods: [installed("a", "ue4ss")] })} />);
    fireEvent.click(screen.getByRole("tab", { name: "Load order" }));
    expect(screen.getByText(/1 UE4SS mod:/)).toBeDefined();
  });
});

describe("UE4SS start order", () => {
  const runtimeEntry = (id: string, name: string, priority: number, runtimeKind: "native" | "script" = "script"): LoadOrderEntry => ({
    id, name, modType: "ue4ss", runtimeKind, enabled: true, priority, supported: true, supportReason: null,
    applied: true, activeConflictCount: 0, potentialConflictCount: 0
  });
  const withUe4ss: LoadOrderState = {
    ...loadOrder,
    ue4ssEntries: [runtimeEntry("talents", "Talents", 1), runtimeEntry("unlocked", "Unlocked", 2)]
  };

  it("lists UE4SS mods in start order and applies a reordered list", async () => {
    const onApplyUe4ssOrder = vi.fn();
    render(<ModsPage {...props({ loadOrder: withUe4ss, onApplyUe4ssOrder })} />);
    await openLoadOrder();
    expect(screen.getByText("UE4SS start order")).toBeDefined();

    await userEvent.click(screen.getByRole("button", { name: "Move Unlocked up" }));
    await userEvent.click(screen.getByRole("button", { name: "Apply start order" }));
    expect(onApplyUe4ssOrder).toHaveBeenCalledWith(["unlocked", "talents"]);
  });

  it("shows the UE4SS list even when no packaged mod is orderable", async () => {
    const onlyUe4ss: LoadOrderState = { ...withUe4ss, entries: [], activeConflicts: [] };
    render(<ModsPage {...props({ loadOrder: onlyUe4ss })} />);
    await openLoadOrder();
    expect(screen.getByText("UE4SS start order")).toBeDefined();
    expect(screen.getByText("No packaged mods to order")).toBeDefined();
  });
});

describe("UE4SS start passes", () => {
  const mixed: LoadOrderState = {
    ...loadOrder,
    ue4ssEntries: [
      { id: "dll", name: "Unlocked", modType: "ue4ss", runtimeKind: "native", enabled: true, priority: 1, supported: true, supportReason: null, applied: true, activeConflictCount: 0, potentialConflictCount: 0 },
      { id: "lua-a", name: "Squad Six", modType: "ue4ss", runtimeKind: "script", enabled: true, priority: 2, supported: true, supportReason: null, applied: true, activeConflictCount: 0, potentialConflictCount: 0 },
      { id: "lua-b", name: "Harder", modType: "ue4ss", runtimeKind: "script", enabled: true, priority: 3, supported: true, supportReason: null, applied: true, activeConflictCount: 0, potentialConflictCount: 0 }
    ]
  };

  it("separates the DLL pass from the Lua pass", async () => {
    render(<ModsPage {...props({ loadOrder: mixed })} />);
    await openLoadOrder();
    expect(screen.getByText("Starts first — DLL mods")).toBeDefined();
    expect(screen.getByText("Starts second — Lua mods")).toBeDefined();
  });

  it("keeps a move inside its own pass", async () => {
    const onApplyUe4ssOrder = vi.fn();
    render(<ModsPage {...props({ loadOrder: mixed, onApplyUe4ssOrder })} />);
    await openLoadOrder();
    // The first Lua mod cannot move above the DLL mod: it is already first in
    // its own pass, so the control is unavailable.
    expect(screen.getByRole("button", { name: "Move Squad Six up" }).hasAttribute("disabled")).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "Move Harder up" }));
    await userEvent.click(screen.getByRole("button", { name: "Apply start order" }));
    expect(onApplyUe4ssOrder).toHaveBeenCalledWith(["dll", "lua-b", "lua-a"]);
  });
});

describe("Nexus mod updates", () => {
  const update: ModUpdate = {
    modId: "unlocked", name: "ZCUnlocked", installedVersion: "1.3", installedFileId: 200,
    nexusModId: 34, latestFileId: 260, latestVersion: "1.4", latestFileName: "ZCUnlocked-1.4.zip",
    pageUrl: "https://www.nexusmods.com/starwarszerocompany/mods/34?tab=files",
    nxmUrl: "nxm://starwarszerocompany/mods/34/files/260",
    checkedAt: "2026-09-01T00:00:00Z"
  };
  const report = (overrides: Partial<ModUpdateReport> = {}): ModUpdateReport => ({
    updates: [update], tracked: 1, identified: 0, unmatched: 0, ignored: 0, checkedAt: "2026-09-01T00:00:00Z", fromCache: true, problem: null, ...overrides
  });
  const library = [{ ...installed("unlocked", "iostore"), name: "ZCUnlocked", version: "1.3" }];

  it("marks the mod and offers the update", async () => {
    const onUpdateMod = vi.fn();
    render(<ModsPage {...props({ mods: library, updates: report(), onUpdateMod })} />);
    expect(screen.getByText("1 update available")).toBeDefined();
    expect(screen.getByText("Update available: 1.4")).toBeDefined();
    await userEvent.click(screen.getByRole("button", { name: "Open on Nexus" }));
    expect(onUpdateMod).toHaveBeenCalledWith(update);
  });

  it("offers a direct download only to a premium account", () => {
    render(<ModsPage {...props({ mods: library, updates: report(), directDownload: true })} />);
    expect(screen.getByRole("button", { name: "Download update" })).toBeDefined();
  });

  it("says nothing when the last check found nothing", () => {
    render(<ModsPage {...props({ mods: library, updates: report({ updates: [] }) })} />);
    expect(screen.queryByText(/update available/i)).toBeNull();
  });

  it("cannot check without a stored API key", async () => {
    const onCheckUpdates = vi.fn();
    render(<ModsPage {...props({ mods: library, updates: null, canCheckUpdates: false, onCheckUpdates })} />);
    const button = screen.getByRole("button", { name: "Check for updates" });
    expect(button.hasAttribute("disabled")).toBe(true);
    await userEvent.click(button);
    expect(onCheckUpdates).not.toHaveBeenCalled();
  });

  it("checks on demand", async () => {
    const onCheckUpdates = vi.fn();
    render(<ModsPage {...props({ mods: library, updates: null, onCheckUpdates })} />);
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(onCheckUpdates).toHaveBeenCalled();
  });
});

describe("linking a mod that was not downloaded here", () => {
  const orphan = installed("adopted", "ue4ss");

  it("offers to link an unmatched mod by its Nexus address", async () => {
    const onLinkMod = vi.fn();
    render(<ModsPage {...props({ mods: [orphan], onLinkMod })} />);
    await userEvent.click(screen.getByRole("button", { name: "More details for adopted" }));
    const field = screen.getByRole("textbox", { name: "Nexus Mods address for adopted" });
    await userEvent.type(field, "https://www.nexusmods.com/games/starwarszerocompany/mods/34");
    await userEvent.click(screen.getByRole("button", { name: "Link" }));
    expect(onLinkMod).toHaveBeenCalledWith(orphan, "https://www.nexusmods.com/games/starwarszerocompany/mods/34");
  });

  it("shows the linked mod and offers to stop checking it", async () => {
    const onSetModChecked = vi.fn();
    const linked = { ...orphan, nexusModId: 34 };
    render(<ModsPage {...props({ mods: [linked], onSetModChecked })} />);
    await userEvent.click(screen.getByRole("button", { name: "More details for adopted" }));
    expect(screen.getByText("#34")).toBeDefined();
    await userEvent.click(screen.getByRole("button", { name: "Stop checking this mod" }));
    expect(onSetModChecked).toHaveBeenCalledWith(linked, false);
  });

  it("lets a mod that is not on Nexus be left out for good", async () => {
    const onSetModChecked = vi.fn();
    render(<ModsPage {...props({ mods: [orphan], onSetModChecked })} />);
    await userEvent.click(screen.getByRole("button", { name: "More details for adopted" }));
    await userEvent.click(screen.getByRole("button", { name: "Never check this mod" }));
    expect(onSetModChecked).toHaveBeenCalledWith(orphan, false);
  });

  it("offers an excluded mod back, and stops asking for an address", async () => {
    const onSetModChecked = vi.fn();
    const excluded = { ...orphan, nexusIgnored: true };
    render(<ModsPage {...props({ mods: [excluded], onSetModChecked })} />);
    await userEvent.click(screen.getByRole("button", { name: "More details for adopted" }));
    expect(screen.queryByRole("textbox", { name: "Nexus Mods address for adopted" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Check this mod again" }));
    expect(onSetModChecked).toHaveBeenCalledWith(excluded, true);
  });
});

describe("hiding a mod", () => {
  const runtime = { ...installed("bpml", "ue4ss"), name: "BPML Generic Functions" };

  it("keeps a hidden mod out of the library list without uninstalling it", async () => {
    const onSetHidden = vi.fn();
    render(<ModsPage {...props({ mods: [runtime, installed("alpha", "iostore")], onSetHidden })} />);
    await userEvent.click(screen.getByRole("button", { name: "Hide BPML Generic Functions" }));
    expect(onSetHidden).toHaveBeenCalledWith(runtime, true);
  });

  it("leaves hidden mods out of every other view and counts them", () => {
    render(<ModsPage {...props({ mods: [{ ...runtime, hidden: true }, installed("alpha", "iostore")] })} />);
    expect(screen.queryByText("BPML Generic Functions")).toBeNull();
    expect(screen.getByText("1 of 2 shown · 1 hidden")).toBeDefined();
  });

  it("shows them under the hidden filter, where they can be brought back", async () => {
    const onSetHidden = vi.fn();
    const hidden = { ...runtime, hidden: true };
    render(<ModsPage {...props({ mods: [hidden], onSetHidden })} />);
    await userEvent.selectOptions(screen.getByLabelText("Filter installed mods"), "hidden");
    expect(screen.getByText("BPML Generic Functions")).toBeDefined();
    await userEvent.click(screen.getByRole("button", { name: "Show BPML Generic Functions" }));
    expect(onSetHidden).toHaveBeenCalledWith(hidden, false);
  });
});

describe("a mod removed while its order is drafted", () => {
  // The bug behind the black window: uninstalling a packaged mod refreshed the
  // list while the drafted order still named it, and the order row looked that
  // name up in a list it had just left. The lookup threw during render, React
  // unmounted the whole tree, and the window went dark until the application
  // was restarted.
  it("renders without throwing when the mod list shrinks under the draft", async () => {
    const { rerender } = render(<ModsPage {...props({ mods: [installed("alpha", "iostore"), installed("bravo", "iostore")] })} />);
    await openLoadOrder();
    expect(screen.getByText("Alpha")).toBeDefined();

    const shrunk: LoadOrderState = {
      ...loadOrder,
      entries: loadOrder.entries.filter(entry => entry.id !== "alpha"),
      activeConflicts: []
    };
    expect(() => rerender(<ModsPage {...props({ mods: [installed("bravo", "iostore")], loadOrder: shrunk })} />)).not.toThrow();
    expect(screen.queryByText("Alpha")).toBeNull();
    expect(screen.getByText("Bravo")).toBeDefined();
  });

  it("does not claim the order changed just because a mod was removed", async () => {
    const { rerender } = render(<ModsPage {...props()} />);
    await openLoadOrder();
    const shrunk: LoadOrderState = {
      ...loadOrder,
      entries: loadOrder.entries.filter(entry => entry.id !== "alpha"),
      activeConflicts: []
    };
    rerender(<ModsPage {...props({ loadOrder: shrunk })} />);
    expect(screen.queryByText("Load order changed")).toBeNull();
  });

  it("survives a UE4SS mod leaving the start order the same way", async () => {
    const withRuntime: LoadOrderState = {
      ...loadOrder,
      ue4ssEntries: [
        { id: "runtime", name: "Squad Six - Runtime", modType: "ue4ss", runtimeKind: "script", enabled: true, priority: 1, supported: true, supportReason: null, applied: true, activeConflictCount: 0, potentialConflictCount: 0 }
      ]
    };
    const { rerender } = render(<ModsPage {...props({ loadOrder: withRuntime })} />);
    await openLoadOrder();
    expect(screen.getByText("Squad Six - Runtime")).toBeDefined();
    expect(() => rerender(<ModsPage {...props({ loadOrder: { ...withRuntime, ue4ssEntries: [] } })} />)).not.toThrow();
    expect(screen.queryByText("UE4SS start order changed")).toBeNull();
  });
});

describe("opening a mod on Nexus", () => {
  const linked = { ...installed("unlocked", "iostore"), name: "ZCUnlocked", nexusModId: 34, nexusUrl: "https://www.nexusmods.com/starwarszerocompany/mods/34" };

  it("offers the page from the row of a linked mod", async () => {
    const onOpenModPage = vi.fn();
    render(<ModsPage {...props({ mods: [linked], onOpenModPage })} />);
    await userEvent.click(screen.getByRole("button", { name: "Open ZCUnlocked on Nexus Mods" }));
    expect(onOpenModPage).toHaveBeenCalledWith(linked);
  });

  it("offers nothing to open for a mod with no page", () => {
    render(<ModsPage {...props({ mods: [installed("mine", "iostore")] })} />);
    expect(screen.queryByRole("button", { name: /on Nexus Mods/ })).toBeNull();
  });

  it("offers it from the details panel as well", async () => {
    const onOpenModPage = vi.fn();
    render(<ModsPage {...props({ mods: [linked], onOpenModPage })} />);
    await userEvent.click(screen.getByRole("button", { name: "More details for ZCUnlocked" }));
    await userEvent.click(screen.getByRole("button", { name: "Open on Nexus Mods" }));
    expect(onOpenModPage).toHaveBeenCalledWith(linked);
  });
});
