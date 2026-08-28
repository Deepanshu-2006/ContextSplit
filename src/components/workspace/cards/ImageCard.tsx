import { Image as ImageIcon } from "lucide-react";
import { BaseCard } from "./BaseCard";

interface ImageCardProps {
  content: string;
  title?: string | null;
  itemId?: string;
}

export function ImageCard({ content, title, itemId }: ImageCardProps) {
  return (
    <BaseCard itemId={itemId} icon={<ImageIcon size={14} />} title={title || "Image"} className="w-[300px]">
      <div className="w-full flex justify-center bg-gray-50 rounded-lg overflow-hidden border border-gray-100">
        <img 
          src={content} 
          alt={title || "Captured Image"} 
          className="max-h-[300px] object-contain"
        />
      </div>
    </BaseCard>
  );
}
