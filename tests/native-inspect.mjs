import { writeFile, mkdir } from 'node:fs/promises';
const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const target = targets.find(t => t.type === 'page' && /127\.0\.0\.1:1420|localhost:1420|tauri\.localhost/.test(t.url));
if (!target) throw Error('NEXUS WebView not found');
const ws = new WebSocket(target.webSocketDebuggerUrl); await new Promise((resolve, reject) => { ws.onopen=resolve; ws.onerror=reject; });
const pending = new Map(); let id=0;
ws.onmessage = e => { const response=JSON.parse(String(e.data)); if(response.id){const entry=pending.get(response.id); if(entry){pending.delete(response.id); clearTimeout(entry.timeout); entry.resolve(response);}}};
function command(method,params={}){return new Promise((resolve,reject)=>{const requestId=++id; const timeout=setTimeout(()=>reject(Error('timeout')),10000);pending.set(requestId,{resolve,timeout});ws.send(JSON.stringify({id:requestId,method,params}));});}
try {
 const result = await command('Runtime.evaluate',{expression:`(async()=>({title:document.title,url:location.href,heading:document.querySelector('main h1')?.textContent,appShell:!!document.querySelector('.desktop-shell'),body:document.body.innerText.slice(0,1400),tauri:typeof window.__TAURI_INTERNALS__,snapshot:await window.__TAURI_INTERNALS__?.invoke('current_snapshot').then(value=>({ready:value.snapshot?.ready,components:value.snapshot?.components?.length,error:value.error}),error=>({error:String(error)}))}))()`,awaitPromise:true,returnByValue:true});
 console.log(JSON.stringify(result,null,2));
 await mkdir('artifacts/native-ui',{recursive:true}); await writeFile('artifacts/native-ui/current-inspect.json',JSON.stringify(result,null,2));
 if(result.result?.result?.value?.appShell) {const screenshot=await command('Page.captureScreenshot',{format:'png',fromSurface:true}); if(screenshot.result?.data) {await writeFile('artifacts/native-ui/current-native.png',Buffer.from(screenshot.result.data,'base64')); console.log('Saved artifacts/native-ui/current-native.png');}}
} finally {ws.close();}
