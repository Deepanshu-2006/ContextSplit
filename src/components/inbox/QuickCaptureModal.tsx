import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { X, Plus, Clipboard, Link2, FileText } from 'lucide-react';
import { db } from '../../services/db';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { toast } from 'sonner';

interface QuickCaptureModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function QuickCaptureModal({ isOpen, onClose }: QuickCaptureModalProps) {
  const [content, setContent] = useState('');
  const [title, setTitle] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (isOpen) {
      setContent('');
      setTitle('');
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 50);
    }
  }, [isOpen]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isOpen) return;
      if (e.key === 'Escape') {
        onClose();
      } else if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        handleSubmit();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, content, title]);

  if (!isOpen) return null;

  const handlePasteClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text) {
        setContent(text);
        if (!title && text.length > 0) {
          const firstLine = text.trim().split('\n')[0].slice(0, 60);
          setTitle(firstLine);
        }
      }
    } catch (err) {
      console.warn('Could not read clipboard:', err);
    }
  };

  const handleSubmit = async () => {
    const trimmedContent = content.trim();
    if (!trimmedContent) return;

    setIsSubmitting(true);
    try {
      // Detect type: URL or text
      const isUrl = /^https?:\/\/[^\s]+$/i.test(trimmedContent);
      const itemType = isUrl ? 'url' : 'text';
      const itemTitle = title.trim() || (isUrl ? trimmedContent : trimmedContent.split('\n')[0].slice(0, 50));

      const newItem = await db.createItem(
        null, // null = Inbox
        itemType,
        trimmedContent,
        itemTitle,
        JSON.stringify({ captured_via: 'manual_quick_capture' })
      );

      // Optimistically update workspace store
      const state = useWorkspaceStore.getState();
      useWorkspaceStore.setState({
        activeItems: [newItem, ...state.activeItems],
        inboxCount: state.inboxCount + 1,
      });

      toast.success('Captured to Inbox', {
        description: itemTitle,
        duration: 2500,
      });

      onClose();
    } catch (err) {
      console.error('Failed to capture item:', err);
      toast.error('Failed to capture item');
    } finally {
      setIsSubmitting(false);
    }
  };

  const isUrl = /^https?:\/\/[^\s]+$/i.test(content.trim());

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 backdrop-blur-xs p-4"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-2xl shadow-2xl border border-black/10 w-full max-w-lg overflow-hidden flex flex-col transition-all"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-black/5">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 rounded-md bg-gray-900 text-white flex items-center justify-center">
              <Plus size={14} strokeWidth={2.5} />
            </div>
            <h2 className="text-[15px] font-semibold text-gray-900">Capture to Inbox</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1 text-gray-400 hover:text-gray-700 rounded-md transition-colors cursor-pointer"
          >
            <X size={16} />
          </button>
        </div>

        {/* Modal Body */}
        <div className="p-5 flex flex-col gap-3">
          <div>
            <input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Title (optional)"
              className="w-full text-sm font-medium text-gray-900 placeholder-gray-400 bg-gray-50/70 hover:bg-gray-50 focus:bg-white border border-black/8 rounded-lg px-3 py-2 outline-none focus:ring-1 focus:ring-black/20 transition-all"
            />
          </div>

          <div className="relative">
            <textarea
              ref={textareaRef}
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder="Paste or write text, link, note, or ideas..."
              rows={5}
              className="w-full text-sm text-gray-800 placeholder-gray-400 bg-gray-50/70 hover:bg-gray-50 focus:bg-white border border-black/8 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black/20 transition-all resize-none leading-relaxed"
            />
            {content.length > 0 && (
              <div className="absolute bottom-2.5 right-3 flex items-center gap-1.5 text-[11px] text-gray-400 font-mono pointer-events-none">
                {isUrl ? (
                  <span className="flex items-center gap-1 text-blue-600">
                    <Link2 size={11} /> Link
                  </span>
                ) : (
                  <span className="flex items-center gap-1 text-gray-500">
                    <FileText size={11} /> Text
                  </span>
                )}
              </div>
            )}
          </div>

          <div className="flex items-center justify-between pt-1">
            <button
              type="button"
              onClick={handlePasteClipboard}
              className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-800 bg-gray-100 hover:bg-gray-200/80 px-2.5 py-1.5 rounded-lg transition-colors cursor-pointer"
            >
              <Clipboard size={13} />
              <span>Paste from Clipboard</span>
            </button>

            <span className="text-[11px] text-gray-400">
              <kbd className="font-mono bg-gray-100 px-1 py-0.5 rounded border border-black/5">⌘</kbd> + <kbd className="font-mono bg-gray-100 px-1 py-0.5 rounded border border-black/5">Enter</kbd> to save
            </span>
          </div>
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-end gap-2.5 px-5 py-3.5 bg-gray-50/80 border-t border-black/5">
          <button
            type="button"
            onClick={onClose}
            className="px-3.5 py-1.5 rounded-lg text-xs font-medium text-gray-600 hover:text-gray-800 hover:bg-black/5 transition-colors cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!content.trim() || isSubmitting}
            className="px-4 py-1.5 rounded-lg text-xs font-semibold text-white bg-gray-900 hover:bg-black disabled:opacity-40 disabled:cursor-not-allowed transition-colors cursor-pointer shadow-xs"
          >
            {isSubmitting ? 'Capturing...' : 'Capture to Inbox'}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
