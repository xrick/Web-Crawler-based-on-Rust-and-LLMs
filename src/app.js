// Browser requests JSON; textContent ensures fetched source text never becomes executable HTML.
const $ = id => document.getElementById(id);
const labels = {running:'執行中', completed:'完成', completed_with_errors:'完成（含缺漏）', failed:'失敗', cancelled:'已取消', interrupted:'已中斷'};
async function api(path, body) {
  const r = await fetch(path, body === undefined ? {} : {method:'POST', headers:{'Content-Type':'application/json','X-Crawler-Request':'1'}, body:JSON.stringify(body)});
  const data = await r.json(); if (!r.ok) throw new Error(data.error || `HTTP ${r.status}`); return data;
}
function message(text, error=false) { $('message').textContent=text; $('message').classList.toggle('error',error); }
function node(tag, text, className) { const el=document.createElement(tag); if(text!==undefined)el.textContent=text; if(className)el.className=className; return el; }
if ($('settings-form')) {
  $('settings-form').addEventListener('submit', async e => {
    e.preventDefault(); $('save').disabled=true;
    try { await api('/api/settings', {categories:[...document.querySelectorAll('[name=category]:checked')].map(c=>c.value), model:$('model').value, max_pages:Number($('max-pages').value)}); message('設定已儲存，將套用到下一次工作。'); }
    catch(e){message(e.message,true);} finally{$('save').disabled=false;}
  });
  (async()=>{try{
    const [settings, models]=await Promise.all([api('/api/settings'),api('/api/models')]);
    document.querySelectorAll('[name=category]').forEach(c=>c.checked=settings.categories.includes(c.value));
    $('max-pages').value=settings.max_pages; $('model').replaceChildren();
    models.models.forEach(m=>{const o=node('option',m);o.value=m;$('model').append(o);});
    $('model').value=settings.model;
    if(!$('model').value)message('原設定模型未安裝，請選擇已安裝模型。',true);
    $('save').disabled=false;
  }catch(e){message(`無法讀取設定或連接 Ollama：${e.message}`,true);}})();
}
if ($('start')) {
  let selected=null, active=null, busy=false, productSignature='';
  $('start').onclick=async()=>{ $('start').disabled=true;try{const j=await api('/api/jobs',{});selected=j.id;message('工作已啟動。');await refresh();}catch(e){message(e.message,true);$('start').disabled=false;} };
  $('cancel').onclick=async()=>{if(!active)return;$('cancel').disabled=true;try{await api(`/api/jobs/${active}/cancel`,{});message('正在取消，已完成資料會保留。');}catch(e){message(e.message,true);}};
  function products(job) {
    const signature=job.id+':'+job.products.length;
    if(signature===productSignature)return;productSignature=signature;
    const area=$('products');area.replaceChildren();
    if(!job.products.length){area.textContent='尚無已完成產品。';return;}
    for(const p of job.products){
      const detail=node('details'),summary=node('summary',`${p.name} · ${p.starting_price ? 'NT$'+p.starting_price.amount.toLocaleString()+' 起' : '售價待確認'} · ${p.specs.length} 個規格區塊`);detail.append(summary);
      const link=node('a','Apple 技術規格來源');link.href=p.specs_url;link.target='_blank';link.rel='noopener';detail.append(link);
      detail.append(node('p',`${p.category} / ${p.model} / LLM ${(p.llm_ms/1000).toFixed(1)} 秒`,'note'));
      if(p.starting_price){const a=node('a',`售價來源：${p.starting_price.source_name}`);a.href=p.starting_price.source_url;a.target='_blank';a.rel='noopener';detail.append(a);detail.append(node('p',p.starting_price.scope,'note'));}
      p.warnings.forEach(w=>detail.append(node('p',w,'note')));
      const groups=new Map(); for(const s of p.specs){const key=s.model||'型號未唯一對應（未分配）';if(!groups.has(key))groups.set(key,[]);groups.get(key).push(s);}
      for(const [model,specs] of groups){
        const variant=node('details');variant.append(node('summary',`${model} · ${specs.length} 區塊`));
        for(const s of specs){const row=node('details');row.append(node('summary',`${s.section} / ${s.label} · ${['source_matched','source_only'].includes(s.status)?'原文保留':s.status==='llm_failed'?'LLM 失敗，保留原文':'待確認'}`));row.append(node('pre',s.value));if(s.conditions)row.append(node('p','限制：'+s.conditions));variant.append(row);}detail.append(variant);
      }area.append(detail);
    }
  }
  async function refresh(){
    if(busy)return;busy=true;
    try{
      const list=await api('/api/jobs');active=list.active_id;$('start').disabled=!!active;$('cancel').disabled=!active;
      if(!selected)selected=active||list.jobs[0]?.id;
      $('history').replaceChildren();
      list.jobs.forEach(j=>{const row=node('tr');row.append(node('td',new Date(j.started_at).toLocaleString()),node('td',labels[j.status]||j.status),node('td',String(j.succeeded)));const cell=node('td'),b=node('button','查看','text-button');b.onclick=()=>{selected=j.id;refresh();};cell.append(b);row.append(cell);$('history').append(row);});
      if(!selected)return;const j=await api('/api/jobs/'+selected);
      $('status').textContent=labels[j.status]||j.status;['discovered','processed','succeeded','failed'].forEach(k=>$(k).textContent=j[k]);
      $('phase').textContent=j.phase;$('current-url').textContent=j.current_url;
      $('coverage').textContent=`已下載 ${j.pages.length} / ${j.settings.max_pages} 頁；${j.issues.length} 筆缺漏或錯誤。探索中的發現數仍可能增加。`;
      $('download').hidden=false;$('download').href=`/api/jobs/${j.id}/download`;
      products(j);$('issues').replaceChildren();if(!j.issues.length)$('issues').textContent='尚無記錄。';
      j.issues.forEach(i=>$('issues').append(node('p',`[${i.stage}] ${i.url} — ${i.message}`,'url')));
    }catch(e){message(`讀取失敗：${e.message}`,true);}finally{busy=false;}
  }
  refresh();setInterval(refresh,2000);
}
