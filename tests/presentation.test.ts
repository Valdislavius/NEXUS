import { expect, test } from 'vitest';
import { displaySpecs, metricValue, missingReason } from '../src/presentation';
import type { Component, Metric } from '../src/model';
const cpu: Component = { id:'cpu',kind:'cpu',name:'CPU',subtitle:'',metrics:[],specs:[{label:'Производитель',value:'GenuineIntel'},{label:'Сокет',value:'U3E1'}] };
test('CPU identifiers are normalized without guessing a socket',()=>{
 expect(displaySpecs(cpu)).toEqual([{label:'Производитель',value:'Intel'}]);
 expect(displaySpecs({...cpu,specs:[{label:'Сокет',value:'LGA1700'}]})).toEqual([{label:'Сокет',value:'LGA1700'}]);
});
test('SMART distinguishes no warnings, combined flags and missing readings',()=>{
 const m:Metric={id:'smart-warning',label:'SMART',value:0,unit:'',source:'NVMe SMART',at:'',primary:false};
 expect(metricValue(m)).toBe('Не обнаружены');
 expect(metricValue({...m,value:3})).toBe('Мало резервных блоков · Температура вне порога');
 expect(metricValue({...m,value:128})).toContain('0x80');
 expect(metricValue({...m,value:null})).toBe('Нет данных');
});

test('all diagnostic states have a readable reason without technical error strings',()=>{
 const m:Metric={id:'temperature',label:'Температура',value:null,unit:'°C',source:'PawnIO',at:'',primary:true,reason:'CreateFile failed path/private/user'};
 for(const [status,label] of [['unsupported','Датчик не поддерживается'],['permission_denied','Недостаточно прав'],['dependency_missing','Отсутствует необходимая зависимость'],['device_unresponsive','Устройство не отвечает'],['driver_no_data','Драйвер не предоставляет данные'],['invalid_response','Некорректный ответ'],['transient_error','Временная ошибка чтения']] as const) {
  expect(missingReason({...m,diagnostic:{status,code:'TECHNICAL_ERROR'}})).toBe(label);
 }
});
