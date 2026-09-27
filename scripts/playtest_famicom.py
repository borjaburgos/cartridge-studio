#!/usr/bin/env python3
"""Run an unmodified .nes in a locally built FCEUmm libretro core, without USB.

Requires numpy/Pillow and a compiled libretro core. Captures video, audio activity,
read-only RAM snapshots, and normal controller input; never patches ROM or RAM.
"""
import argparse
import ctypes as C
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
from PIL import Image


class GameInfo(C.Structure):
    _fields_ = [('path', C.c_char_p), ('data', C.c_void_p), ('size', C.c_size_t), ('meta', C.c_char_p)]


class Variable(C.Structure):
    _fields_ = [('key', C.c_char_p), ('value', C.c_char_p)]


ENV = C.CFUNCTYPE(C.c_bool, C.c_uint, C.c_void_p)
VIDEO = C.CFUNCTYPE(None, C.c_void_p, C.c_uint, C.c_uint, C.c_size_t)
SAMPLE = C.CFUNCTYPE(None, C.c_int16, C.c_int16)
BATCH = C.CFUNCTYPE(C.c_size_t, C.POINTER(C.c_int16), C.c_size_t)
POLL = C.CFUNCTYPE(None)
INPUT = C.CFUNCTYPE(C.c_int16, C.c_uint, C.c_uint, C.c_uint, C.c_uint)
BUTTONS = {'b': 0, 'select': 2, 'start': 3, 'up': 4, 'down': 5, 'left': 6, 'right': 7, 'a': 8}


class Player:
    def __init__(self, core, rom, directory):
        self.directory = Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self.system = str(self.directory).encode()
        self.options = {b'fceumm_hdpacks': b'disabled', b'fceumm_overscan_h': b'disabled',
                        b'fceumm_overscan_v': b'disabled'}
        self.pixel_format, self.frame_number, self.audio_samples = 0, 0, 0
        self.audio_peak, self.video, self.buttons = 0, None, set()
        self.checkpoints = []
        self.core = C.CDLL(str(Path(core).resolve()))
        self.callbacks = [ENV(self.environment), VIDEO(self.video_frame), SAMPLE(self.sample),
                          BATCH(self.batch), POLL(lambda: None), INPUT(self.input)]
        for name, callback in zip(('environment', 'video_refresh', 'audio_sample',
                                   'audio_sample_batch', 'input_poll', 'input_state'), self.callbacks):
            fn = getattr(self.core, 'retro_set_'+name)
            fn.argtypes = [type(callback)]
            fn.restype = None
            fn(callback)
        self.core.retro_load_game.argtypes = [C.POINTER(GameInfo)]
        self.core.retro_load_game.restype = C.c_bool
        self.core.retro_get_memory_data.argtypes = [C.c_uint]
        self.core.retro_get_memory_data.restype = C.c_void_p
        self.core.retro_get_memory_size.argtypes = [C.c_uint]
        self.core.retro_get_memory_size.restype = C.c_size_t
        self.core.retro_set_controller_port_device.argtypes = [C.c_uint, C.c_uint]
        self.core.retro_init()
        self.raw = Path(rom).read_bytes()
        self.rom_buffer = C.create_string_buffer(self.raw)
        self.rom_path = str(Path(rom).resolve()).encode()
        info = GameInfo(self.rom_path, C.cast(self.rom_buffer, C.c_void_p), len(self.raw), None)
        if not self.core.retro_load_game(C.byref(info)):
            self.core.retro_deinit()
            raise RuntimeError('FCEUmm could not load this ROM.')
        self.core.retro_set_controller_port_device(0, 1)

    def environment(self, command, pointer):
        if command in (9, 31):
            C.cast(pointer, C.POINTER(C.c_char_p))[0] = self.system
            return True
        if command == 10:
            self.pixel_format = C.cast(pointer, C.POINTER(C.c_int))[0]
            return self.pixel_format in (0, 1, 2)
        if command in (39, 52):
            C.cast(pointer, C.POINTER(C.c_uint))[0] = 0
            return True
        if command == 16:
            variables = C.cast(pointer, C.POINTER(Variable))
            index = 0
            while variables[index].key:
                key, value = variables[index].key, variables[index].value
                self.options.setdefault(key, value.split(b'; ', 1)[-1].split(b'|', 1)[0])
                index += 1
            return True
        if command == 15:
            variable = C.cast(pointer, C.POINTER(Variable)).contents
            variable.value = self.options.get(variable.key)
            return variable.value is not None
        if command == 17:
            C.cast(pointer, C.POINTER(C.c_bool))[0] = False
            return True
        return command in (11, 18, 35, 37)

    def video_frame(self, pointer, width, height, pitch):
        if not pointer:
            return
        raw = C.string_at(pointer, pitch*height)
        if self.pixel_format == 1:
            pixels = np.frombuffer(raw, dtype='<u4').reshape(height, pitch//4)[:, :width]
            shifts, masks = (16, 8, 0), (255, 255, 255)
        else:
            pixels = np.frombuffer(raw, dtype='<u2').reshape(height, pitch//2)[:, :width]
            shifts = (11, 5, 0) if self.pixel_format == 2 else (10, 5, 0)
            masks = (31, 63, 31) if self.pixel_format == 2 else (31, 31, 31)
        self.video = np.stack([((pixels >> shift) & mask)*255//mask for shift, mask in zip(shifts, masks)], axis=-1).astype('uint8')

    def sample(self, left, right):
        self.audio_samples += 1
        self.audio_peak = max(self.audio_peak, abs(left), abs(right))

    def batch(self, data, frames):
        audio = np.ctypeslib.as_array(data, shape=(frames*2,))
        self.audio_samples += frames
        if frames:
            self.audio_peak = max(self.audio_peak, int(np.abs(audio.astype('int32')).max()))
        return frames

    def input(self, port, device, index, button):
        return int(port == 0 and device == 1 and button in self.buttons)

    def run(self, frames, *buttons):
        self.buttons = {BUTTONS[button] for button in buttons}
        for _ in range(frames):
            self.core.retro_run()
            self.frame_number += 1
        self.buttons = set()

    def capture(self, name):
        if self.video is None:
            raise RuntimeError('Emulator produced no video.')
        Image.fromarray(self.video).resize((self.video.shape[1]*3, self.video.shape[0]*3), Image.Resampling.NEAREST).save(self.directory/f'{name}.png')
        size = self.core.retro_get_memory_size(2)
        ram = C.string_at(self.core.retro_get_memory_data(2), size)
        (self.directory/f'{name}.ram.bin').write_bytes(ram)
        result = {'name': name, 'frame': self.frame_number,
                  'video_sha256': hashlib.sha256(self.video.tobytes()).hexdigest(),
                  'colors': len(np.unique(self.video.reshape(-1, 3), axis=0)),
                  'ram_sha256': hashlib.sha256(ram).hexdigest()}
        self.checkpoints.append(result)
        return result

    def report(self):
        result = {'rom_sha256': hashlib.sha256(self.raw).hexdigest(), 'emulator': 'FCEUmm libretro',
                  'frames': self.frame_number, 'audio_samples': self.audio_samples, 'audio_peak': self.audio_peak,
                  'rom_or_memory_patches': False, 'checkpoints': self.checkpoints}
        (self.directory/'playtest.json').write_text(json.dumps(result, indent=2)+'\n')
        return result

    def close(self):
        self.core.retro_unload_game()
        self.core.retro_deinit()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rom', type=Path)
    parser.add_argument('--core', type=Path, default=Path(__file__).resolve().parents[1]/'tmp/toolchains/fceumm/fceumm_libretro.so')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    player = Player(args.core, args.rom, args.output)
    try:
        player.run(300)
        player.capture('boot')
        player.run(900)
        player.capture('idle')
        for button in ('start', 'a', 'b', 'select', 'right', 'down', 'left', 'up'):
            player.run(12, button)
            player.capture(f'held-{button}')
            player.run(120)
            player.capture(f'after-{button}')
        print(json.dumps(player.report(), indent=2))
    finally:
        player.close()


if __name__ == '__main__':
    main()
