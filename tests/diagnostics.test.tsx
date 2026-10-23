import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { AppView } from '../src/App';
import { diagnosticReportSchema, downloadDiagnostics } from '../src/diagnostics';
import type { DiagnosticReport } from '../src/diagnostics';
import type { Snapshot } from '../src/model';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.mocked(invoke).mockReset(); vi.mocked(isTauri).mockReturnValue(true); vi.useRealTimers(); });
const report:DiagnosticReport={version:1,type:'diagnostics',appVersion:'0.3.1',at:'2026-10-08T00:00:00Z',privacy:{serialNumbers:false,userIdentifiers:false,ipAddresses:false},collector:{running:true,snapshotReady:true},sensors:[{component:'cpu-1',sensor:'temperature',source:'PawnIO',status:'permission_denied',code:'PAWNIO_ACCESS_DENIED'}]};
const snapshot:Snapshot={version:1,type:'snapshot',at:report.at,host:'private-host',platform:'windows',ready:true,warnings:[],components:[{id:'cpu',kind:'cpu',name:'CPU',subtitle:'',specs:[],metrics:[{id:'temperature',label:'Температура',value:null,unit:'°C',source:'PawnIO',at:report.at,primary:true,diagnostic:{status:'permission_denied',code:'PAWNIO_ACCESS_DENIED'}}]}]};
const monitor=()=>({snapshot,error:'',interval:2000,changeInterval:vi.fn(async()=>{}),restart:vi.fn(async()=>{}),restarting:false,stale:false});

test('export reconstruction omits arbitrary snapshots and sensitive metadata',()=>{
 const parsed=diagnosticReportSchema.parse({...report,host:'private-host',serialNumber:'private-serial',user:'private-user',ip:'192.0.2.1',snapshot,sensors:report.sensors.map(s=>({...s,name:'private-device',rawValue:'secret-data'}))});
 const json=JSON.stringify(parsed);for(const value of ['private-host','private-serial','private-user','192.0.2.1','private-device','secret-data'])expect(json).not.toContain(value);
 expect(diagnosticReportSchema.safeParse({...report,privacy:{...report.privacy,serialNumbers:true}}).success).toBe(false);
});

test('diagnostic mode inspects existing statuses without any device or IPC request',()=>{
 const state=monitor();const view=render(<AppView monitor={state}/>);fireEvent.click(view.getByRole('button',{name:'Настройки',exact:true}));fireEvent.click(view.getByRole('checkbox',{name:'Безопасный диагностический режим'}));
 expect(view.getByText('Код: PAWNIO_ACCESS_DENIED')).toBeVisible();expect(view.getByText(/Дополнительные команды устройствам не отправляются/)).toBeVisible();expect(invoke).not.toHaveBeenCalled();expect(state.restart).not.toHaveBeenCalled();
});

test('export requests only the sanitized command and downloads a local JSON file',async()=>{
 vi.useFakeTimers();vi.mocked(invoke).mockResolvedValueOnce(report);
 const create=vi.fn(()=> 'blob:local-diagnostics');const revoke=vi.fn();vi.stubGlobal('URL',class extends URL {static createObjectURL=create;static revokeObjectURL=revoke;});
 const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{});
 await downloadDiagnostics();expect(invoke).toHaveBeenCalledExactlyOnceWith('export_diagnostics');expect(create).toHaveBeenCalledOnce();expect(create.mock.calls[0][0]).toBeInstanceOf(Blob);expect(click).toHaveBeenCalledOnce();expect(document.querySelector('a[download]')).toBeNull();vi.advanceTimersByTime(1000);expect(revoke).toHaveBeenCalledWith('blob:local-diagnostics');vi.unstubAllGlobals();
});

test('malformed diagnostic output never becomes a download',async()=>{
 vi.mocked(invoke).mockResolvedValueOnce(snapshot);const click=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{});
 await expect(downloadDiagnostics()).rejects.toThrow();expect(click).not.toHaveBeenCalled();
});

test('export failure shows a clear message instead of raw backend errors',async()=>{
 vi.mocked(invoke).mockRejectedValueOnce(new Error('C:/Users/private-user 192.0.2.1'));
 const view=render(<AppView monitor={monitor()}/>);fireEvent.click(view.getByRole('button',{name:'Настройки',exact:true}));fireEvent.click(view.getByRole('button',{name:'Экспорт диагностического отчёта'}));await waitFor(()=>expect(view.getByRole('alert')).toHaveTextContent('Не удалось сохранить диагностический отчёт'));expect(view.queryByText(/private-user/)).toBeNull();
});

test('browser preview disables export with a useful explanation',()=>{
 vi.mocked(isTauri).mockReturnValue(false);const view=render(<AppView monitor={monitor()}/>);fireEvent.click(view.getByRole('button',{name:'Настройки',exact:true}));expect(view.getByRole('button',{name:'Экспорт диагностического отчёта'})).toBeDisabled();expect(view.getByText('Экспорт доступен в настольном приложении.')).toBeVisible();
});
