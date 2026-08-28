import { BaseCard } from "./BaseCard";
import { File } from "lucide-react";

interface FileCardProps {
  content: string; // The base64 Data URI or file path
  title?: string | null;
  itemId?: string;
  selected?: boolean;
}

export function FileCard({ content, title, itemId, selected }: FileCardProps) {
  const displayTitle = title || "Generic File";

  return (
    <BaseCard icon={<File size={14} />} title={displayTitle} itemId={itemId} selected={selected}>
      <div className="flex flex-col items-center justify-center py-6 px-4 bg-gray-50 rounded-lg border border-gray-100">
        <div className="w-12 h-12 bg-white rounded-xl shadow-sm border border-gray-200 flex items-center justify-center mb-3">
          <File className="text-gray-400" size={24} strokeWidth={1.5} />
        </div>
        <p className="text-sm font-medium text-gray-700 text-center line-clamp-2">
          {displayTitle}
        </p>
        <p className="text-xs text-gray-500 mt-1">
          {content.startsWith('data:') ? 'Local File' : 'File'}
        </p>
      </div>
    </BaseCard>
  );
}
