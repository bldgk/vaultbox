import { describe, it, expect, vi, beforeEach } from "vitest";
import { useDialogStore } from "./dialogStore";

describe("dialogStore", () => {
  beforeEach(() => {
    useDialogStore.setState({ confirmDialog: null });
  });

  it("starts with no dialog", () => {
    expect(useDialogStore.getState().confirmDialog).toBeNull();
  });

  it("showConfirm opens a dialog with defaults", () => {
    const onConfirm = vi.fn();
    useDialogStore.getState().showConfirm({
      title: "Test",
      message: "Are you sure?",
      onConfirm,
    });

    const dialog = useDialogStore.getState().confirmDialog;
    expect(dialog).not.toBeNull();
    expect(dialog!.open).toBe(true);
    expect(dialog!.title).toBe("Test");
    expect(dialog!.message).toBe("Are you sure?");
    expect(dialog!.confirmLabel).toBe("Confirm");
    expect(dialog!.danger).toBe(false);
    expect(dialog!.onConfirm).toBe(onConfirm);
  });

  it("showConfirm respects custom options", () => {
    useDialogStore.getState().showConfirm({
      title: "Delete",
      message: "This will be permanent.",
      confirmLabel: "Delete Forever",
      danger: true,
      onConfirm: vi.fn(),
    });

    const dialog = useDialogStore.getState().confirmDialog;
    expect(dialog!.confirmLabel).toBe("Delete Forever");
    expect(dialog!.danger).toBe(true);
  });

  it("hideConfirm clears the dialog", () => {
    useDialogStore.getState().showConfirm({
      title: "Test",
      message: "msg",
      onConfirm: vi.fn(),
    });
    expect(useDialogStore.getState().confirmDialog).not.toBeNull();

    useDialogStore.getState().hideConfirm();
    expect(useDialogStore.getState().confirmDialog).toBeNull();
  });
});
