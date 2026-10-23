import { snapshotSchema } from '../src/model.ts';
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
const child=spawn('src-tauri/binaries/nexus-collector-x86_64-pc-windows-msvc.exe',[],{windowsHide:true,stdio:['pipe','pipe','pipe']});
let buffer='',samples=[];const timer=setTimeout(()=>{child.kill();console.error('Collector timeout');process.exitCode=1},45000);
child.stdout.on('data',data=>{buffer+=data;let end;while((end=buffer.indexOf('\n'))>=0){const value=snapshotSchema.parse(JSON.parse(buffer.slice(0,end)));buffer=buffer.slice(end+1);samples.push(value);if(value.ready){console.log(JSON.stringify({ready:value.ready,components:value.components.map(c=>({kind:c.kind,name:c.name,metrics:c.metrics.filter(m=>m.primary).map(m=>({id:m.id,value:m.value}))})),warnings:value.warnings}));}if(samples.filter(s=>s.ready).length>=3){writeFileSync('artifacts/hardware-snapshots.json',JSON.stringify(samples,null,2));child.stdin.end();}}});
child.stderr.on('data',data=>process.stderr.write(data));child.on('exit',code=>{clearTimeout(timer);console.log('Collector exited after stdin EOF:',code);if(code!==0||samples.filter(s=>s.ready).length<3)process.exitCode=1;});
