import React from 'react';
import { useWorkspaceStore } from '../../store/workspaceStore';
import { Check, X } from 'lucide-react';

export const CaptureTarget: React.FC = () => {
  const { captureState } = useWorkspaceStore();

  if (captureState === 'hidden') return null;

  const baseClasses = "fixed top-4 left-1/2 -translate-x-1/2 z-50 rounded-2xl flex items-center justify-center transition-all duration-300 ease-out border backdrop-blur-md overflow-hidden pointer-events-none";
  
  let stateClasses = "";
  let content = <span className="font-medium text-sm text-gray-800">Drop to ContextSplit</span>;

  switch (captureState) {
    case 'visible':
      stateClasses = "w-32 h-10 bg-white/80 border-black/5 shadow-sm scale-100 opacity-90";
      content = <span className="font-medium text-xs text-gray-500">ContextSplit</span>;
      break;
    case 'approaching':
      stateClasses = "w-40 h-12 bg-white/90 border-black/10 shadow-md scale-105 opacity-100";
      break;
    case 'active':
      stateClasses = "w-48 h-14 bg-blue-50/90 border-blue-200 shadow-xl scale-110 opacity-100 ring-4 ring-blue-500/20";
      content = <span className="font-semibold text-sm text-blue-700">Drop to ContextSplit</span>;
      break;
    case 'success':
      stateClasses = "w-40 h-12 bg-green-500/90 border-green-600 shadow-lg scale-105 opacity-100";
      content = (
        <div className="flex items-center gap-2 text-white">
          <Check size={18} strokeWidth={3} />
          <span className="font-semibold text-sm">Saved</span>
        </div>
      );
      break;
    case 'failure':
      stateClasses = "w-40 h-12 bg-red-500/90 border-red-600 shadow-lg scale-100 opacity-100 animate-pulse";
      content = (
        <div className="flex items-center gap-2 text-white">
          <X size={18} strokeWidth={3} />
          <span className="font-semibold text-sm">Failed</span>
        </div>
      );
      break;
  }

  return (
    <div className={`${baseClasses} ${stateClasses}`}>
      <div className="flex items-center justify-center w-full h-full relative">
        <div className="absolute inset-0 bg-gradient-to-b from-white/40 to-transparent pointer-events-none mix-blend-overlay"></div>
        <div className="relative z-10 flex items-center justify-center">
          {content}
        </div>
      </div>
    </div>
  );
};
