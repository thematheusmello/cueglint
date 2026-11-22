"""Independent measurement of the native authored short-reply fixture. Units are local to the window."""
import argparse, hashlib, io, json, math, zipfile
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw

EDGE_LIMITS=(366,74,661,114)
TOLERANCE=2.0

def measure(image, scale):
    assert image.mode=='RGBA' and scale>0
    a=np.asarray(image).astype(np.float64)
    assert abs(a.shape[1]/scale-690)<=2 and abs(a.shape[0]/scale-500)<=2
    # Premultiplied RGBA preserves opacity-only differences over translucent backgrounds.
    a[:,:,:3]*=a[:,:,3:4]/255
    def rect(x0,y0,x1,y1):
        return a[round(y0*scale):round(y1*scale),round(x0*scale):round(x1*scale)]
    bg=np.median(rect(245,80,295,110),axis=(0,1))
    inner=np.median(rect(440,79,590,84),axis=(0,1))
    delta=inner-bg
    assert np.linalg.norm(delta)>4, ('bubble fill indistinguishable',inner,bg)
    field=np.dot(a-bg,delta)/np.dot(delta,delta)
    # Median across the flat edges omits rounded corners and ink in the bubble interior.
    horizontal=np.median(field[round(90*scale):round(98*scale),:],axis=0)
    vertical=np.median(field[:,round(435*scale):round(592*scale)],axis=1)
    def span(profile,start,end,threshold):
        idx=np.flatnonzero(profile[round(start*scale):round(end*scale)]>=threshold)
        assert idx.size, ('empty band',start,end,threshold)
        cuts=np.flatnonzero(np.diff(idx)>1)+1
        longest=max(np.split(idx,cuts),key=len)
        return ((longest[0]+round(start*scale))/scale,(longest[-1]+1+round(start*scale))/scale)
    bands={}
    for threshold in (0.35,0.50,0.65):
        x0,x1=span(horizontal,240,677,threshold)
        y0,y1=span(vertical,60,121,threshold)
        bands[str(threshold)]=[x0,y0,x1,y1]
    # Read the bounds of the painted answer; the reference galley origin has a separate meaning.
    crop=rect(8,118,260,170)
    contrast=np.max(abs(crop-bg),axis=2)
    ink={}
    for limit in (52,76,100):
        yy,xx=np.where(contrast>limit)
        if not len(xx):continue
        ink[str(limit)]=[(xx.min()+round(8*scale))/scale,(yy.min()+round(118*scale))/scale,
                         (xx.max()+1+round(8*scale))/scale,(yy.max()+1+round(118*scale))/scale]
    assert len(ink)==3,'painted answer missing'
    return {'edges_logical':bands,'answer_ink_ltrb_logical':ink,
      'max_delta_half_threshold':float(max(abs(b-c) for b,c in zip(bands['0.5'],EDGE_LIMITS))),
      'passed_geometry':all(abs(b-c)<=TOLERANCE for values in bands.values() for b,c in zip(values,EDGE_LIMITS))}
def control(shift=0,scale=1):
    im=Image.new('RGBA',(690*scale,500*scale),(40,40,40,165))
    draw=ImageDraw.Draw(im)
    x,y,r,b=EDGE_LIMITS
    draw.rounded_rectangle([(int((x+shift)*scale),y*scale),(int((r+shift)*scale)-1,b*scale-1)],radius=17*scale,fill=(58,58,58,185))
    draw.rectangle([(20*scale,133*scale),(135*scale,145*scale)],fill='white')
    return measure(im,scale)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('archive',nargs='?');p.add_argument('--sha256');args=p.parse_args()
    for scale in (1,2):
        assert control(scale=scale)['max_delta_half_threshold']==0
        assert control(2,scale)['passed_geometry'] and not control(3,scale)['passed_geometry']
    if args.archive:
        raw=Path(args.archive).read_bytes()
        assert args.sha256 and hashlib.sha256(raw).hexdigest()==args.sha256
        with zipfile.ZipFile(io.BytesIO(raw)) as z:
            report=json.loads(z.read('assistant-short-v1/report.json'))
            assert report['passed'] and report['source_binary_endpoints_match']
            assert len(report['runs'])==1
            run,=report['runs']; receipt=run['receipt']
            assert run['fixture']==receipt['fixture']=='assistant-short-v1' and run['passed']
            assert receipt['zoom_factor']==1
            png=z.read('assistant-short-v1/assistant-short-v1.png')
            assert hashlib.sha256(png).hexdigest()==run['png_sha256']==receipt['verified_png_sha256']
            out=measure(Image.open(io.BytesIO(png)),receipt['pixels_per_point'])
            out.update({'png_sha256':run['png_sha256'],'archive_sha256':args.sha256,'source_commit':report['source_commit'],'image':receipt['image'],'pixels_per_point':receipt['pixels_per_point'],'adapter':receipt['adapter'],'binary_sha256':report['binary_sha256'],'fixture_data_sha256':report['fixture_data_sha256'],'os_version':report['os_version'],'os_build':report['os_build'],'font_set_id':receipt['fonts']['identity']['font_set_id']})
            print(json.dumps(out,indent=2))
    else:print('Synthetic 1x/2x zero, +2 and +3 logical-pixel controls passed.')
