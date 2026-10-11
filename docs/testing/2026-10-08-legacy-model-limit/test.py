from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
with (p/'test-actual.log').open('wb') as log:
    result=subprocess.run(['cargo','test','-p','coolzhu-web-console','--offline'],stdout=log,stderr=subprocess.STDOUT)
(p/'test-result.json').write_text(json.dumps({'exit_code':result.returncode,'command':'cargo test -p coolzhu-web-console --offline','log':'test-actual.log'})+'\n')
print((p/'test-actual.log').read_text(encoding='utf-8',errors='replace')[-2000:])
sys.exit(result.returncode)
