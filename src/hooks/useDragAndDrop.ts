import { useEffect } from 'react';
import { useWorkspaceStore } from '../store/workspaceStore';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function useDragAndDrop() {
  useEffect(() => {
    let dragCounter = 0;
    const windowLabel = typeof window !== 'undefined' ? getCurrentWindow().label : '';
    const isNotch = windowLabel === "macos-notch-target";
    const isWindowsOrb = windowLabel === "windows-orb-target";

    const handleDragEnter = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter++;
      if (dragCounter === 1) {
        if (isNotch) {
          await invoke('notch_drag_enter');
        } else if (isWindowsOrb) {
          await invoke('windows_target_drag_enter');
        }
      }
    };

    const handleDragOver = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      
      if (isNotch) {
        const isNearCenter = e.clientX > window.innerWidth * 0.15 && e.clientX < window.innerWidth * 0.85;
        invoke('notch_cursor_moved', { cursorX: e.clientX, cursorY: e.clientY }).catch(() => {});
        await invoke('notch_drag_over', { isNearCenter });
      } else if (isWindowsOrb) {
        const isInsideTarget = e.clientX > window.innerWidth * 0.2 && e.clientX < window.innerWidth * 0.8 && e.clientY < 120;
        await invoke('windows_target_drag_over', { isInsideTarget });
      }
    };

    const handleDragLeave = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter--;
      if (dragCounter <= 0) {
        dragCounter = 0;
        if (isNotch) {
          await invoke('notch_drag_leave');
        } else if (isWindowsOrb) {
          await invoke('windows_target_drag_leave');
        }
      }
    };

    const handleDrop = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter = 0;
      
      const dt = e.dataTransfer;
      if (!dt) {
        if (isNotch) {
          await invoke('notch_drag_leave');
        } else if (isWindowsOrb) {
          await invoke('windows_target_drag_leave');
        }
        return;
      }

      if (isNotch) {
        // Native Cocoa DragDrop in macos.rs handles drops on the notch target directly and instantly,
        // and it manages the notch state machine (including reset). We do not need to invoke 'notch_drop' from JS.
        return;
      } else if (isWindowsOrb) {
        const accepted = await invoke<boolean>('windows_target_drop');
        if (!accepted) {
          return;
        }
      }

      try {
        let req = null;
        const currentWs = useWorkspaceStore.getState().activeWorkspaceId;
        const targetWs = currentWs === 'inbox' ? null : currentWs;

        // 1. Check for files (images or generic files)
        if (dt.files && dt.files.length > 0) {
          const file = dt.files[0];
          const isImage = file.type.startsWith('image/');
          
          const dataUri = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = (ev) => resolve(ev.target?.result as string);
            reader.onerror = (err) => reject(err);
            reader.readAsDataURL(file);
          });

          req = {
            data_type: isImage ? 'image' : 'file',
            content: dataUri,
            title: file.name,
            source: 'drag_drop',
            filename: file.name,
            mime_type: file.type || (isImage ? 'image/png' : null),
            original_url: null,
            size: file.size,
            workspace_id: targetWs,
          };
        } 
        // 2. Check for URLs
        else if (dt.getData('text/uri-list')) {
          const url = dt.getData('text/uri-list');
          req = {
            data_type: 'url',
            content: url,
            title: null,
            source: 'drag_drop',
            filename: null,
            mime_type: 'text/uri-list',
            original_url: url,
            size: url.length,
            workspace_id: targetWs,
          };
        } 
        // 3. Check for plain text
        else if (dt.getData('text/plain')) {
          const text = dt.getData('text/plain');
          req = {
            data_type: 'text',
            content: text,
            title: null,
            source: 'drag_drop',
            filename: null,
            mime_type: 'text/plain',
            original_url: null,
            size: text.length,
            workspace_id: targetWs,
          };
        }

        if (req) {
          await invoke('handle_capture', { req });
        }
      } catch (err) {
        console.error('Failed to process drop:', err);
      }
    };

    window.addEventListener('dragenter', handleDragEnter);
    window.addEventListener('dragover', handleDragOver);
    window.addEventListener('dragleave', handleDragLeave);
    window.addEventListener('drop', handleDrop);

    return () => {
      window.removeEventListener('dragenter', handleDragEnter);
      window.removeEventListener('dragover', handleDragOver);
      window.removeEventListener('dragleave', handleDragLeave);
      window.removeEventListener('drop', handleDrop);
    };
  }, []);
}
