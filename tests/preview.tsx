// Development-only visual QA. Captured local hardware readings, not shipped in dist.
import { createRoot } from 'react-dom/client';
import { useState } from 'react';
import { AppView } from '../src/App';
import { snapshotSchema, reconcileSnapshot } from '../src/model';
import captured from '../artifacts/hardware-snapshots.json';
import '../src/style.css';
const snapshot = reconcileSnapshot(null, snapshotSchema.parse(captured[captured.length-1]));
function Preview() { const [interval,setInterval]=useState(2000); return <AppView monitor={{snapshot,error:'',interval,changeInterval:async value=>setInterval(value),restart:async()=>{},restarting:false,stale:false}}/>; }
createRoot(document.getElementById('root')!).render(<Preview/>);
