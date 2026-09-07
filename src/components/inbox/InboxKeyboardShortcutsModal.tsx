import { useEffect } from 'react';
import { X, Keyboard, Sparkles, Search, HelpCircle } from 'lucide-react';
import { getModifierKeyName, isMacPlatform } from '../../store/inboxStore';

interface InboxKeyboardShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function InboxKeyboardShortcutsModal({
  isOpen,
  onClose,
}: InboxKeyboardShortcutsModalProps) {
  const modKey = getModifierKeyName();
  const isMac = isMacPlatform();

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const shortcutSections = [
    {
      title: 'Navigation & Focus',
      icon: <ArrowDownUpIcon className="text-indigo-500" size={16} />,
      shortcuts: [
        {
          keys: ['J', 'or', '↓'],
          description: 'Focus next item in the list',
        },
        {
          keys: ['K', 'or', '↑'],
          description: 'Focus previous item in the list',
        },
        {
          keys: ['X', 'or', 'Space'],
          description: 'Toggle selection of focused item',
        },
        {
          keys: [`${modKey}`, 'A'],
          description: 'Select all items in view',
        },
      ],
    },
    {
      title: 'Contextual Actions',
      icon: <Sparkles className="text-purple-500" size={16} />,
      shortcuts: [
        {
          keys: ['A'],
          secondaryKeys: [`${modKey}`, '↵'],
          description: 'Accept contextual suggestion (moves item to workspace)',
        },
        {
          keys: ['D'],
          secondaryKeys: ['⌫'],
          description: 'Dismiss suggestion (marks item understood)',
        },
        {
          keys: ['↵', 'or', 'O'],
          description: 'Inspect context evidence & item metadata',
        },
        {
          keys: ['R', 'or', 'C'],
          description: 'Open Context Candidate Review modal',
        },
      ],
    },
    {
      title: 'Search & Global',
      icon: <Search className="text-blue-500" size={16} />,
      shortcuts: [
        {
          keys: [`${modKey}`, 'K'],
          secondaryKeys: ['/'],
          description: 'Focus Inbox search & filters',
        },
        {
          keys: ['Esc'],
          description: 'Clear search query / Close modals / Deselect',
        },
        {
          keys: ['?'],
          description: 'Toggle this keyboard shortcuts cheatsheet',
        },
      ],
    },
  ];

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-150"
      onClick={onClose}
    >
      <div
        className="w-full max-w-xl bg-white rounded-2xl shadow-2xl border border-gray-200/90 overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-100 bg-gradient-to-r from-gray-50/80 via-white to-gray-50/50">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-xl bg-indigo-50 border border-indigo-100/80 flex items-center justify-center text-indigo-600">
              <Keyboard size={18} />
            </div>
            <div>
              <h2 className="text-base font-semibold text-gray-900 tracking-tight">
                Inbox Keyboard Shortcuts
              </h2>
              <p className="text-[12px] text-gray-500">
                Optimized for {isMac ? 'macOS (Command key)' : 'Windows / Linux (Control key)'}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors cursor-pointer"
            title="Close (Esc)"
          >
            <X size={18} />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 overflow-y-auto space-y-6">
          {shortcutSections.map((sec, idx) => (
            <div key={idx} className="space-y-3">
              <div className="flex items-center gap-2 text-xs font-semibold text-gray-700 tracking-wide uppercase">
                {sec.icon}
                <span>{sec.title}</span>
              </div>
              <div className="grid grid-cols-1 gap-2">
                {sec.shortcuts.map((sc, scIdx) => (
                  <div
                    key={scIdx}
                    className="flex items-center justify-between py-2 px-3 rounded-xl bg-gray-50/60 hover:bg-gray-50 border border-gray-100 transition-colors"
                  >
                    <span className="text-[13px] text-gray-700 font-medium">
                      {sc.description}
                    </span>
                    <div className="flex items-center gap-1.5 shrink-0 ml-3">
                      {sc.keys.map((k, kIdx) =>
                        k === 'or' ? (
                          <span key={kIdx} className="text-[11px] text-gray-400 font-sans px-0.5">
                            or
                          </span>
                        ) : (
                          <kbd
                            key={kIdx}
                            className="min-w-6 h-6 px-1.5 inline-flex items-center justify-center text-xs font-semibold font-mono text-gray-800 bg-white border border-gray-300/80 rounded-md shadow-2xs"
                          >
                            {k}
                          </kbd>
                        )
                      )}
                      {sc.secondaryKeys && (
                        <>
                          <span className="text-[11px] text-gray-400 font-sans px-0.5">/</span>
                          {sc.secondaryKeys.map((sk, skIdx) => (
                            <kbd
                              key={skIdx}
                              className="min-w-6 h-6 px-1.5 inline-flex items-center justify-center text-xs font-semibold font-mono text-gray-800 bg-white border border-gray-300/80 rounded-md shadow-2xs"
                            >
                              {sk}
                            </kbd>
                          ))}
                        </>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>

        {/* Footer info banner */}
        <div className="px-6 py-3 border-t border-gray-100 bg-gray-50/50 flex items-center justify-between text-xs text-gray-500">
          <div className="flex items-center gap-1.5">
            <HelpCircle size={14} className="text-gray-400" />
            <span>Shortcuts are disabled while typing in text inputs</span>
          </div>
          <div className="flex items-center gap-1 text-[11px] font-mono text-gray-400">
            <span>Press</span>
            <kbd className="px-1.5 py-0.5 bg-white border border-gray-200 rounded text-gray-700 font-bold">
              Esc
            </kbd>
            <span>to close</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function ArrowDownUpIcon(props: { className?: string; size?: number }) {
  return (
    <svg
      width={props.size || 16}
      height={props.size || 16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={props.className}
    >
      <path d="m3 16 4 4 4-4" />
      <path d="M7 20V4" />
      <path d="m21 8-4-4-4 4" />
      <path d="M17 4v16" />
    </svg>
  );
}
