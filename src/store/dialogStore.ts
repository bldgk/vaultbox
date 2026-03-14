import { create } from "zustand";

interface ConfirmDialogOptions {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  danger: boolean;
  onConfirm: () => void;
}

interface DialogState {
  confirmDialog: ConfirmDialogOptions | null;
  showConfirm: (opts: {
    title: string;
    message: string;
    confirmLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
  }) => void;
  hideConfirm: () => void;
}

export const useDialogStore = create<DialogState>((set) => ({
  confirmDialog: null,

  showConfirm: (opts) =>
    set({
      confirmDialog: {
        open: true,
        title: opts.title,
        message: opts.message,
        confirmLabel: opts.confirmLabel ?? "Confirm",
        danger: opts.danger ?? false,
        onConfirm: opts.onConfirm,
      },
    }),

  hideConfirm: () => set({ confirmDialog: null }),
}));
