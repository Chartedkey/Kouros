"""Inspect a built/extracted Windows installer without executing it or reading user data."""
import argparse,hashlib,json,struct
from pathlib import Path

def pe(path):
 data=path.read_bytes();assert data[:2]==b'MZ',path
 start=struct.unpack_from('<I',data,0x3c)[0];assert data[start:start+4]==b'PE\0\0'
 machine,sections=struct.unpack_from('<HH',data,start+4);optional_size=struct.unpack_from('<H',data,start+20)[0];optional=start+24
 magic=struct.unpack_from('<H',data,optional)[0];directory=optional+(112 if magic==0x20b else 96)
 table=optional+optional_size;layout=[]
 for i in range(sections):
  pos=table+40*i;virtual_size,virtual,raw_size,raw=struct.unpack_from('<IIII',data,pos+8);layout.append((virtual,max(virtual_size,raw_size),raw))
 def offset(rva):
  for begin,size,raw in layout:
   if begin<=rva<begin+size:return raw+rva-begin
  raise AssertionError(('Unmapped RVA',rva,path))
 imports=[];import_rva=struct.unpack_from('<I',data,directory+8)[0]
 if import_rva:
  pos=offset(import_rva)
  while any(data[pos:pos+20]):
   name=offset(struct.unpack_from('<I',data,pos+12)[0]);imports.append(data[name:data.index(b'\0',name)].decode('ascii'));pos+=20
 resource_rva=struct.unpack_from('<I',data,directory+16)[0];version=None
 if resource_rva:
  base=offset(resource_rva)
  def leaves(relative):
   pos=base+relative;count=sum(struct.unpack_from('<HH',data,pos+12))
   for i in range(count):
    key,dest=struct.unpack_from('<II',data,pos+16+8*i)
    if dest&0x80000000:yield from leaves(dest&0x7fffffff)
    else:
     rva,size=struct.unpack_from('<II',data,base+dest);yield data[offset(rva):offset(rva)+size]
  # RT_VERSION is numeric resource type16.
  count=sum(struct.unpack_from('<HH',data,base+12))
  for i in range(count):
   key,dest=struct.unpack_from('<II',data,base+16+8*i)
   if key!=16:continue
   for blob in leaves(dest&0x7fffffff):
    sig=blob.find(b'\xbd\x04\xef\xfe')
    if sig>=0:
     ms,ls=struct.unpack_from('<II',blob,sig+8);version=[ms>>16,ms&65535,ls>>16,ls&65535]
 return {'machine':hex(machine),'version':version,'imports':imports}

def validate(a):
 expected=[int(x) for x in a.version.split('.')]+[0]
 native=pe(a.binary);installed=pe(a.payload/'kouros.exe');loader=pe(a.payload/'WebView2Loader.dll');installer=pe(a.installer)
 assert native['machine']==installed['machine']==loader['machine']=='0x8664'
 assert native['version']==installed['version']==installer['version']==expected,(native,installed,installer)
 original=a.binary.read_bytes()
 if a.frontend:
  frontend_assets=[str(p.relative_to(a.frontend)).replace('\\','/') for p in (a.frontend/'assets').iterdir() if p.suffix in ['.js','.css']]
  assert frontend_assets and all(name.encode() in original for name in frontend_assets),'Embedded frontend differs from current dist'
 payload=(a.payload/'kouros.exe').read_bytes();assert len(original)==len(payload)
 changed=[i for i,(x,y) in enumerate(zip(original,payload)) if x!=y]
 if changed:
  assert len(changed)==3 and changed==list(range(changed[0],changed[0]+3))
  at=changed[0];assert original[at:at+3]==b'UNK' and payload[at:at+3]==b'NSS'
 assert (a.payload/'WebView2Loader.dll').read_bytes()==a.loader.read_bytes()
 script=a.script.read_text(encoding='utf-8');assert '!define INSTALLMODE "currentUser"' in script
 assert '!define BUNDLEID "app.kouros.desktop"' in script
 assert f'!define VERSION "{a.version}"' in script
 # These are Windows system DLLs; the only bundled runtime DLL is WebView2Loader.
 allowed={'combase.dll','rpcrt4.dll','ktmw32.dll','advapi32.dll','bcrypt.dll','bcryptprimitives.dll','comctl32.dll','comdlg32.dll','d3d11.dll','d3dcompiler_47.dll','dcomp.dll','dwmapi.dll','dxgi.dll','gdi32.dll','imm32.dll','iphlpapi.dll','kernel32.dll','msvcrt.dll','ntdll.dll','ole32.dll','oleaut32.dll','opengl32.dll','propsys.dll','psapi.dll','secur32.dll','shell32.dll','shlwapi.dll','ucrtbase.dll','user32.dll','userenv.dll','uxtheme.dll','version.dll','webview2loader.dll','windowscodecs.dll','winhttp.dll','winmm.dll','winspool.drv','ws2_32.dll','wtsapi32.dll','crypt32.dll','setupapi.dll','wininet.dll','dbghelp.dll'}
 unknown=[x for x in native['imports'] if x.lower() not in allowed and not x.lower().startswith(('api-ms-win-','ext-ms-win-'))]
 assert not unknown,('Check/package additional imported DLLs',unknown)
 report={'version':a.version,'app_pe':native,'installer_pe':installer,'payload_pe':installed,'loader_pe':loader,'payload_matches_binary_except_nsis_marker':True,'marker_patch_bytes':len(changed),'loader_matches_crate':True,'current_frontend_embedded':bool(a.frontend),'install_mode':'currentUser','identifier':'app.kouros.desktop','windows_execution':'not_run_on_physical_windows','installer':{'file':a.installer.name,'bytes':a.installer.stat().st_size,'sha256':hashlib.sha256(a.installer.read_bytes()).hexdigest()}}
 a.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(json.dumps(report,ensure_ascii=False))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--version',default='1.7.0')
 p.add_argument('--frontend',type=Path,default=Path(__file__).resolve().parents[2]/'dist')
 for name in ['binary','payload','loader','installer','script','output']:p.add_argument('--'+name,type=Path,required=True)
 validate(p.parse_args())
