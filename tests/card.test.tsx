import { afterEach, expect, test } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { Card } from '../src/Card';
import { Dashboard } from '../src/App';
import { reconcileSnapshot } from '../src/model';
import type { Component, Snapshot } from '../src/model';
afterEach(cleanup);
const component: Component = {id:'cpu',kind:'cpu',name:'CPU',subtitle:'',specs:[],metrics:[]};
const snapshot: Snapshot = {version:1,type:'snapshot',at:'2026-09-19T00:00:00Z',host:'test',platform:'windows',ready:true,components:[component],warnings:[]};
test('cards expand independently and retain state on metric refresh', () => {
 const view=render(<><Card component={component}/><Card component={{...component,id:'gpu'}}/></>);
 const buttons=view.container.querySelectorAll('button.card-summary'); fireEvent.click(buttons[0]);fireEvent.click(buttons[1]);
 expect([...buttons].every(b=>b.getAttribute('aria-expanded')==='true')).toBe(true);
 view.rerender(<><Card component={{...component,name:'Updated'}}/><Card component={{...component,id:'gpu'}}/></>);
 expect(view.container.querySelector('button')?.getAttribute('aria-expanded')).toBe('true');
 fireEvent.click(view.container.querySelector('button')!);expect(view.container.querySelector('button')).toHaveAttribute('aria-expanded','false');
});
test('unsupported sensors and OEM placeholders are not displayed',()=>{
 const c={...component,kind:'gpu',specs:[{label:'Ревизия',value:'x.x'}],metrics:[{id:'temperature',label:'Температура',value:null,unit:'°C',source:'Windows',at:'',primary:true}]};
 const s=reconcileSnapshot(null,{...snapshot,components:[c]});const view=render(<Card component={s.components[0]}/>);
 expect(view.queryByText('Температура')).toBeNull();expect(view.queryByText('x.x')).toBeNull();
});
test('temporary failure retains a previously working sensor without retaining old reading',()=>{
 const m={id:'temperature',label:'Температура',value:42,unit:'°C',source:'NVMe',at:'',primary:true};
 const first=reconcileSnapshot(null,{...snapshot,components:[{...component,metrics:[m]}]});
 const next=reconcileSnapshot(first,{...snapshot,components:[{...component,metrics:[{...m,value:null}]}]});
 expect(next.components[0].metrics).toHaveLength(1);expect(next.components[0].metrics[0].value).toBeNull();
 const view=render(<Card component={next.components[0]}/>);expect(view.getByText('Нет данных')).toBeVisible();expect(view.queryByText('42 °C')).toBeNull();
});
test('filters select components without mixing disks and processors',()=>{
 const s={...snapshot,components:[{...component,specs:[{label:'Cores',value:'16'}]},{...component,id:'disk',kind:'storage',name:'SSD',specs:[{label:'Capacity',value:'1 TB'}]}]};
 const view=render(<Dashboard snapshot={s}/>);fireEvent.click(view.getByRole('button',{name:'Накопители',exact:true}));
 expect(view.getByRole('heading',{name:'SSD'})).toBeVisible();expect(view.queryByRole('heading',{name:'CPU'})).toBeNull();
 fireEvent.click(view.getByRole('button',{name:'Все компоненты'}));expect(view.getAllByRole('article')).toHaveLength(2);
});

test('CPU summary reserves temperature when access to the sensor is unavailable',()=>{
 const m={id:'temperature',label:'Температура',value:null,unit:'°C',source:'PawnIO',at:'',primary:true};
 const s=reconcileSnapshot(null,{...snapshot,components:[{...component,subtitle:'10 ядер · 16 потоков',metrics:[m]}]});
 const view=render(<Card component={s.components[0]}/>);expect(view.getByText('Нет данных')).toBeVisible();expect(view.getByText('10 ядер · 16 потоков')).toBeVisible();expect(view.queryByText('Ядра / потоки')).toBeNull();
});

test('expanded CPU gives the missing temperature reason without a fabricated value',()=>{
 const c:Component={...component,metrics:[{id:'temperature',label:'Температура',value:null,unit:'°C',source:'PawnIO',at:'',primary:true,diagnostic:{status:'permission_denied',code:'PAWNIO_ACCESS_DENIED'}}]};
 const view=render(<Card component={c}/>);fireEvent.click(view.getByRole('button',{expanded:false}));
 expect(view.getByText('Недостаточно прав')).toBeVisible();expect(view.getByText('Нет данных')).toBeVisible();expect(view.queryByText('0 °C')).toBeNull();expect(view.queryByText('PAWNIO_ACCESS_DENIED')).toBeNull();
});

test('SATA attributes separate normalized data from exact raw values and unknown units',()=>{
 const c:Component={...component,id:'disk',kind:'storage',name:'Samsung SSD',smart:[{id:177,name:'Wear Leveling Count',flags:19,current:98,worst:97,rawHex:'000000000017',rawValue:'23',source:'ATA SMART'},{id:241,name:'Total LBAs Written',flags:0,current:99,worst:99,rawHex:'FFFFFFFFFFFF',rawValue:'281474976710655',source:'ATA SMART'}]};
 const view=render(<Card component={c}/>);fireEvent.click(view.getByRole('button',{expanded:false}));
 const table=view.getByRole('table');expect(table).toHaveTextContent('Норм.');expect(table).toHaveTextContent('RAW');expect(table).toHaveTextContent('98');expect(table).toHaveTextContent('97');expect(table).toHaveTextContent('23');expect(table).toHaveTextContent('HEX FFFFFFFFFFFF');expect(table).toHaveTextContent('281474976710655');expect(table.textContent).not.toMatch(/%|ТиБ|ГиБ|исправен/);expect(view.getByText(/единицы зависят от модели/)).toBeVisible();
});
