import {expect,test} from 'vitest';
import {formatMetric,snapshotSchema,metricSchema,smartAttributeSchema,reconcileSnapshot} from '../src/model';
const metric={id:'x',label:'x',value:null,unit:'%',source:'test',at:'2026-10-20T00:00:00Z',primary:true};
test('missing sensors never become zero',()=>{expect(formatMetric(metric)).toBe('Нет данных');expect(formatMetric({...metric,value:0})).toBe('0 %');});
test('unsupported protocol and malformed readings are rejected',()=>{expect(snapshotSchema.safeParse({version:2}).success).toBe(false);for(const value of [-1,NaN,Infinity,'12'])expect(metricSchema.safeParse({...metric,value}).success).toBe(false);});
test('binary memory and decimal link units',()=>{expect(formatMetric({...metric,value:1073741824,unit:'bytes'})).toBe('1 ГиБ');expect(formatMetric({...metric,value:1000000000,unit:'bit/s'})).toBe('1 Гбит/с');expect(formatMetric({...metric,value:38.85,unit:'°C'})).toBe('39 °C');});

test('version 1 keeps optional structured reasons without weakening numeric validation',()=>{
 const statuses=['unsupported','permission_denied','dependency_missing','device_unresponsive','driver_no_data','invalid_response','transient_error'];
 for(const status of statuses){const parsed=metricSchema.parse({...metric,diagnostic:{status,code:'SENSOR_READ_FAILED'},description:'Описание датчика'});expect(parsed.diagnostic?.status).toBe(status);expect(parsed.value).toBeNull();}
 expect(metricSchema.safeParse({...metric,diagnostic:{status:'healthy',code:'x'}}).success).toBe(false);
 expect(metricSchema.safeParse({...metric,value:NaN,diagnostic:{status:'invalid_response',code:'x'}}).success).toBe(false);
});

test('ATA raw values remain strings and invalid attribute bytes are rejected',()=>{
 const attribute={id:241,name:'Total LBAs Written',flags:0,current:99,worst:98,rawHex:'FFFFFFFFFFFF',rawValue:'281474976710655',source:'ATA SMART'};
 const parsed=smartAttributeSchema.parse(attribute);expect(parsed.rawValue).toBe('281474976710655');expect(parsed.value).toBeUndefined();expect(parsed.unit).toBeUndefined();
 expect(smartAttributeSchema.safeParse({...attribute,current:256}).success).toBe(false);
 expect(smartAttributeSchema.safeParse({...attribute,rawHex:'not bytes'}).success).toBe(false);
 expect(smartAttributeSchema.safeParse({...attribute,rawValue:281474976710655}).success).toBe(false);
});

test('an unavailable sensor with a diagnostic remains available for inspection',()=>{
 const input=snapshotSchema.parse({version:1,type:'snapshot',at:metric.at,host:'',platform:'windows',ready:true,warnings:[],components:[{id:'gpu',kind:'gpu',name:'GPU',subtitle:'',specs:[],metrics:[{...metric,diagnostic:{status:'driver_no_data',code:'GPU_NO_READING'}}]}]});
 expect(reconcileSnapshot(null,input).components[0].metrics[0].value).toBeNull();
});

test('wire timestamps require a complete ISO date and retain Go nanosecond precision',()=>{
 for(const at of ['', 'not-a-date', '2026-10-20']) expect(metricSchema.safeParse({...metric,at}).success).toBe(false);
 expect(metricSchema.safeParse({...metric,at:'2026-10-20T12:34:56.123456789Z'}).success).toBe(true);
 const frame={version:1,type:'snapshot',at:metric.at,host:'',platform:'windows',ready:true,warnings:[],components:[]};
 expect(snapshotSchema.safeParse({...frame,at:'invalid'}).success).toBe(false);
});
