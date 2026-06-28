
import fs from 'node:fs';
import ImageTracer from 'imagetracerjs';
import { createCanvas, loadImage } from 'canvas';
const img = await loadImage('docs/branding/generated/vector-src/dockrev-icon-clean-flat.png');
const canvas = createCanvas(img.width, img.height);
const ctx = canvas.getContext('2d'); ctx.drawImage(img, 0, 0);
const data = ctx.getImageData(0, 0, img.width, img.height);
const opts = {
  ltres: 1.4, qtres: 1.4, pathomit: 18, rightangleenhance: false,
  colorsampling: 0, numberofcolors: 3, mincolorratio: 0.002, colorquantcycles: 3,
  layering: 0, strokewidth: 0, linefilter: true, scale: 1, roundcoords: 1, viewbox: true
};
let svg = ImageTracer.imagedataToSVG(data, opts);
svg = svg.replace(/<svg[^>]*>/, `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${img.width} ${img.height}" role="img" aria-label="Dockrev icon clean trace">`);
svg = svg.replace(/<rect[^>]*fill="rgb\(255,255,255\)"[^>]*><\/rect>/g, '');
svg = svg.replace(/<path[^>]*fill="rgb\(255,255,255\)"[^>]*><\/path>/g, '');
svg = svg.replace(/stroke="rgb\([^)]*\)" stroke-width="0"/g, '');
fs.writeFileSync('docs/branding/generated/dockrev-icon-clean-trace.svg', svg);
