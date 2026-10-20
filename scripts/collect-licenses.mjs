import {readFileSync,writeFileSync,readdirSync,existsSync} from 'node:fs';
import {join,dirname} from 'node:path';
import {execFileSync} from 'node:child_process';
let sections=[];const add=(name,dir)=>{for(const f of readdirSync(dir)){if(/^(licen[cs]e|copying|notice)([.-]|$)/i.test(f)){try{sections.push(`\n===== ${name} / ${f} =====\n${readFileSync(join(dir,f),'utf8')}`)}catch{}}}};
const lock=JSON.parse(readFileSync('package-lock.json','utf8'));for(const [path,pkg] of Object.entries(lock.packages)){if(path&&existsSync(path))add(`${pkg.name||path} ${pkg.version}`,path);}
const cargo=JSON.parse(execFileSync('.tools/cargo/bin/cargo.exe',['metadata','--manifest-path','src-tauri/Cargo.toml','--format-version','1','--locked'],{encoding:'utf8',maxBuffer:30*1024*1024}));let licenseRows=[];for(const p of cargo.packages){if(p.source){add(`${p.name} ${p.version}`,dirname(p.manifest_path));licenseRows.push(`${p.name}\t${p.version}\t${p.license||'see license file'}`);}}
const mods=execFileSync('go',['-C','collector','list','-m','-json','all'],{encoding:'utf8'});for(const part of mods.trim().split(/\r?\n}\r?\n(?={)/)){const m=JSON.parse(part.endsWith('}')?part:part+'}');if(m.Dir&&!m.Main)add(`${m.Path} ${m.Version}`,m.Dir);}
writeFileSync('docs/THIRD-PARTY-NOTICES.txt',sections.join('\n'));writeFileSync('docs/rust-dependency-licenses.tsv','Package\tVersion\tLicense\n'+licenseRows.join('\n'));console.log(`Collected ${sections.length} license/notice files; ${licenseRows.length} Rust dependencies`);
