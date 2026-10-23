import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { Reports } from '../src/Reports';
import type { Component } from '../src/model';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
afterEach(() => { cleanup(); vi.mocked(invoke).mockReset(); });
const cpu: Component = { id: 'cpu', kind: 'cpu', name: 'CPU fixture', subtitle: '', specs: [], metrics: [] };
const gpu: Component = { ...cpu, id: 'gpu', kind: 'gpu', name: 'GPU fixture' };
const ram: Component = { ...cpu, id: 'ram', kind: 'ram', name: 'RAM fixture' };
const allSections = { summary: true, metrics: true, diagnostics: true, benchmarks: true };
const saved = { path: 'reports/report.xlsx', rows: 5 };

test('reports explicitly include every section and anonymization by default', async () => {
  vi.mocked(invoke).mockResolvedValue(saved);
  const view = render(<Reports components={[cpu, gpu]} enabled />);
  for (const input of within(view.getByRole('group', { name: 'Содержание отчёта' })).getAllByRole('checkbox')) expect(input).toBeChecked();
  fireEvent.click(view.getByRole('button', { name: 'Создать отчёт' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('report_generate', { request: expect.objectContaining({ format: 'xlsx', components: ['cpu', 'gpu'], anonymized: true, sections: allSections }) }));
  expect(await view.findByText('Отчёт сохранён · 5 строк')).toBeVisible();
});

test('unselected report sections are sent as explicit false values', async () => {
  vi.mocked(invoke).mockResolvedValue(saved);
  const view = render(<Reports components={[cpu]} enabled />);
  fireEvent.click(view.getByRole('checkbox', { name: /^История метрик/ }));
  fireEvent.click(view.getByRole('checkbox', { name: /^Диагностические события/ }));
  fireEvent.click(view.getByRole('button', { name: 'Создать отчёт' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('report_generate', { request: expect.objectContaining({ sections: { summary: true, metrics: false, diagnostics: false, benchmarks: true } }) }));
});

test('an empty report section selection cannot invoke native export', () => {
  const view = render(<Reports components={[cpu]} enabled />);
  for (const input of within(view.getByRole('group', { name: 'Содержание отчёта' })).getAllByRole('checkbox')) fireEvent.click(input);
  const create = view.getByRole('button', { name: 'Создать отчёт' });
  expect(create).toBeDisabled();
  expect(view.getByText('Выберите хотя бы один раздел отчёта.')).toBeVisible();
  fireEvent.click(create);
  expect(invoke).not.toHaveBeenCalled();
});

test.each([['Excel .xlsx', 'xlsx'], ['CSV', 'csv'], ['PDF', 'pdf']])('format %s preserves the selected period and explicit privacy choice', async (button, format) => {
  vi.mocked(invoke).mockResolvedValue(saved);
  const view = render(<Reports components={[cpu, gpu]} componentId="gpu" enabled />);
  expect(view.getByRole('checkbox', { name: 'CPU fixture' })).not.toBeChecked();
  expect(view.getByRole('checkbox', { name: 'GPU fixture' })).toBeChecked();
  fireEvent.click(view.getByRole('button', { name: button }));
  fireEvent.click(view.getByRole('button', { name: '7 дней' }));
  fireEvent.click(view.getByRole('checkbox', { name: /^Обезличенный отчёт/ }));
  expect(view.getByText(/Отчёт может содержать личные сведения/)).toBeVisible();
  fireEvent.click(view.getByRole('button', { name: 'Создать отчёт' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledOnce());
  const request = vi.mocked(invoke).mock.calls[0][1] as { request: { format: string; fromMs: number; toMs: number; components: string[]; anonymized: boolean; sections: typeof allSections } };
  expect(request.request).toMatchObject({ format, components: ['gpu'], anonymized: false, sections: allSections });
  expect(request.request.toMs - request.request.fromMs).toBe(168 * 3600000);
});

test('components arriving asynchronously get defaults only once and empty user choices stay empty', async () => {
  const view = render(<Reports components={[]} enabled />);
  expect(view.getByRole('button', { name: 'Создать отчёт' })).toBeDisabled();
  view.rerender(<Reports components={[cpu, gpu]} enabled />);
  await waitFor(() => expect(view.getByRole('checkbox', { name: 'CPU fixture' })).toBeChecked());
  expect(view.getByRole('checkbox', { name: 'GPU fixture' })).toBeChecked();
  fireEvent.click(view.getByRole('checkbox', { name: 'CPU fixture' }));
  fireEvent.click(view.getByRole('checkbox', { name: 'GPU fixture' }));
  view.rerender(<Reports components={[cpu, gpu, ram]} enabled />);
  expect(view.getByRole('button', { name: 'Создать отчёт' })).toBeDisabled();
  for (const name of ['CPU fixture', 'GPU fixture', 'RAM fixture']) expect(view.getByRole('checkbox', { name })).not.toBeChecked();
});

test('an asynchronously available component page defaults to that device only', async () => {
  const view = render(<Reports components={[]} componentId="gpu" enabled />);
  view.rerender(<Reports components={[cpu, gpu]} componentId="gpu" enabled />);
  await waitFor(() => expect(view.getByRole('checkbox', { name: 'GPU fixture' })).toBeChecked());
  expect(view.getByRole('checkbox', { name: 'CPU fixture' })).not.toBeChecked();
});

test('removed devices never remain hidden in the export selection', async () => {
  vi.mocked(invoke).mockResolvedValue(saved);
  const view = render(<Reports components={[cpu, gpu]} enabled />);
  view.rerender(<Reports components={[gpu]} enabled />);
  fireEvent.click(view.getByRole('button', { name: 'Создать отчёт' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('report_generate', { request: expect.objectContaining({ components: ['gpu'] }) }));
  view.rerender(<Reports components={[ram]} enabled />);
  expect(view.getByRole('button', { name: 'Создать отчёт' })).toBeDisabled();
  expect(view.getByRole('checkbox', { name: 'RAM fixture' })).not.toBeChecked();
});
