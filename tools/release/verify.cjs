// Verify the actual downloaded update, using only the existing public updater key.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),dir=process.argv[2],version=process.argv[3];
assert(dir&&/^\d+\.\d+\.\d+$/.test(version),'Expected directory and version');
const config=JSON.parse(fs.readFileSync(path.join(root,'src-tauri/tauri.conf.json'),'utf8'));
const publicLines=Buffer.from(config.plugins.updater.pubkey,'base64').toString('utf8').trim().split(/\r?\n/);
const pub=Buffer.from(publicLines[1],'base64');assert.equal(pub.length,42);
const key=crypto.createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),pub.subarray(10)]),format:'der',type:'spki'});
const name=`Kouros_${version}_x64-setup.exe`,body=fs.readFileSync(path.join(dir,name)),encoded=fs.readFileSync(path.join(dir,name+'.sig'),'utf8').trim();
assert.equal(body.subarray(0,2).toString(),'MZ');
const lines=Buffer.from(encoded,'base64').toString('utf8').trim().split(/\r?\n/),packet=Buffer.from(lines[1],'base64');
assert.equal(packet.length,74);assert(packet.subarray(2,10).equals(pub.subarray(2,10)),'Updater signing key changed');
const algorithm=packet.subarray(0,2).toString();assert(['ED','Ed'].includes(algorithm));
const message=algorithm==='ED'?crypto.createHash('blake2b512').update(body).digest():body;
assert(crypto.verify(null,message,key,packet.subarray(10)),'Installer signature failed');
assert(lines[2].startsWith('trusted comment: '));
assert(crypto.verify(null,Buffer.concat([packet.subarray(10),Buffer.from(lines[2].slice('trusted comment: '.length))]),key,Buffer.from(lines[3],'base64')),'Signature metadata failed');
const latest=JSON.parse(fs.readFileSync(path.join(dir,'latest.json'),'utf8'));assert.equal(latest.version,version);
for(const platform of ['windows-x86_64','windows-x86_64-nsis']){assert.equal(latest.platforms[platform].signature.trim(),encoded);assert.equal(latest.platforms[platform].url,`https://github.com/Chartedkey/Kouros/releases/download/v${version}/${name}`);}
console.log(JSON.stringify({version,installer:name,bytes:body.length,sha256:crypto.createHash('sha256').update(body).digest('hex'),signature:'verified with existing updater key',manifest:'verified'}));
