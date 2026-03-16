import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Breadcrumb } from "./Breadcrumb";
import { useFileStore } from "../store/fileStore";

describe("Breadcrumb", () => {
  beforeEach(() => {
    useFileStore.setState({
      currentPath: "",
      navigationHistory: [""],
      historyIndex: 0,
    });
  });

  it("renders Vault Root button at root path", () => {
    render(<Breadcrumb />);
    expect(screen.getByText("Vault Root")).toBeInTheDocument();
  });

  it("renders breadcrumb parts for nested path", () => {
    useFileStore.setState({ currentPath: "docs/photos" });
    render(<Breadcrumb />);
    expect(screen.getByText("Vault Root")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(screen.getByText("photos")).toBeInTheDocument();
  });

  it("marks the last breadcrumb segment as current location", () => {
    useFileStore.setState({ currentPath: "docs/photos" });
    render(<Breadcrumb />);
    expect(screen.getByText("photos")).toHaveAttribute("aria-current", "location");
    expect(screen.getByText("docs")).not.toHaveAttribute("aria-current");
  });

  it("navigates when a breadcrumb segment is clicked", async () => {
    const user = userEvent.setup();
    useFileStore.setState({ currentPath: "docs/photos" });
    render(<Breadcrumb />);

    await user.click(screen.getByText("docs"));
    expect(useFileStore.getState().currentPath).toBe("docs");
  });

  it("navigates to root when Vault Root is clicked", async () => {
    const user = userEvent.setup();
    useFileStore.setState({ currentPath: "docs/photos" });
    render(<Breadcrumb />);

    await user.click(screen.getByText("Vault Root"));
    expect(useFileStore.getState().currentPath).toBe("");
  });

  it("has nav landmark with aria-label", () => {
    render(<Breadcrumb />);
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toBeInTheDocument();
  });
});
