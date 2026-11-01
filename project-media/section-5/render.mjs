import fs from 'node:fs';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const require = createRequire(import.meta.url);
const sharp = require('sharp');
const dir=path.dirname(fileURLToPath(import.meta.url));
const paper='#FAFAF7',ink='#292D32',muted='#62676B',blue='#487BDB',rule='#D7DAD7';
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;');
const text=(x,y,s,size=30,weight=400,color=ink,anchor='start')=>`<text x="${x}" y="${y}" font-size="${size}" font-weight="${weight}" fill="${color}" text-anchor="${anchor}">${esc(s)}</text>`;
const line=(x1,y1,x2,y2,color=rule,width=2,dash='')=>`<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${color}" stroke-width="${width}" ${dash?`stroke-dasharray="${dash}"`:''}/>`;
function page(title,subtitle,content,footer,metadata){return `<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080"><title>${esc(title)}</title><metadata>${esc(JSON.stringify(metadata))}</metadata><rect width="1920" height="1080" fill="${paper}"/><g font-family="Arial, Helvetica, sans-serif">${text(108,94,'OBSERVATORY / MEASUREMENTS',22,600,muted)}${text(108,186,title,62,700)}${text(108,245,subtitle,30,400,muted)}${content}${line(108,966,1812,966)}${text(108,1014,footer,25,400,muted)}</g></svg>`}
async function save(name,svg){fs.writeFileSync(path.join(dir,name+'.svg'),svg);await sharp(Buffer.from(svg)).png().toFile(path.join(dir,name+'.png'));}
const source='docs/project/rx-unified-tag14-result-20261029.md';
let body='';const x=455,w=1070,max=173;const ys=[390,520,650,780];
const min=x+w*156/max;
body+=text(min,316,'Minimum: 156',25,400,muted,'middle');
ys.forEach((y,i)=>{body+=text(108,y+13,`TX1 → RX${i+1}`,34,600);body+=`<rect x="${x}" y="${y-28}" width="${w}" height="54" fill="${blue}"/>`;body+=text(1570,y+13,'173 / 173',34,700);});
body+=line(min,337,min,825,paper,7)+line(min,337,min,825,ink,2,'9 8');
body+=line(x,840,x+w,840);body+=text(x,883,'0',25,400,muted,'middle')+text(x+w,883,'173',25,400,muted,'middle');body+=text(x+w/2,930,'Valid analysis windows per transmitter-to-receiver link',26,400,muted,'middle');
await save('01-four-rx-baseline',page('Four-receiver baseline','180-second recording · all four links passed',body,'29 Oct 2026 · Capture quality does not establish positioning accuracy.',{source,duration_seconds:180,links:['TX1→RX1','TX1→RX2','TX1→RX3','TX1→RX4'],qualified_windows:[173,173,173,173],denominator:173,required:156,threshold_source:'docs/project/tag14-mit-rx-unified-20261027.md'}));
body='';
const panels=[{x:108,w:780,title:'Qualified peer frames',max:2800,threshold:600,values:[2581,2578],labels:['2,581','2,578'],ticks:[0,1000,2000,2800]},{x:1032,w:780,title:'Valid analysis windows',max:293,threshold:264,values:[293,293],labels:['293 / 293','293 / 293'],ticks:[0,100,200,293]}];
for(const p of panels){body+=text(p.x,345,p.title,34,600);const bw=560;const px=p.x+12; const t=px+bw*p.threshold/p.max;body+=text(t,408,`Minimum: ${p.threshold}`,24,400,muted,'middle');[525,725].forEach((y,i)=>{body+=text(p.x,y-53,i===0?'RX4 → RX1':'RX1 → RX4',28,600);body+=`<rect x="${px}" y="${y-15}" width="${bw*p.values[i]/p.max}" height="52" fill="${blue}"/>`;body+=text(px+bw+25,y+23,p.labels[i],30,700);});for(const y of [525,725]) body+=line(t,y-31,t,y+53,paper,7)+line(t,y-31,t,y+53,ink,2,'9 8');body+=line(px,815,px+bw,815);for(const tick of p.ticks)body+=text(px+bw*tick/p.max,861,String(tick),24,400,muted,'middle');}
body+=text(108,929,'Both directions passed the frame-count and window-quality requirements.',29,600);
await save('02-peer-pair-test',page('Receiver-to-receiver test','300-second recording · RX1 ↔ RX4',body,'29 Oct 2026 · Positioning benefit and a four-receiver mesh are not yet established.',{source,duration_seconds:300,links:['RX4→RX1','RX1→RX4'],frames:[2581,2578],required_frames:600,qualified_windows:[293,293],denominator:293,required_windows:264}));
const rows=[['DAY 1','Check the radar reference','Test both sensors and align their space and time coordinates.'],['DAY 2','Collect walking data','Record an empty room, then calibrate by slowly walking through it.'],['DAYS 3–4','Compare models and test independently','Evaluate positioning models using separate recordings.'],['DAYS 5–7','Test live tracking and refine it','Check Wi-Fi-only tracking, improve it, and repeat independent tests.'],['DAYS 8–11','Test another room and complete final checks','Recalibrate, resolve remaining issues, validate and document the setup.'],['DAYS 12–13','Experiment with two people','Integrate and independently evaluate two-person tracking.']];
body='';rows.forEach(([days,title,detail],i)=>{const y=331+i*101;body+=text(108,y,days,26,700,blue);body+=text(418,y,title,32,600);body+=text(418,y+40,detail,25,400,muted);if(i<rows.length-1)body+=line(108,y+66,1812,y+66);});
let plan=page('What comes next','Project still in development · planned workdays, starting from Day 1',body,'Planned sequence, not fixed dates. Each stage depends on passing the previous checks.',{source:'docs/project/tracking-progress.html',mapping:'original October 15–27 plan → Day 1–13; video-only labels; original project plan unchanged',rows});
plan=plan.replace('OBSERVATORY / MEASUREMENTS','OBSERVATORY / NEXT STEPS');await save('03-next-steps',plan);
const narration='These measurements show that all four receivers can collect usable data, and the receiver-to-receiver test passed too. The project is still unfinished. Next, I’ll check the radar sensors, collect walking data, and test positioning models. After that come live tracking, a second room, and eventually two-person experiments.';
fs.writeFileSync(path.join(dir,'voiceover-en.txt'),narration+'\n');
console.log('Created three 1920 × 1080 PNGs, editable SVG sources and the shortened voiceover.');
