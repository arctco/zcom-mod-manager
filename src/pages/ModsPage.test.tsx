// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ConflictGroup, LoadOrderEntry, LoadOrderPreview, LoadOrderState } from "../types";
import { dropOrder, ModsPage, moveOrder, winnerFor } from "./ModsPage";

afterEach(cleanup);
window.requestAnimationFrame = callback => { callback(0); return 0; };

const entry = (id: string, enabled = true): LoadOrderEntry => ({
  id, name: id[0].toUpperCase() + id.slice(1), modType: "iostore", enabled,
  priority: id === "alpha" ? 2 : 1, supported: true, supportReason: null,
  applied: true, activeConflictCount: 1, potentialConflictCount: 1
});

const conflict: ConflictGroup = {
  id: "overlap-1", memberIds: ["alpha", "bravo"], packageCount: 2,
  active: true, potential: false, winnerId: "alpha"
};

const loadOrder: LoadOrderState = {
  entries: [
    entry("alpha"),
    entry("bravo"),
    {
      id: "legacy", name: "Legacy PAK", modType: "pak", enabled: true,
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
    onInstall: vi.fn(), onToggle: vi.fn(), onUninstall: vi.fn(), onVerify: vi.fn(),
    onOpenInstalled: vi.fn(), onOpenSource: vi.fn(), onBrowseNexus: vi.fn(),
    onPreviewOrder: vi.fn(), onApplyOrder: vi.fn(), onCancelOrder: vi.fn(),
    ...overrides
  };
}

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
