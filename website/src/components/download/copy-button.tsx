'use client';
import { useState } from 'react';
import { Copy, Check } from 'lucide-react';
export function CopyButton({ text, label = 'Копировать', accessibleLabel }: { text: string; label?: string; accessibleLabel: string }) {
  const [status, setStatus] = useState<'idle' | 'success' | 'error'>('idle');
  async function copy() {
    try { await navigator.clipboard.writeText(text); setStatus('success'); }
    catch { setStatus('error'); }
  }
  return <div><button type="button" className="copy-button" aria-label={accessibleLabel} onClick={copy}>{status === 'success' ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}{status === 'success' ? 'Скопировано' : label}</button><span className={status === 'error' ? 'copy-error' : 'sr-only'} role="status">{status === 'error' ? 'Не удалось скопировать. Выделите и скопируйте текст вручную.' : status === 'success' ? 'Текст скопирован в буфер обмена.' : ''}</span></div>;
}
