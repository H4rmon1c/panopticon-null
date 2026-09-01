Globe.prototype.draw = function(now) {
    const c=this.ctx,w=this.width,h=this.height;if(!w||!h)return;c.clearRect(0,0,w,h);const p=this.palette();
    this.stars.forEach((star,i)=>{c.fillStyle=alpha(p.star,(.14+(Math.sin(now*.001+i)+1)*.06)*star.a);c.fillRect(star.x*w,star.y*h,star.s,star.s);});
    const header=52,timeline=parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--timeline"))||84,available=Math.max(320,h-header-timeline);
    const cx=w*(w<900?.60:.56),cy=header+available*.51,r=Math.min(available*.58,w*.43)*this.zoom;this.cx=cx;this.cy=cy;this.r=r;this.camera=basis(this.lon,this.lat);
    const glow=c.createRadialGradient(cx,cy,r*.78,cx,cy,r*1.18);glow.addColorStop(0,"transparent");glow.addColorStop(.78,alpha(p.cyan,.1));glow.addColorStop(.93,alpha(p.cyan,.23));glow.addColorStop(1,"transparent");c.fillStyle=glow;c.beginPath();c.arc(cx,cy,r*1.2,0,Math.PI*2);c.fill();
    c.save();c.beginPath();c.arc(cx,cy,r,0,Math.PI*2);c.clip();
    const ocean=c.createRadialGradient(cx-r*.27,cy-r*.32,r*.05,cx,cy,r);ocean.addColorStop(0,p.oceanLight);ocean.addColorStop(.5,p.ocean);ocean.addColorStop(1,p.oceanDark);c.fillStyle=ocean;c.fillRect(cx-r,cy-r,r*2,r*2);
    c.strokeStyle=alpha(p.grid,.18);c.lineWidth=.65;for(let lat=-75;lat<=75;lat+=15)this.geoLine(Array.from({length:121},(_,i)=>[-180+i*3,lat]),c,r);for(let lon=-180;lon<180;lon+=15)this.geoLine(Array.from({length:90},(_,i)=>[lon,-89+i*2]),c,r);
    WORLD.forEach((shape,i)=>{c.beginPath();let drawing=false,visible=0;shape.forEach(([lon,lat])=>{const q=this.project(lat,lon);if(q.z<-.01){drawing=false;return;}visible++;if(!drawing){c.moveTo(q.x,q.y);drawing=true;}else c.lineTo(q.x,q.y);});if(visible>2){c.closePath();c.fillStyle=p.land[i%p.land.length];c.fill();c.strokeStyle=alpha(p.border,.42);c.lineWidth=.7;c.stroke();}});
    this.drawScan(now,p);this.drawLinks(now,p);this.drawMarkers(now,p);
    const shade=c.createRadialGradient(cx-r*.4,cy-r*.42,r*.05,cx+r*.36,cy+r*.35,r*1.08);shade.addColorStop(0,"rgba(255,255,255,.035)");shade.addColorStop(.48,"rgba(0,0,0,.02)");shade.addColorStop(1,"rgba(0,0,0,.72)");c.fillStyle=shade;c.fillRect(cx-r,cy-r,r*2,r*2);c.restore();
    c.save();c.strokeStyle=alpha(p.cyan,.46);c.lineWidth=1.2;c.shadowColor=p.cyan;c.shadowBlur=12;c.beginPath();c.arc(cx,cy,r,0,Math.PI*2);c.stroke();c.strokeStyle=alpha(p.cyan,.13);c.lineWidth=6;c.beginPath();c.arc(cx,cy,r+4,0,Math.PI*2);c.stroke();c.restore();
  };
Globe.prototype.geoLine = function(points,c,r) {let drawing=false;c.beginPath();points.forEach(([lon,lat])=>{const q=this.project(lat,lon);if(q.z<=.01){drawing=false;return;}if(!drawing){c.moveTo(q.x,q.y);drawing=true;}else c.lineTo(q.x,q.y);});c.stroke();};
Globe.prototype.drawScan = function(now,p) {const c=this.ctx,angle=now*.00014%(Math.PI*2),g=c.createRadialGradient(this.cx,this.cy,0,this.cx,this.cy,this.r);g.addColorStop(0,alpha(p.cyan,.14));g.addColorStop(1,"transparent");c.save();c.translate(this.cx,this.cy);c.rotate(angle);c.beginPath();c.moveTo(0,0);c.arc(0,0,this.r,-.065,.065);c.closePath();c.fillStyle=g;c.fill();c.beginPath();c.moveTo(0,0);c.lineTo(this.r,0);c.strokeStyle=alpha(p.cyan,.25);c.stroke();c.restore();};
