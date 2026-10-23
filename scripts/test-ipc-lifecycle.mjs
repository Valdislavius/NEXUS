import {spawn} from 'node:child_process';
import {writeFileSync} from 'node:fs';
import {snapshotSchema} from '../src/model.ts';
const child=spawn('src-tauri/binaries/nexus-collector-x86_64-pc-windows-msvc.exe',[],{windowsHide:true,stdio:['pipe','pipe','pipe']});
const started=Date.now();let buffer='',ready=0,prior=0,gaps=[],stopped=false;
const timeout=setTimeout(()=>{child.kill();console.error('Timed out');process.exitCode=1},30000);
child.stdout.on('data',d=>{buffer+=d;let end;while((end=buffer.indexOf('\n'))>=0){const s=snapshotSchema.parse(JSON.parse(buffer.slice(0,end)));buffer=buffer.slice(end+1);if(!s.ready)continue;ready++;const now=Date.now();if(prior)gaps.push(now-prior);prior=now;
 if(ready===2)child.stdin.write(JSON.stringify({command:'interval',intervalMs:5000})+'\n');
 if(ready===5){stopped=true;child.stdin.write(JSON.stringify({command:'stop'})+'\n');}
}});
child.stderr.on('data',d=>process.stderr.write(d));
child.on('exit',code=>{clearTimeout(timeout);const ok=code===0&&stopped&&gaps.slice(-2).every(g=>g>=4500&&g<9000);const result={ok,exitCode:code,readySnapshots:ready,intervalsMs:gaps,elapsedMs:Date.now()-started,explicitStop:stopped};writeFileSync('artifacts/ipc-lifecycle.json',JSON.stringify(result,null,2));console.log(JSON.stringify(result));if(!ok)process.exitCode=1;});
