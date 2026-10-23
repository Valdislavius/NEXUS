// Offline source preparation for the authenticated GitHub Git Data connector.
// Does not obtain credentials or perform network operations. Output is ignored.
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
const git = args => execFileSync('git', args, {maxBuffer: 64 * 1024 * 1024});
const text = args => git(args).toString('utf8').trim();
// Pin a verified commit before enumerating objects. Using the resolved SHA in
// later commands also keeps a caller-supplied ref from being parsed as options.
const baseRef = process.argv[2] ?? 'origin/main';
const base = text(['rev-parse','--verify','--end-of-options',`${baseRef}^{commit}`]);
const output = path.resolve('artifacts/git-publication');
fs.mkdirSync(output,{recursive:true});
const existing = new Set(text(['rev-list','--objects',base]).split('\n').map(line=>line.split(' ')[0]));
const shas = text(['rev-list','--reverse',`${base}..HEAD`]).split('\n').filter(Boolean);
const objects = new Map();
const commits = shas.map(sha=>{
  const elements = git(['ls-tree','-r','-z',sha]).toString('utf8').split('\0').filter(Boolean).map(line=>{
    const [metadata,name] = line.split('\t');
    const [mode,type,blob] = metadata.split(' ');
    if(type !== 'blob') throw new Error('Unexpected source type');
    if(!existing.has(blob) && !objects.has(blob)) objects.set(blob, {sha:blob,content:git(['cat-file','blob',blob]).toString('base64')});
    return {path:name,mode,type,sha:blob};
  });
  return {localSha:sha,tree:text(['show','-s','--format=%T',sha]),message:text(['show','-s','--format=%B',sha]),elements};
});
const batches=[];let batch=[],bytes=0;
for(const object of objects.values()) {
  if(batch.length && (bytes+object.content.length > 1500000 || batch.length>=16)){batches.push(batch);batch=[];bytes=0;}
  batch.push(object);bytes+=object.content.length;
}
if(batch.length)batches.push(batch);
batches.forEach((items,i)=>fs.writeFileSync(path.join(output,`batch-${i}.json`),JSON.stringify(items)));
fs.writeFileSync(path.join(output,'manifest.json'),JSON.stringify({baseRef,base,localHead:text(['rev-parse','HEAD']),commits,batches:batches.length,blobs:objects.size},null,2));
console.log(JSON.stringify({commits:commits.length,blobs:objects.size,batches:batches.length,baseRef,base,localHead:text(['rev-parse','HEAD'])}));
