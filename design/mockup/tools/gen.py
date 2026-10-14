import numpy as np, sys
from PIL import Image

def sdf_rrect(W,H,R):
    y,x=np.mgrid[0:H,0:W]
    px=x+0.5-W/2; py=y+0.5-H/2
    qx=np.abs(px)-(W/2-R); qy=np.abs(py)-(H/2-R)
    outside=np.hypot(np.maximum(qx,0),np.maximum(qy,0))
    inside=np.minimum(np.maximum(qx,qy),0)
    return outside+inside-R   # negative inside

def make(name,W,H,R,B,sign=1.0):
    s=sdf_rrect(W,H,R)
    d=-s                      # distance inside (>=0)
    gy,gx=np.gradient(d)      # points toward interior
    n=np.hypot(gx,gy)+1e-6
    nx,ny=gx/n,gy/n          # inward normal
    t=np.clip(1-d/B,0,1)
    mag=t**2                  # strongest at rim
    vx=sign*nx*mag; vy=sign*ny*mag
    vx[d<0]=0; vy[d<0]=0
    r=((vx*0.5+0.5)*255).round().astype(np.uint8)
    g=((vy*0.5+0.5)*255).round().astype(np.uint8)
    b=np.full_like(r,128)
    Image.fromarray(np.dstack([r,g,b]),'RGB').save(f'{name}.png')
    # rim highlight overlay (RGBA): light from top-left, shade bottom-right
    ang=np.arctan2(-ny,-nx)   # outward normal angle
    light=np.cos(ang-np.deg2rad(-135))  # lit toward top-left
    rim=np.clip(1-d/3.0,0,1)*(d>=0)      # thin 3px rim
    glow=np.clip(1-d/B,0,1)**3*(d>=0)
    a1=rim*(0.35+0.65*np.clip(light,0,1))      # bright specular rim
    a2=rim*0.25*np.clip(-light,0,1)            # faint opposite rim
    alpha=np.clip(a1*0.95+a2*0.6+glow*0.10,0,1)
    rgba=np.dstack([np.full_like(alpha,255),np.full_like(alpha,255),np.full_like(alpha,255),(alpha*255).round()]).astype(np.uint8)
    Image.fromarray(rgba,'RGBA').save(f'{name}_rim.png')
    t2=np.clip(d/(B*0.6),0,1); sm=t2*t2*(3-2*t2)
    ma=(1-sm)*(d>=0)
    z=np.zeros_like(ma)
    Image.fromarray(np.dstack([z,z,z,(ma*255).round()]).astype(np.uint8),'RGBA').save(f'{name}_mask.png')

specs=[('rail',84,212,36,30),('gear',56,56,28,28),('bar',964,52,26,26),('insp',328,928,32,56),('sheet',760,620,36,40),('set',1308,640,32,56),('set_d',1308,470,32,56),('set_s',1308,560,32,56),('pv',420,56,28,28),('set_a',1308,760,32,56),('chk',328,540,32,56),('dl',1100,700,36,56)]
for n,W,H,R,B in specs: make(n,W,H,R,B,sign=1.0)
print('ok')
