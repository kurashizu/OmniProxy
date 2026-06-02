"use client";

export function Dialog({ open, title, message, onClose }: {
  open: boolean; title: string; message: string; onClose: () => void;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="rounded-lg border border-border bg-card p-6 max-w-sm w-full mx-4">
        <h2 className="text-text font-semibold mb-2">{title}</h2>
        <p className="text-muted text-sm mb-4 whitespace-pre-wrap">{message}</p>
        <button
          onClick={onClose}
          className="rounded-md bg-primary px-4 py-1.5 text-sm text-white hover:bg-[#2563eb] transition-colors"
        >
          OK
        </button>
      </div>
    </div>
  );
}
