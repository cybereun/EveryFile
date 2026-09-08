import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const root=resolve('docs/mockups');
const original=process.argv.includes('--original');
const profile=resolve(`.tmp/mockup-edge-${Date.now()}`);
await mkdir(profile,{recursive:true});
const browser=spawn('C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',[
  '--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check',
  '--disable-background-networking','--disable-extensions','--remote-debugging-port=0',
  `--user-data-dir=${profile}`,'about:blank'
],{windowsHide:true,stdio:'ignore',env:{...process.env,TEMP:profile,TMP:profile}});
let socket;
try{
  let port;
  for(let i=0;i<100;i++){
    try{port=(await readFile(resolve(profile,'DevToolsActivePort'),'utf8')).split('\n')[0];if(port)break;}catch{}
    await new Promise(r=>setTimeout(r,150));
  }
  if(!port)throw Error('Browser debugging port unavailable');
  const targets=await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  socket=new WebSocket(targets.find(t=>t.type==='page').webSocketDebuggerUrl);
  await new Promise((r,j)=>{socket.onopen=r;socket.onerror=j;});
  let id=0;const waiting=new Map();const errors=[];
  socket.onmessage=({data})=>{const msg=JSON.parse(data);if(msg.id){const p=waiting.get(msg.id);if(!p)return;waiting.delete(msg.id);clearTimeout(p.timer);msg.error?p.reject(Error(JSON.stringify(msg.error))):p.resolve(msg.result);}else if(msg.method==='Runtime.exceptionThrown')errors.push(msg.params.exceptionDetails.text);};
  const call=(method,params={})=>new Promise((resolve,reject)=>{const key=++id;const timer=setTimeout(()=>{waiting.delete(key);reject(Error(`CDP timeout: ${method}; socket state ${socket.readyState}`));},15000);waiting.set(key,{resolve,reject,timer});socket.send(JSON.stringify({id:key,method,params}));});
  const evaluate=async(expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  await call('Runtime.enable');await call('Page.enable');
  await call('Emulation.setDeviceMetricsOverride',{width:original?1600:1440,height:1000,deviceScaleFactor:1,mobile:false});
  await call('Page.navigate',{url:pathToFileURL(resolve(root,original?'everyfile-original-refresh.html':'everyfile-upgrade.html')).href});
  for(let i=0;i<60;i++){if(await evaluate(`document.readyState === "complete" && !!document.querySelector("${original?'#hits button':'#rows button'}")`))break;await new Promise(r=>setTimeout(r,100));}
  const capture=async(name)=>{const r=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await writeFile(resolve(root,name),Buffer.from(r.data,'base64'));};
  if(original){
    await capture('원본기반-리뉴얼.png');
    await evaluate('document.querySelector("#ask-file").click()');await capture('원본기반-AI질문.png');
    await evaluate('document.querySelector("#close-ai").click();document.querySelector("#dark").click()');await capture('원본기반-다크.png');
    const checks=await evaluate(`(()=>{
      const check=(value,message)=>{if(!value)throw Error(message)};
      const click=s=>document.querySelector(s).click();
      check(document.querySelectorAll('.hit').length===6,'initial search results');
      click('#filename-mode');check(document.querySelectorAll('.hit').length===1,'filename mode');click('#keyword-mode');
      document.querySelector('#extension').value='PDF';document.querySelector('#extension').dispatchEvent(new Event('change'));check(document.querySelectorAll('.hit').length===2,'extension filter');click('#reset-filters');
      click('[data-doc="1"]');check(document.querySelector('#document-title').textContent.endsWith('.txt'),'selection');click('[data-doc="3"]');
      click('#ask-file');check(document.querySelector('#ai-panel').classList.contains('open'),'AI expand');click('#close-ai');
      click('#next-page');check(document.querySelector('#page-number').textContent==='2','pagination');click('#prev-page');
      click('#text-tab');check(document.querySelector('#text-view').classList.contains('active'),'text view');click('#layout-tab');
      document.querySelector('#find-input').value='없는검색어';document.querySelector('#find-input').dispatchEvent(new Event('input'));check(document.querySelector('#find-count').textContent==='0 / 0','find missing');
      document.querySelector('#find-input').value='화학반응';document.querySelector('#find-input').dispatchEvent(new Event('input'));check(document.querySelector('#find-count').textContent==='1 / 3','find matches');
      click('#pause-btn');check(document.querySelector('#jobs-btn').textContent.includes('일시정지'),'pause');click('#pause-btn');
      click('#jobs-btn');check(document.querySelector('#dialog').open,'job dialog');click('#close-dialog');
      click('#more-filters');check(document.querySelector('#extra').classList.contains('open'),'advanced filters');click('#more-filters');
      click('#warm');return {checks:12,overflow:document.documentElement.scrollWidth>innerWidth};
    })()`);
    await call('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:false});
    await capture('원본기반-좁은화면.png');
    const mobileOverflow=await evaluate('document.documentElement.scrollWidth > innerWidth');
    if(errors.length||checks.overflow||mobileOverflow)throw Error(JSON.stringify({errors,checks,mobileOverflow}));
    console.log(JSON.stringify({result:'passed',variant:'original-refresh',...checks,mobileOverflow,runtimeErrors:errors.length,screenshots:4}));
  }else{
  await capture('추천-리디자인.png');
  await evaluate('document.querySelector("[data-theme=warm]").click()');await capture('기존색감-유지안.png');
  await evaluate('document.querySelector("[data-theme=dark]").click()');await capture('다크모드.png');
  const checks=await evaluate(`(()=>{
    const check=(value,message)=>{if(!value)throw Error(message)};
    check(document.querySelectorAll('#rows tr').length===8,'initial rows');
    const q=document.querySelector('#search');q.value='보고서';q.dispatchEvent(new Event('input'));check(document.querySelectorAll('#rows tr').length===2,'search');
    q.value='';q.dispatchEvent(new Event('input'));
    document.querySelector('#type').value='xlsx';document.querySelector('#type').dispatchEvent(new Event('change'));check(document.querySelectorAll('#rows tr').length===2,'filter');
    document.querySelector('#alltypes').click();document.querySelector('[data-file="3"]').click();check(document.querySelector('#detail-name').textContent.includes('브랜드'),'selection');
    document.querySelector('#pause').click();check(document.querySelector('#bottom-state').textContent==='수집 일시정지','pause');document.querySelector('#pause').click();
    document.querySelector('#jobs').click();check(document.querySelector('#modal').open,'dialog');document.querySelector('#modal').close();
    document.querySelector('[data-source=local]').click();check(document.querySelectorAll('#rows tr').length===2,'source');document.querySelector('[data-source=all]').click();
    return {checks:7,overflow:document.documentElement.scrollWidth>innerWidth};
  })()`);
  await evaluate('document.querySelector("[data-theme=modern]").click()');
  await call('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:false});
  await capture('모바일-레이아웃.png');
  const mobileOverflow=await evaluate('document.documentElement.scrollWidth > innerWidth');
  if(errors.length||checks.overflow||mobileOverflow)throw Error(JSON.stringify({errors,checks,mobileOverflow}));
  console.log(JSON.stringify({result:'passed',...checks,mobileOverflow,runtimeErrors:errors.length,screenshots:4}));
  }
  await call('Browser.close');
}finally{socket?.close();browser.kill();}
