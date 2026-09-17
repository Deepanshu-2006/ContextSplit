import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { RotateCcw } from 'lucide-react';

interface UndoToastProps {
  toastId: string | number;
  message: string;
  durationMs?: number;
  onUndo: () => void | Promise<void>;
}

export function UndoToast({
  toastId,
  message,
  durationMs = 5000,
  onUndo,
}: UndoToastProps) {
  const [progress, setProgress] = useState(100);

  useEffect(() => {
    const startTime = Date.now();
    const interval = setInterval(() => {
      const elapsed = Date.now() - startTime;
      const remainingPct = Math.max(0, 100 - (elapsed / durationMs) * 100);
      setProgress(remainingPct);
      if (remainingPct <= 0) {
        clearInterval(interval);
      }
    }, 20);

    return () => clearInterval(interval);
  }, [durationMs]);

  const handleUndoClick = async () => {
    toast.dismiss(toastId);
    await onUndo();
  };

  return (
    <div className="relative overflow-hidden w-[310px] bg-gray-950/95 backdrop-blur-2xl border border-white/12 text-white px-3.5 py-2.5 rounded-2xl shadow-[0_16px_40px_-6px_rgba(0,0,0,0.5)] flex items-center justify-between gap-3 select-none font-sans">
      <span className="text-[12.5px] font-medium text-white/95 truncate">
        {message}
      </span>

      <button
        onClick={handleUndoClick}
        className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-white/12 hover:bg-white/22 active:bg-white/28 border border-white/15 text-white text-[11.5px] font-medium transition-all shadow-2xs cursor-pointer shrink-0"
      >
        <RotateCcw size={11} className="text-gray-300" />
        <span>Undo</span>
      </button>

      {/* Decreasing Timer Progress Line */}
      <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-white/10 overflow-hidden">
        <div
          className="h-full bg-white/70"
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}

export function showUndoToast(options: {
  message: string;
  onUndo: () => void | Promise<void>;
  durationMs?: number;
}) {
  const duration = options.durationMs || 5000;
  toast.custom(
    (t) => (
      <UndoToast
        toastId={t}
        message={options.message}
        durationMs={duration}
        onUndo={options.onUndo}
      />
    ),
    { duration }
  );
}
