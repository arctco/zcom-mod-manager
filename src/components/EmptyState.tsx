import { PackageOpen } from "lucide-react";

export function EmptyState({ title, body, action }: { title: string; body: string; action?: React.ReactNode }) {
  return <div className="empty-state"><PackageOpen aria-hidden size={32} /><h2>{title}</h2><p>{body}</p>{action}</div>;
}
