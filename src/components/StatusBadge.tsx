import { AlertTriangle, CheckCircle2, CircleHelp, XCircle } from "lucide-react";
import type { Health } from "../types";

const icons = { good: CheckCircle2, warning: AlertTriangle, error: XCircle, unknown: CircleHelp };

export function StatusBadge({ status, children }: { status: Health; children: React.ReactNode }) {
  const Icon = icons[status];
  return <span className={`status status-${status}`}><Icon aria-hidden size={15} />{children}</span>;
}
