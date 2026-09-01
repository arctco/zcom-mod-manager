// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorBoundary } from "./ErrorBoundary";

afterEach(cleanup);
// jsdom re-throws a caught render error as a window error event, which prints a
// stack for a failure these tests are causing on purpose.
window.addEventListener("error", event => event.preventDefault());

function Boom({ fail }: { fail: boolean }): React.ReactElement {
  if (fail) throw new Error("entry is undefined");
  return <p>The library</p>;
}

describe("interface crash recovery", () => {
  it("keeps the window usable instead of leaving it blank", () => {
    // React logs the caught error itself; the test does not need the noise.
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    render(<ErrorBoundary><Boom fail /></ErrorBoundary>);
    expect(screen.getByRole("alert")).toBeDefined();
    expect(screen.getByText("The interface stopped responding")).toBeDefined();
    // The message is shown so a report can say what actually failed.
    expect(screen.getByText("entry is undefined")).toBeDefined();
    expect(screen.getByRole("button", { name: "Reload the interface" })).toBeDefined();
    logged.mockRestore();
  });

  it("lets the interface be retried without a restart", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const { rerender } = render(<ErrorBoundary><Boom fail /></ErrorBoundary>);
    rerender(<ErrorBoundary><Boom fail={false} /></ErrorBoundary>);
    await userEvent.click(screen.getByRole("button", { name: "Try to continue" }));
    expect(screen.getByText("The library")).toBeDefined();
    logged.mockRestore();
  });

  it("stays out of the way while nothing is wrong", () => {
    render(<ErrorBoundary><Boom fail={false} /></ErrorBoundary>);
    expect(screen.getByText("The library")).toBeDefined();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
