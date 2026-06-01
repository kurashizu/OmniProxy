"use client";

export default function Error({ error, reset }: { error: Error; reset: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full text-[#9ca3af] p-8">
      <div className="text-2xl mb-2 text-[#ef4444]">Something went wrong</div>
      <pre className="text-xs whitespace-pre-wrap max-w-2xl mb-4">{error.message}</pre>
      <button
        onClick={reset}
        className="rounded border border-[#252934] bg-[#0f1115] px-3 py-1.5 text-sm hover:border-[#3b82f6]"
      >
        Retry
      </button>
    </div>
  );
}
