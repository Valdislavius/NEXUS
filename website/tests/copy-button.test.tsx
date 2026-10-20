import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CopyButton } from '@/components/download/copy-button';
afterEach(() => vi.unstubAllGlobals());
describe('Копирование контрольной суммы', () => {
  it('передаёт точный текст в буфер и сообщает об успехе', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    render(<CopyButton text="ABC123" accessibleLabel="Копировать SHA-256" />);
    fireEvent.click(screen.getByRole('button', { name: 'Копировать SHA-256' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('ABC123'));
    expect(await screen.findByText('Скопировано')).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('Текст скопирован');
  });
  it('не заявляет успешное копирование при отказе браузера', async () => {
    vi.stubGlobal('navigator', { clipboard: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } });
    render(<CopyButton text="ABC123" accessibleLabel="Копировать SHA-256" />);
    fireEvent.click(screen.getByRole('button', { name: 'Копировать SHA-256' }));
    expect(await screen.findByText(/Не удалось скопировать/)).toBeVisible();
    expect(screen.queryByText('Скопировано')).not.toBeInTheDocument();
  });
});
