import { explorer, type Run } from "@/lib/run";

export default function TxLink({ run, sig, label }: { run: Run; sig?: string; label?: string }) {
  if (!sig) return <span className="muted">—</span>;
  const href = explorer(run, sig);
  const text = label ?? `${sig.slice(0, 6)}…${sig.slice(-4)}`;
  return href ? <a href={href} rel="noopener" target="_blank">{text}<span className="skip"> (탐색기에서 열기)</span></a> : <code title={sig}>{text}</code>;
}
