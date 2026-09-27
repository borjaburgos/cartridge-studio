#!/usr/bin/env python3
"""Local browser controller for FCEUmm; serves only the selected ROM's video."""
import argparse
import io
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from PIL import Image
from playtest_famicom import Player, BUTTONS

PAGE = '''<!doctype html><meta charset="utf-8"><title>Cartridge readback</title>
<style>body{margin:40px auto;max-width:850px;background:#151921;color:#f4f4f4;font:18px system-ui;text-align:center}canvas{width:min(100%,768px);image-rendering:pixelated;background:black;border:1px solid #414856}p{color:#b9c3d4}button{font:inherit;padding:10px 20px;margin:8px;border-radius:8px;border:0;cursor:pointer}</style>
<h1>Famicom cartridge readback</h1><canvas id="screen" width="256" height="240"></canvas>
<p>Arrows: direction &middot; Z: A &middot; X: B &middot; Enter: Start &middot; Shift: Select</p>
<button id="pause">Pause</button><button id="reset">Restart</button>
<p id="status">Loading the exact verified readback. No ROM patches.</p>
<script>
const keys={ArrowUp:'up',ArrowDown:'down',ArrowLeft:'left',ArrowRight:'right',KeyZ:'a',KeyX:'b',Enter:'start',ShiftLeft:'select',ShiftRight:'select'},held=new Set();
let paused=false,restart=false;const canvas=document.querySelector('canvas'),ctx=canvas.getContext('2d');
addEventListener('keydown',e=>{if(keys[e.code]){held.add(keys[e.code]);e.preventDefault()}});
addEventListener('keyup',e=>{if(keys[e.code]){held.delete(keys[e.code]);e.preventDefault()}});
addEventListener('blur',()=>held.clear());
document.querySelector('#pause').onclick=()=>{paused=!paused;document.querySelector('#pause').textContent=paused?'Resume':'Pause'};
document.querySelector('#reset').onclick=()=>restart=true;
async function step(){const start=performance.now();try{if(!paused){const response=await fetch('/frame',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({buttons:[...held],restart})});restart=false;if(!response.ok)throw Error('Player stopped');const bitmap=await createImageBitmap(await response.blob());if(canvas.width!==bitmap.width||canvas.height!==bitmap.height){canvas.width=bitmap.width;canvas.height=bitmap.height}ctx.drawImage(bitmap,0,0);bitmap.close();document.querySelector('#status').textContent='Verified cartridge readback — interactive diagnostic ROM';}}catch(error){document.querySelector('#status').textContent=error.message;paused=true;}setTimeout(step,Math.max(1,50-(performance.now()-start)))}step();
</script>'''.encode('utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--port', type=int, default=8768)
    parser.add_argument('--core', type=Path, default=Path(__file__).resolve().parents[1]/'tmp/toolchains/fceumm/fceumm_libretro.so')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    player = Player(args.core, args.rom, args.output)
    player.run(120)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_GET(self):
            if self.path not in ('/', '/index.html'):
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(PAGE)))
            self.end_headers()
            self.wfile.write(PAGE)
        def do_POST(self):
            if self.path != '/frame':
                self.send_error(404)
                return
            size = int(self.headers.get('Content-Length', 0))
            if not 0 < size <= 1024:
                self.send_error(400)
                return
            try:
                request = json.loads(self.rfile.read(size))
                buttons = request.get('buttons', [])
                if not isinstance(buttons, list) or any(button not in BUTTONS for button in buttons):
                    raise ValueError('Unknown controller button')
                if request.get('restart'):
                    player.core.retro_reset()
                player.run(3, *buttons)
                output = io.BytesIO()
                Image.fromarray(player.video).save(output, format='PNG')
                data = output.getvalue()
            except (ValueError, TypeError):
                self.send_error(400)
                return
            self.send_response(200)
            self.send_header('Content-Type', 'image/png')
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
    server = HTTPServer(('127.0.0.1', args.port), Handler)
    print(f'Player: http://127.0.0.1:{args.port}/', flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        player.close()


if __name__ == '__main__':
    main()
