import { Link, ExternalLink } from "lucide-react";
import { BaseCard } from "./BaseCard";

export function UrlCard({ content, title }: { content: string, title?: string }) {
  let domain = "";
  try {
    const url = new URL(content);
    domain = url.hostname;
  } catch (e) {
    domain = content;
  }

  return (
    <BaseCard icon={<Link size={14} />} title={domain}>
      <div className="flex flex-col gap-2">
        <h4 className="text-sm font-medium text-gray-900 line-clamp-2">
          {title || content}
        </h4>
        <a 
          href={content}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-600 font-medium w-fit"
        >
          <ExternalLink size={12} />
          Open Link
        </a>
      </div>
    </BaseCard>
  );
}
