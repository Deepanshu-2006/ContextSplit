import { NodeProps } from "@xyflow/react";
import { TextCard } from "../cards/TextCard";
import { ImageCard } from "../cards/ImageCard";
import { UrlCard } from "../cards/UrlCard";
import { FileCard } from "../cards/FileCard";

export function ItemNode({ data, selected }: NodeProps) {
  const { item } = data as any;
  
  if (!item) {
    return null;
  }
  
  switch (item.item_type) {
    case 'text':
      return <TextCard content={item.content} title={item.title} itemId={item.id} selected={selected} />;
    case 'image':
      return <ImageCard content={item.content} title={item.title} itemId={item.id} selected={selected} />;
    case 'url':
      return <UrlCard content={item.content} title={item.title} itemId={item.id} selected={selected} />;
    case 'file':
      return <FileCard content={item.content} title={item.title} itemId={item.id} selected={selected} />;
    default:
      return <FileCard content={item.content} title={item.title} itemId={item.id} selected={selected} />;
  }
}
