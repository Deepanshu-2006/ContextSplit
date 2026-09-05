import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Check } from 'lucide-react';

export const CaptureToastRenderer = () => {
  const [toast, setToast] = useState<{title: string, content: string, type: string} | null>(null);

  useEffect(() => {
    let timeout: ReturnType<typeof setTimeout>;
    
    const unlisten = listen('capture://event', async (event: any) => {
      const payload = event.payload;
      
      const title = payload.metadata?.title || payload.source_metadata?.window_title || payload.source_application || "Captured to Inbox";
      
      let content = payload.content || "";
      if (payload.content_type === "url") {
        content = payload.metadata?.domain || content;
      } else if (payload.content_type === "file") {
        content = payload.metadata?.filename || content;
      } else if (payload.content_type === "image") {
        content = "Image saved to Inbox";
      } else if (content.length > 45) {
        content = content.substring(0, 45) + "...";
      }

      setToast({
        title,
        content,
        type: payload.content_type || "text"
      });
      
      const win = getCurrentWindow();
      await win.show();
      
      clearTimeout(timeout);
      timeout = setTimeout(async () => {
        setToast(null);
        await win.hide();
      }, 3000);
    });

    return () => {
      unlisten.then(f => f());
      clearTimeout(timeout);
    };
  }, []);

  if (!toast) return null;

  return (
    <div className="bg-white/95 dark:bg-zinc-900/95 backdrop-blur-xl rounded-2xl shadow-2xl border border-black/10 dark:border-white/15 p-3.5 flex items-center gap-3 w-[300px] pointer-events-auto transition-all duration-300">
      <div className="bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 rounded-xl p-2 shrink-0 border border-emerald-500/20">
        <Check size={18} strokeWidth={2.5} />
      </div>
      <div className="flex-1 min-w-0 overflow-hidden text-left">
        <div className="flex items-center gap-1.5">
          <span className="text-[12px] font-bold text-gray-900 dark:text-white tracking-tight leading-tight truncate">
            {toast.title}
          </span>
        </div>
        <p className="text-[11px] text-gray-500 dark:text-zinc-400 truncate mt-0.5 font-medium leading-tight">
          {toast.content}
        </p>
      </div>
    </div>
  );
};
