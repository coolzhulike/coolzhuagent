from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
with (p/'build-actual.log').open('wb') as log:
    result=subprocess.run(['cargo','build','-p','coolzhu-web-console','--offline'],stdout=log,stderr=subprocess.STDOUT)
(p/'build-result.json').write_text(json.dumps({'exit_code':result.returncode,'command':'cargo build -p coolzhu-web-console --offline','log':'build-actual.log'})+'\n')
print((p/'build-actual.log').read_text(encoding='utf-8',errors='replace')[-3500:])
sys.exit(result.returncode)
