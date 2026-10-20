import {afterEach,expect,test,vi} from 'vitest';
import {cleanup,fireEvent,render,waitFor} from '@testing-library/react';
import {AppView} from '../src/App';
import type {Snapshot} from '../src/model';
afterEach(cleanup);
const snapshot:Snapshot={version:1,type:'snapshot',at:'2026-09-19T00:00:00Z',host:'test',platform:'windows',ready:true,components:[],warnings:[]};
const createMonitor=()=>({snapshot,error:'',interval:2000,changeInterval:vi.fn(async()=>{}),restart:vi.fn(async()=>{}),restarting:false,stale:false});
test('settings sends the selected interval',async()=>{const monitor=createMonitor();const view=render(<AppView monitor={monitor}/>);fireEvent.click(view.getByRole('button',{name:'Настройки',exact:true}));fireEvent.change(view.getByRole('combobox'),{target:{value:'5000'}});await waitFor(()=>expect(monitor.changeInterval).toHaveBeenCalledWith(5000));});
test('settings reports failure instead of pretending success',async()=>{const monitor=createMonitor();monitor.changeInterval.mockRejectedValueOnce(new Error('pipe closed'));const view=render(<AppView monitor={monitor}/>);fireEvent.click(view.getByRole('button',{name:'Настройки',exact:true}));fireEvent.change(view.getByRole('combobox'),{target:{value:'5000'}});expect(await view.findByRole('alert')).toHaveTextContent('Не удалось изменить интервал');});
test('collector error provides working retry action',()=>{const monitor={...createMonitor(),error:'Сборщик остановлен'};const view=render(<AppView monitor={monitor}/>);expect(view.getByRole('alert')).toBeVisible();fireEvent.click(view.getByRole('button',{name:/Повторить/}));expect(monitor.restart).toHaveBeenCalledOnce();});
test('stale readings are explicitly marked',()=>{const view=render(<AppView monitor={{...createMonitor(),stale:true}}/>);expect(view.getByRole('status')).toHaveTextContent('Показатели временно недоступны');});

test('a stalled collector never presents its last values as current',()=>{
 const monitor=createMonitor();monitor.snapshot={...snapshot,components:[{id:'cpu',kind:'cpu',name:'CPU',subtitle:'',specs:[],metrics:[{id:'load',label:'Загрузка',value:42,unit:'%',source:'test',at:snapshot.at,primary:true}]}]};
 const view=render(<AppView monitor={monitor}/>);expect(view.getByText('42 %')).toBeVisible();
 view.rerender(<AppView monitor={{...monitor,stale:true}}/>);expect(view.queryByText('42 %')).toBeNull();expect(view.getAllByText('Нет данных').length).toBeGreaterThan(0);
 view.rerender(<AppView monitor={monitor}/>);expect(view.getByText('42 %')).toBeVisible();
});

test('stale ATA attributes are hidden until a fresh snapshot arrives',()=>{
 const monitor=createMonitor();monitor.snapshot={...snapshot,components:[{id:'disk',kind:'storage',name:'SSD',subtitle:'',specs:[{label:'Интерфейс',value:'SATA'}],metrics:[],smart:[{id:9,name:'Power-On Hours',flags:0,current:99,worst:99,rawHex:'000000000017',rawValue:'23',source:'ATA SMART'}]}]};
 const view=render(<AppView monitor={monitor}/>);fireEvent.click(view.container.querySelector('button.card-summary')!);expect(view.getByRole('table')).toBeVisible();
 view.rerender(<AppView monitor={{...monitor,stale:true}}/>);expect(view.queryByRole('table')).toBeNull();
 view.rerender(<AppView monitor={monitor}/>);expect(view.getByRole('table')).toBeVisible();
});
