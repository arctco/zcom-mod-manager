// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AboutPage } from "./AboutPage";

afterEach(cleanup);

describe("About update status", () => {
  it("shows the startup result and opens the published release", async () => {
    const onOpenLink = vi.fn();
    render(<AboutPage
      projectUrl="https://github.com/arctco/zcom-mod-manager"
      onOpenLink={onOpenLink}
      update={{
        currentVersion: "0.2.0",
        latestVersion: "0.2.1",
        releaseUrl: "https://github.com/arctco/zcom-mod-manager/releases/tag/v0.2.1",
        updateAvailable: true
      }}
      checking={false}
      error={null}
      onCheckUpdates={vi.fn()}
    />);

    expect(screen.getByText("Version 0.2.1 is available.")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: /open release page/i }));
    expect(onOpenLink).toHaveBeenCalledWith("https://github.com/arctco/zcom-mod-manager/releases/tag/v0.2.1");
  });

  it("lets an offline user retry without hiding the error", async () => {
    const onCheckUpdates = vi.fn();
    render(<AboutPage
      projectUrl="https://github.com/arctco/zcom-mod-manager"
      onOpenLink={vi.fn()}
      update={null}
      checking={false}
      error="Network unavailable"
      onCheckUpdates={onCheckUpdates}
    />);

    expect(screen.getByText(/Couldn’t check GitHub: Network unavailable/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(onCheckUpdates).toHaveBeenCalledOnce();
  });
});
