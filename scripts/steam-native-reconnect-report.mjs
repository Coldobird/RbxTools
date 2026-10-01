import fs from 'node:fs';
import path from 'node:path';
const [directory,steamRoot='C:/Program Files (x86)/Steam'] = process.argv.slice(2);
if(!directory)throw Error('Diagnostic directory required');
const readLines=file=>fs.existsSync(file)?fs.readFileSync(file,'utf8').trim().split(/\r?\n/).filter(Boolean).flatMap(line=>{try{return [JSON.parse(line)]}catch{return []}}):[];
const events=fs.readdirSync(directory).filter(n=>n.endsWith('-events.jsonl')).flatMap(n=>readLines(path.join(directory,n)));
const actions=readLines(path.join(directory,'actions.jsonl'));
const presence=readLines(path.join(directory,'presence.jsonl'));
const followups=readLines(path.join(directory,'followup-actions.jsonl')).map(action=>{
 const observerName=action.candidate==='friends-reconnect'?'friends-followup-observer.jsonl':'connect-followup-observer.jsonl';
 const samples=readLines(path.join(directory,observerName)).filter(row=>row.event==='sample');
 const first=samples[0];
 const online=samples.find(row=>row.details.online);
 return {...action,initiallyOnline:first?.details.online,firstOnlineUtcMs:online?.utcMs,invocationToOnlineMs:online && first?.details.online===false?online.utcMs-action.invokedUtcMs:null,causality:first?.details.online?'already online; no recovery attributed':'uncontrolled follow-up; recovery cause unproven'};
});
const ids=[...new Set(events.filter(e=>e.event==='trial_start').map(e=>e.id))];
const trials=ids.map(id=>{
 const rows=events.filter(e=>e.id===id);
 const start=rows.find(e=>e.event==='trial_start');
 const restore=rows.find(e=>e.event==='restored');
 const online=rows.find(e=>['online','natural_recovery_before_candidate'].includes(e.event));
 const action=actions.find(a=>a.id===id);
 const invocation=start.details.candidate!=='control'?action?.invokedUtcMs:undefined;
 const end=rows.find(e=>e.event==='trial_complete');
 return {id,candidate:start.details.candidate,blockSeconds:start.details.blockSeconds,status:end?'complete':rows.some(e=>e.event==='error')?'error':restore?'observing':'blocked',restoredUtcMs:restore?.utcMs,invokedUtcMs:invocation,actionResult:action?.result,restorationToOnlineMs:online&&restore?online.utcMs-restore.utcMs:null,invocationToOnlineMs:online&&invocation?online.utcMs-invocation:null,online:end?.details.online,filtersClosed:end?.details.filtersClosed,lastEvent:rows.at(-1)};
});
const offsets=JSON.parse(fs.readFileSync(path.join(directory,'log-offsets.json'),'utf8').replace(/^\uFEFF/,''));
const relevant={};
for(const name of ['connection_log.txt','gameprocess_log.txt','console_log.txt']) {
 const bytes=fs.readFileSync(path.join(steamRoot,'logs',name));
 const text=bytes.subarray(bytes.length>=offsets[name]?offsets[name]:0).toString('utf8');
 const pattern=name==='connection_log.txt'?/Reconnect|Connect\(\)|ConnectionDisconnected|RecvMsgClientLogOnResponse|Connectivity test|QoS/:name==='gameprocess_log.txt'?/./:/Reconnect|GoOnline|offline|Assert|Error/i;
 relevant[name]=text.split(/\r?\n/).filter(line=>pattern.test(line)&&!/Using JWT|password|token|secret|nonce/i.test(line)).map(line=>line.replace(/\[U:[^\]]+\]/g,'[steam-user]').replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g,'[ip]').replace(/\b[0-9a-f]{1,4}(?::[0-9a-f]{0,4}){2,}\b/gi,match=>match.includes('::') || match.split(':').length===8 ? '[ipv6]' : match));
}
const result={generatedUtc:new Date().toISOString(),trials,followups,localGameIds:[...new Set(presence.map(row=>row.inspection?.gameId).filter(Boolean))],lastPresence:presence.at(-1),gameLogEntries:relevant['gameprocess_log.txt'].length,spacewarGameRegistration:relevant['gameprocess_log.txt'].some(line=>/(?:appid|gameid)\D*480\b/i.test(line)),logs:relevant};
fs.writeFileSync(path.join(directory,'results.json'),JSON.stringify(result,null,2));
console.log(JSON.stringify({trials:trials.map(({lastEvent,...trial})=>({...trial,lastEvent:lastEvent.event})),gameLogEntries:result.gameLogEntries,spacewarGameRegistration:result.spacewarGameRegistration},null,2));
