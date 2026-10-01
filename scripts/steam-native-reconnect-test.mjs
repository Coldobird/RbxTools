// Diagnostic-only access to Steam's local CEF debugger. No game API initialization.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const expressions = {
  inspect: `JSON.stringify({origin:location.origin,reconnect:typeof SteamClient?.User?.Reconnect,connect:typeof SteamClient?.User?.Connect,goOnline:typeof SteamClient?.User?.GoOnline,friendsReconnect:typeof window.g_FriendsUIApp?.Reconnect,friendsReady:window.g_FriendsUIApp?.ready_to_render,friendsConnected:window.g_FriendsUIApp?.CMInterface?.m_bConnected,friendsLoggedOn:window.g_FriendsUIApp?.CMInterface?.m_bLoggedOn,friendsConnectionFailed:window.g_FriendsUIApp?.CMInterface?.m_bConnectionFailed,gameId:String(window.g_FriendsUIApp?.FriendStore?.self?.persona?.m_gameid??'unknown')})`,
  'inspect-friends': `JSON.stringify({cmFields:Object.keys(window.g_FriendsUIApp?.CMInterface??{}),transportFields:Object.keys(window.g_FriendsUIApp?.m_CMInterface??{})})`,
  'native-reconnect': `(()=>{if(typeof SteamClient?.User?.Reconnect!=='function')throw Error('Reconnect unavailable');SteamClient.User.Reconnect();return 'invoked'})()`,
  'native-connect': `(()=>{if(typeof SteamClient?.User?.Connect!=='function')throw Error('Connect unavailable');SteamClient.User.Connect();return 'invoked'})()`,
  'friends-reconnect': `(()=>{if(typeof window.g_FriendsUIApp?.Reconnect!=='function')throw Error('Friends reconnect unavailable');window.g_FriendsUIApp.Reconnect();return 'invoked'})()`,
};
const delay = ms => new Promise(resolve => setTimeout(resolve,ms));

async function evaluate(expression) {
  const targets = await fetch('http://127.0.0.1:8080/json/list',{signal:AbortSignal.timeout(3000)}).then(r=>r.json());
  const candidates = targets.filter(t=>t.type==='page' && t.title==='SharedJSContext' && new URL(t.url).hostname==='steamloopback.host');
  if(candidates.length!==1)throw Error('Expected exactly one Steam SharedJSContext');
  const ws = new WebSocket(candidates[0].webSocketDebuggerUrl);
  return new Promise((resolve,reject)=>{
    const timer = setTimeout(()=>{ws.close();reject(Error('Steam evaluation timeout'));},6000);
    ws.addEventListener('open',()=>ws.send(JSON.stringify({id:1,method:'Runtime.evaluate',params:{expression,returnByValue:true}})));
    ws.addEventListener('message',event=>{
      const result=JSON.parse(event.data); if(result.id!==1)return;
      clearTimeout(timer); ws.close();
      if(result.error || result.result?.exceptionDetails)reject(Error(result.error?.message || result.result.exceptionDetails.text));
      else resolve(result.result?.result?.value ?? result.result?.result?.type);
    });
    ws.addEventListener('error',()=>{clearTimeout(timer);reject(Error('Steam debugger connection failed'));});
  });
}

const [mode,controlDir,steamPath,expectedPid,logDir,eventFileArg] = process.argv.slice(2);
if(mode==='inspect' || mode==='inspect-friends') {
  console.log(await evaluate(expressions[mode]));
} else if(mode==='invoke') {
  const candidate=controlDir, steamPid=steamPath, outputFile=expectedPid, observerFile=logDir;
  if(!['native-reconnect','native-connect','friends-reconnect'].includes(candidate) || !outputFile || !observerFile)throw Error('An observed native candidate, Steam PID, evidence file, and live observer file are required');
  const actualPid=execFileSync('powershell.exe',['-NoProfile','-Command',"(Get-Process -Name steam -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id) -join ','"],{encoding:'utf8',windowsHide:true}).trim();
  if(actualPid!==steamPid)throw Error('Steam process changed; refusing diagnostic invocation');
  const before=JSON.parse(await evaluate(expressions.inspect));
  const nativeSamples=fs.readFileSync(observerFile,'utf8').trim().split(/\r?\n/).flatMap(line=>{try{return [JSON.parse(line)]}catch{return []}});
  const nativeSample=nativeSamples.filter(row=>row.event==='sample').at(-1);
  if(!nativeSample || Date.now()-nativeSample.utcMs>3000)throw Error('A current native login observation is required');
  const recovered=candidate==='friends-reconnect'?before.friendsConnected && before.friendsLoggedOn:nativeSample.details.online;
  if(recovered) {
    const record={event:'exploratory_skipped',candidate,expectedPid:Number(steamPid),finishedUtcMs:Date.now(),result:'already connected',before};
    fs.appendFileSync(outputFile,JSON.stringify(record)+'\n');console.log(JSON.stringify(record));
  } else {
  const invokedUtcMs=Date.now();
  const result=await evaluate(expressions[candidate]);
  const after=JSON.parse(await evaluate(expressions.inspect));
  const record={event:'exploratory_invocation',candidate,expectedPid:Number(steamPid),invokedUtcMs,finishedUtcMs:Date.now(),result,before,after};
  fs.appendFileSync(outputFile,JSON.stringify(record)+'\n');
  console.log(JSON.stringify(record));
  }
} else if(mode==='coordinate') {
  const handled=new Set();
  const actions=path.join(logDir,'actions.jsonl');
  const eventFile=eventFileArg || path.join(logDir,'screening-events.jsonl');
  const record=value=>fs.appendFileSync(actions,JSON.stringify(value)+'\n');
  record({event:'coordinator_start',utcMs:Date.now(),expectedPid:Number(expectedPid)});
  while(!fs.existsSync(path.join(controlDir,'stop')) && !fs.existsSync(path.join(controlDir,'done'))) {
    try {
      const ready=JSON.parse(fs.readFileSync(path.join(controlDir,'ready.json'),'utf8'));
      if(!handled.has(ready.id)) {
        handled.add(ready.id);
        const actualPid=execFileSync('powershell.exe',['-NoProfile','-Command',"(Get-Process -Name steam -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id) -join ','"],{encoding:'utf8',windowsHide:true}).trim();
        if(actualPid!==expectedPid)throw Error('Steam process changed; cancelling suite');
        const events=fs.readFileSync(eventFile,'utf8').trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line));
        if(ready.candidate!=='control' && events.some(e=>e.id===ready.id && ['online','natural_recovery_before_candidate'].includes(e.event))) {
          const ack={id:ready.id,candidate:ready.candidate,finishedUtcMs:Date.now(),result:'skipped; Steam already recovered'};
          record({event:'candidate_skipped',...ack});fs.writeFileSync(path.join(controlDir,'ack.json'),JSON.stringify(ack));continue;
        }
        const invokedUtcMs=Date.now();
        let result;
        if(ready.candidate==='control')result='no action';
        else if(ready.candidate==='goonline') {
          execFileSync(steamPath,['steam://open/goonline'],{windowsHide:true,timeout:6000});result='Go Online URI dispatched';
        } else {
          if(!expressions[ready.candidate])throw Error('Unsupported native candidate');
          result=await evaluate(expressions[ready.candidate]);
        }
        const ack={id:ready.id,candidate:ready.candidate,invokedUtcMs,finishedUtcMs:Date.now(),result};
        record({event:'candidate_invoked',...ack});
        fs.writeFileSync(path.join(controlDir,'ack.json'),JSON.stringify(ack));
      }
    } catch(error) {
      if(error.code!=='ENOENT' && !(error instanceof SyntaxError)) {
        record({event:'error',utcMs:Date.now(),message:error.message});
        fs.writeFileSync(path.join(controlDir,'stop'),error.message);throw error;
      }
    }
    await delay(150);
  }
  record({event:'coordinator_complete',utcMs:Date.now()});
} else throw Error('Usage: inspect OR invoke <candidate> <pid> <evidence-file> <observer-file> OR coordinate <control-dir> <steam.exe> <pid> <log-dir>');
