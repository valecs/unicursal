# First AI-silhouette probe: prompts two Gemini image models for stencil
# silhouettes. Auth comes from the environment (a proxy-injected
# x-goog-api-key header); no key is read here. Usage: python3 ai_silhouettes.py OUTDIR
import json, base64, sys, urllib.request, os
from concurrent.futures import ThreadPoolExecutor
D = sys.argv[1] if len(sys.argv) > 1 else "ai_out"; os.makedirs(D, exist_ok=True)
PROMPT=("A single bold, solid black silhouette of {s}, on a plain pure-white background. "
 "Flat 2-colour stencil: pure black and pure white only, no grey, no shading, no outlines, no texture, no ground, no shadow, no text. "
 "The shape must be one connected piece. Show key features (such as eyes) as large, simple white cut-outs inside the black shape. "
 "Avoid thin lines and fine detail: every part should be chunky. Centred, filling about half the image, with a wide white margin on all sides.")
SUBJ=['a cat sitting, seen from the side','a jack-o-lantern pumpkin','an owl','a rocket ship']
MODELS=['gemini-2.5-flash-image','gemini-3.1-flash-image']
def run(job):
    m,i,s=job
    body={"contents":[{"parts":[{"text":PROMPT.format(s=s)}]}],
          "generationConfig":{"responseModalities":["IMAGE"],"imageConfig":{"aspectRatio":"3:4"}}}
    req=urllib.request.Request(f"https://generativelanguage.googleapis.com/v1beta/models/{m}:generateContent",
        data=json.dumps(body).encode(),headers={"Content-Type":"application/json"})
    try:
        r=json.load(urllib.request.urlopen(req,timeout=180))
    except urllib.error.HTTPError as e:
        return m,s,'HTTP %s %s'%(e.code,e.read()[:300])
    for c in r.get('candidates',[]):
        for p in c.get('content',{}).get('parts',[]):
            if 'inlineData' in p:
                fn=f"{D}/{m}_{i}.png"; open(fn,'wb').write(base64.b64decode(p['inlineData']['data'])); return m,s,fn
    return m,s,'no image: '+json.dumps(r)[:300]
jobs=[(m,i,s) for m in MODELS for i,s in enumerate(SUBJ)]
with ThreadPoolExecutor(8) as ex:
    for res in ex.map(run,jobs): print(res)
