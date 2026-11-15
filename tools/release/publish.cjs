// Publish only after all three real release assets pass independent signature verification.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),cp=require('node:child_process'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),version=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8')).version,tag=`v${version}`,repo='Chartedkey/Kouros';
const gh=(...args)=>cp.execFileSync('gh',args,{encoding:'utf8'}).trim();
const expected=process.env.GITHUB_SHA;assert(expected&&/^[a-f0-9]{40}$/.test(expected),'Missing build commit');
const release=JSON.parse(gh('release','view',tag,'--repo',repo,'--json','tagName,isDraft,assets,targetCommitish'));
assert.equal(release.tagName,tag);assert.equal(release.targetCommitish,expected,'Release must point at this build commit');
const names=[`Kouros_${version}_x64-setup.exe`,`Kouros_${version}_x64-setup.exe.sig`,'latest.json'];
for(const name of names)assert(release.assets.some(a=>a.name===name&&a.size>100),`Missing release asset ${name}`);
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'kouros-release-'));
gh('release','download',tag,'--repo',repo,'--dir',dir,...names.flatMap(n=>['--pattern',n]));
cp.execFileSync(process.execPath,[path.join(__dirname,'verify.cjs'),dir,version],{stdio:'inherit'});
if(release.isDraft)gh('release','edit',tag,'--repo',repo,'--draft=false');
const ref=JSON.parse(gh('api',`repos/${repo}/git/ref/tags/${tag}`));assert.equal(ref.object.sha,expected,'Published tag must match checked source');
console.log(`Published ${tag} from ${expected}`);
