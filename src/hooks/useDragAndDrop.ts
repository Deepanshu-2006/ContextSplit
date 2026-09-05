import { useEffect } from 'react';
import { useWorkspaceStore } from '../store/workspaceStore';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function useDragAndDrop() {
  const { setCaptureState } = useWorkspaceStore();

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
        } else {
          setCaptureState('visible');
        }
      }
    };

    const handleDragOver = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      
      if (isNotch) {
        // Check if near center (Y axis is less relevant for notch, let's say X is near center)
        const isNearCenter = e.clientX > window.innerWidth * 0.2 && e.clientX < window.innerWidth * 0.8;
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
      if (dragCounter === 0) {
        if (isNotch) {
          await invoke('notch_drag_leave');
        } else if (isWindowsOrb) {
          await invoke('windows_target_drag_leave');
        } else {
          setCaptureState('hidden');
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
        } else {
          setCaptureState('hidden');
        }
        return;
      }

      if (isNotch) {
        // For Notch Target, check if drop is accepted (i.e., state is Activated)
        const accepted = await invoke<boolean>('notch_drop');
        if (!accepted) {
          return;
        }
      } else if (isWindowsOrb) {
        // For Windows Orb Target, check if drop is accepted (i.e., state is Activated)
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
            title: null,
            source: 'drag_drop',
            filename: file.name,
            mime_type: file.type || null,
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
          if (!isNotch && !isWindowsOrb) {
            setCaptureState('success');
            setTimeout(() => setCaptureState('hidden'), 2000);
          }
        } else {
          if (!isNotch && !isWindowsOrb) {
            setCaptureState('failure');
            setTimeout(() => setCaptureState('hidden'), 2000);
          }
        }
      } catch (err) {
        console.error('Failed to process drop:', err);
        if (!isNotch && !isWindowsOrb) {
          setCaptureState('failure');
          setTimeout(() => setCaptureState('hidden'), 2000);
        }
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
  }, [setCaptureState]);
}
