import { NodeProps } from "@xyflow/react";
import { TextCard } from "../cards/TextCard";
import { ImageCard } from "../cards/ImageCard";
import { UrlCard } from "../cards/UrlCard";

export function ItemNode({ data }: NodeProps) {
  const { item } = data as any; // Using any for simplicity here, it expects the Item object
  
  if (!item) {
    return null;
  }
  
  switch (item.item_type) {
    case 'text':
      return <TextCard content={item.content} title={item.title} />;
    case 'image':
      return <ImageCard content={item.content} title={item.title} />;
    case 'url':
      return <UrlCard content={item.content} title={item.title} />;
    default:
      // Fallback for file or other unknown types
      return <TextCard content={item.content} title={item.title} />;
  }
}
