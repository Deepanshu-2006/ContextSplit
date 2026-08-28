import { useEffect } from 'react';
import { useWorkspaceStore } from '../store/workspaceStore';
import { invoke } from '@tauri-apps/api/core';

export function useDragAndDrop() {
  const { setCaptureState } = useWorkspaceStore();

  useEffect(() => {
    let dragCounter = 0;

    const handleDragEnter = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter++;
      if (dragCounter === 1) {
        setCaptureState('visible');
      }
    };

    const handleDragOver = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      // Visual proximity state is now handled independently in CaptureTarget
    };

    const handleDragLeave = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter--;
      if (dragCounter === 0) {
        setCaptureState('hidden');
      }
    };

    const handleDrop = async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter = 0;
      
      const dt = e.dataTransfer;
      if (!dt) {
        setCaptureState('hidden');
        return;
      }

      try {
        let req = null;

        // 1. Check for files (images or generic files)
        if (dt.files && dt.files.length > 0) {
          const file = dt.files[0];
          const isImage = file.type.startsWith('image/');
          
          // Convert file to base64 data URI so Rust can handle it easily
          // In a production app with huge files we'd just send the path, 
          // but browser security prevents getting absolute paths easily without Tauri plugins.
          // Since we are prototyping cross-platform, FileReader is robust.
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
            source: 'drag_and_drop',
            filename: file.name,
            mime_type: file.type || null,
            original_url: null,
            size: file.size,
          };
        } 
        // 2. Check for URLs
        else if (dt.getData('text/uri-list')) {
          const url = dt.getData('text/uri-list');
          req = {
            data_type: 'url',
            content: url,
            title: null,
            source: 'drag_and_drop',
            filename: null,
            mime_type: 'text/uri-list',
            original_url: url,
            size: url.length,
          };
        } 
        // 3. Check for plain text
        else if (dt.getData('text/plain')) {
          const text = dt.getData('text/plain');
          req = {
            data_type: 'text',
            content: text,
            title: null,
            source: 'drag_and_drop',
            filename: null,
            mime_type: 'text/plain',
            original_url: null,
            size: text.length,
          };
        }

        if (req) {
          await invoke('handle_capture', { req });
          setCaptureState('success');
          // Hide success message after 2 seconds
          setTimeout(() => setCaptureState('hidden'), 2000);
        } else {
          setCaptureState('failure');
          setTimeout(() => setCaptureState('hidden'), 2000);
        }
      } catch (err) {
        console.error('Failed to process drop:', err);
        setCaptureState('failure');
        setTimeout(() => setCaptureState('hidden'), 2000);
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
