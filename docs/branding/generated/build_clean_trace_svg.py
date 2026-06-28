from __future__ import annotations
from pathlib import Path
from PIL import Image, ImageFilter
import subprocess, tempfile, textwrap

GEN=Path('docs/branding/generated')
TMP=GEN/'vector-src'
TMP.mkdir(parents=True, exist_ok=True)

def classify_icon(src: Path, out: Path, size=(1024,1024)):
    im=Image.open(src).convert('RGBA')
    data=[]
    for r,g,b,a in im.getdata():
        mx=max(r,g,b); mn=min(r,g,b); sat=mx-mn
        if g > r + 60 and g > b + 45 and g > 120:
            data.append((45,220,116,255))
        elif mx>70 and sat>22 and not (r<35 and g<48 and b<70):
            data.append((34,189,247,255))
        else:
            data.append((255,255,255,0))
    im.putdata(data)
    box=im.getchannel('A').getbbox(); im=im.crop(box)
    sc=min(size[0]/im.width,size[1]/im.height); nw=round(im.width*sc); nh=round(im.height*sc)
    im=im.resize((nw,nh),Image.Resampling.LANCZOS)
    canvas=Image.new('RGBA',size,(255,255,255,0)); canvas.alpha_composite(im,((size[0]-nw)//2,(size[1]-nh)//2))
    # Smooth tiny pixel noise but preserve bold shapes.
    a=canvas.getchannel('A').filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(0.7)).point(lambda v:255 if v>96 else 0)
    rgb=canvas.convert('RGB').filter(ImageFilter.GaussianBlur(0.4))
    clean=Image.new('RGBA',size,(255,255,255,0))
    outdata=[]
    for (r,g,b),aa in zip(rgb.getdata(),a.getdata()):
        if aa==0: outdata.append((255,255,255,0))
        elif g > r + 60 and g > b + 45 and g > 120: outdata.append((45,220,116,255))
        else: outdata.append((34,189,247,255))
    clean.putdata(outdata); clean.save(out)

def classify_logo(src: Path, out: Path, size=(1280,640)):
    im=Image.open(src).convert('RGBA')
    data=[]
    for r,g,b,a in im.getdata():
        mx=max(r,g,b); mn=min(r,g,b); sat=mx-mn
        if mx>145 and sat<100:
            data.append((246,250,255,255))
        elif g > r + 60 and g > b + 45 and g > 120:
            data.append((45,220,116,255))
        elif mx>70 and sat>22 and not (r<35 and g<50 and b<80):
            data.append((34,189,247,255))
        else:
            data.append((255,255,255,0))
    im.putdata(data)
    box=im.getchannel('A').getbbox(); im=im.crop(box)
    sc=min(size[0]/im.width,size[1]/im.height); nw=round(im.width*sc); nh=round(im.height*sc)
    im=im.resize((nw,nh),Image.Resampling.LANCZOS)
    canvas=Image.new('RGBA',size,(255,255,255,0)); canvas.alpha_composite(im,((size[0]-nw)//2,(size[1]-nh)//2))
    a=canvas.getchannel('A').filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(0.7)).point(lambda v:255 if v>96 else 0)
    rgb=canvas.convert('RGB').filter(ImageFilter.GaussianBlur(0.4))
    clean=Image.new('RGBA',size,(255,255,255,0))
    outdata=[]
    for (r,g,b),aa in zip(rgb.getdata(),a.getdata()):
        if aa==0: outdata.append((255,255,255,0))
        elif max(r,g,b)>145 and max(r,g,b)-min(r,g,b)<100: outdata.append((246,250,255,255))
        elif g > r + 60 and g > b + 45 and g > 120: outdata.append((45,220,116,255))
        else: outdata.append((34,189,247,255))
    clean.putdata(outdata); clean.save(out)

def trace_png(png: Path, svg: Path, label: str, colors: int):
    js_path_name='trace_clean.mjs'
    js=Path(js_path_name)
    js.write_text(textwrap.dedent(f'''
    import fs from 'node:fs';
    import ImageTracer from 'imagetracerjs';
    import {{ createCanvas, loadImage }} from 'canvas';
    const img = await loadImage({str(png.resolve())!r});
    const canvas = createCanvas(img.width, img.height);
    const ctx = canvas.getContext('2d'); ctx.drawImage(img, 0, 0);
    const data = ctx.getImageData(0, 0, img.width, img.height);
    const opts = {{
      ltres: 1.4, qtres: 1.4, pathomit: 18, rightangleenhance: false,
      colorsampling: 0, numberofcolors: {colors}, mincolorratio: 0.0001, colorquantcycles: 6,
      layering: 0, strokewidth: 0, linefilter: true, scale: 1, roundcoords: 1, viewbox: true
    }};
    let svg = ImageTracer.imagedataToSVG(data, opts);
    svg = svg.replace(/<svg[^>]*>/, `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${{img.width}} ${{img.height}}" role="img" aria-label="{label}">`);
    svg = svg.replace(/<rect[^>]*fill="rgb\\(255,255,255\\)"[^>]*><\\/rect>/g, '');
    svg = svg.replace(/<path[^>]*fill="rgb\\(255,255,255\\)"[^>]*><\\/path>/g, '');
    svg = svg.replace(/stroke="rgb\\([^)]*\\)" stroke-width="0"/g, '');
    fs.writeFileSync({str(svg.resolve())!r}, svg);
    '''))
    with tempfile.TemporaryDirectory() as td:
        subprocess.run(['npm','init','-y'],cwd=td,stdout=subprocess.DEVNULL,check=True)
        subprocess.run(['npm','install','imagetracerjs','canvas'],cwd=td,stdout=subprocess.DEVNULL,check=True)
        tmp_js=Path(td)/js_path_name
        tmp_js.write_text(js.read_text())
        subprocess.run(['node',js_path_name],cwd=td,check=True)

classify_icon(GEN/'dockrev-icon-imagegen-candidate.png', TMP/'dockrev-icon-clean-flat.png')
classify_logo(GEN/'dockrev-logo-imagegen-candidate.png', TMP/'dockrev-logo-clean-flat.png')
trace_png(TMP/'dockrev-icon-clean-flat.png', GEN/'dockrev-icon-clean-trace.svg', 'Dockrev icon clean trace', 8)
trace_png(TMP/'dockrev-logo-clean-flat.png', GEN/'dockrev-logo-clean-trace.svg', 'Dockrev logo clean trace', 4)
print('generated clean trace svgs')
