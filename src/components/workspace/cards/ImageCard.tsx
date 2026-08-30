import { Image as ImageIcon } from "lucide-react";
import { BaseCard } from "./BaseCard";

interface ImageCardProps {
  content: string;
  title?: string | null;
  itemId?: string;
  selected?: boolean;
}

export function ImageCard({ content, title, itemId, selected }: ImageCardProps) {
  return (
    <BaseCard itemId={itemId} selected={selected} icon={<ImageIcon size={14} />} title={title || "Image"} className="w-75">
      <div className="w-full flex justify-center bg-gray-50 rounded-lg overflow-hidden border border-gray-100">
        <img 
          src={content} 
          alt={title || "Captured Image"} 
          className="max-h-75 object-contain"
        />
      </div>
    </BaseCard>
  );
}
