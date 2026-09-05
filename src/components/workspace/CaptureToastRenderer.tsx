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
      
      const title = payload.metadata?.title || payload.source_metadata?.window_title || payload.source_application || "Captured";
      
      let content = payload.content || "";
      if (payload.content_type === "url") {
        content = payload.metadata?.domain || content;
      } else if (payload.content_type === "file") {
        content = payload.metadata?.filename || content;
      } else if (content.length > 50) {
        content = content.substring(0, 50) + "...";
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
    <div className="bg-white rounded-xl shadow-[0_8px_30px_rgb(0,0,0,0.12)] border border-gray-100 p-3.5 flex items-start gap-3.5 w-[280px] pointer-events-auto transition-all duration-300">
      <div className="bg-green-100 text-green-600 rounded-full p-1.5 mt-0.5 shrink-0">
        <Check size={16} strokeWidth={3} />
      </div>
      <div className="flex-1 min-w-0 overflow-hidden text-left">
        <h4 className="text-[13px] font-semibold text-gray-900 truncate leading-tight">{toast.title}</h4>
        <p className="text-[11px] text-gray-500 truncate mt-1 leading-tight">{toast.content}</p>
      </div>
    </div>
  );
};
