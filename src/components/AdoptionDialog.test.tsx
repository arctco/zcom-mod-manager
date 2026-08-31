// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ExistingModCandidate, ExistingModScan } from "../types";
import { AdoptionDialog, initialAdoptionGroups, mergeSelectedGroups, splitAdoptionGroup } from "./AdoptionDialog";

afterEach(cleanup);

function candidate(id: string, modType: ExistingModCandidate["modType"] = "pak", overrides: Partial<ExistingModCandidate> = {}): ExistingModCandidate {
  return {
    id,
    name: id[0].toUpperCase() + id.slice(1),
    version: null,
    modType,
    files: [`${id}_P.pak`],
    enabled: true,
    packageCount: 0,
    warnings: [],
    adoptable: true,
    blockedReason: null,
    selectedByDefault: true,
    likelyRuntimeComponent: false,
    inferredPriority: 1,
    ...overrides
  };
}

function scan(candidates: ExistingModCandidate[]): ExistingModScan {
  return { scanId: "scan", candidates, unsupported: [], warnings: [] };
}

describe("adoption grouping", () => {
  it("merges selected packaged candidates and splits them without changing names", () => {
    const candidates = [candidate("alpha"), candidate("bravo", "iostore")];
    const initial = initialAdoptionGroups(candidates);
    const merged = mergeSelectedGroups(initial, candidates);
    expect(merged).toEqual([{ candidateIds: ["alpha", "bravo"], name: "Alpha", selected: true }]);
    expect(splitAdoptionGroup(merged, 0, candidates)).toEqual([
      { candidateIds: ["alpha"], name: "Alpha", selected: true },
      { candidateIds: ["bravo"], name: "Bravo", selected: true }
    ]);
  });

  it("does not merge UE4SS candidates with packaged candidates", () => {
    const candidates = [candidate("alpha"), candidate("script", "ue4ss")];
    expect(mergeSelectedGroups(initialAdoptionGroups(candidates), candidates)).toHaveLength(2);
  });
});

describe("adoption dialog", () => {
  it("leaves likely runtime components unchecked and blocks unsafe candidates", () => {
    const runtime = candidate("runtime", "ue4ss", { likelyRuntimeComponent: true, selectedByDefault: false });
    const blocked = candidate("broken", "iostore", { adoptable: false, selectedByDefault: false, blockedReason: "Incomplete pair" });
    render(<AdoptionDialog scan={scan([runtime, blocked])} busy={false} onClose={vi.fn()} onAdopt={vi.fn()} />);
    const checkboxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(checkboxes.every(checkbox => !checkbox.checked)).toBe(true);
    expect(checkboxes[1].disabled).toBe(true);
    expect(screen.getByText("Incomplete pair")).toBeTruthy();
    expect(screen.getByText(/Likely part of the UE4SS runtime/)).toBeTruthy();
  });

  it("keeps failed groups available after successful groups are adopted", async () => {
    const candidates = [candidate("alpha"), candidate("bravo")];
    const onAdopt = vi.fn(async () => ({ outcomes: [
      { candidateIds: ["alpha"], name: "Alpha", modSummary: { id: "managed", name: "Alpha" } as never, error: null },
      { candidateIds: ["bravo"], name: "Bravo", modSummary: null, error: "Bravo changed after discovery." }
    ] }));
    const onClose = vi.fn();
    render(<AdoptionDialog scan={scan(candidates)} busy={false} onClose={onClose} onAdopt={onAdopt} />);
    await userEvent.click(screen.getByRole("button", { name: "Adopt 2 selected" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Bravo changed after discovery.");
    expect(screen.getByRole("textbox", { name: "Name for Bravo" })).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "Name for Alpha" })).toBeNull();
    expect(onClose).not.toHaveBeenCalled();
  });
});
