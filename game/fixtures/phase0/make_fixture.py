#!/usr/bin/env python3
"""Writes the Phase 0 fixture (FORK-PLAN §12, Phase 0).

Outputs, beside this script:
  place/game.rbxlx        R15 place
  place/game-r6.rbxlx     the same place with StarterPlayer.GameSettingsAvatar = R6
  assets/900000001.png     the decal texture
  assets/900000002.wav     the looping sound

The place contains a baseplate, a SpawnLocation, a wall with a Decal that uses
the bundled texture id, a Speaker part with a looping Sound that uses the
bundled sound id, and ReplicatedStorage/Animations/Wave: a KeyframeSequence
with a waving right arm posed for both rigs (R15 UpperTorso -> RightUpperArm,
R6 Torso -> Right Arm). StarterPlayerScripts/FixtureProbe (a LocalScript from
scripts/FixtureProbe.luau) plays it through the current register-then-
LoadAnimation form and checks that the shoulder Motor6D.Transform moves.

Everything is generated: edit this script (or the probe) and re-run it.
"""

import math
import os
import struct
import wave
import zlib
from xml.sax.saxutils import escape

HERE = os.path.dirname(os.path.abspath(__file__))
IDENTITY = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
TEXTURE_ID = 900000001
SOUND_ID = 900000002
WAVE_SECONDS = 1.0


def rgb(r, g, b):
    return (r << 16) | (g << 8) | b


def cframe(x, y, z, m=IDENTITY):
    rows = "".join(
        f"<R{i}{j}>{m[i * 3 + j]:.9g}</R{i}{j}>" for i in range(3) for j in range(3)
    )
    return (
        f'<CoordinateFrame name="CFrame"><X>{x:.9g}</X><Y>{y:.9g}</Y>'
        f"<Z>{z:.9g}</Z>{rows}</CoordinateFrame>"
    )


def look_at(pos, target):
    """Roblox CFrame rotation (columns: Right, Up, -Look)."""
    fx, fy, fz = (target[i] - pos[i] for i in range(3))
    length = math.sqrt(fx * fx + fy * fy + fz * fz)
    fx, fy, fz = fx / length, fy / length, fz / length
    # right = normalize(cross(forward, up)), up = cross(right, forward)
    rx, ry, rz = -fz, 0.0, fx
    rl = math.sqrt(rx * rx + ry * ry + rz * rz)
    rx, ry, rz = rx / rl, ry / rl, rz / rl
    ux = ry * fz - rz * fy
    uy = rz * fx - rx * fz
    uz = rx * fy - ry * fx
    return [rx, ux, -fx, ry, uy, -fy, rz, uz, -fz]


def rot_z(angle):
    c, s = math.cos(angle), math.sin(angle)
    return [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]


def part(ref, name, x, y, z, size, color, children=""):
    return f"""<Item class="Part" referent="{ref}"><Properties>
<string name="Name">{escape(name)}</string>
<bool name="Anchored">true</bool>
{cframe(x, y, z)}
<Vector3 name="size"><X>{size[0]:g}</X><Y>{size[1]:g}</Y><Z>{size[2]:g}</Z></Vector3>
<Color3uint8 name="Color3uint8">{color}</Color3uint8>
<float name="Transparency">0</float>
<token name="Material">256</token>
</Properties>{children}</Item>"""


def pose(ref, name, m=IDENTITY, children=""):
    return f"""<Item class="Pose" referent="{ref}"><Properties>
<string name="Name">{escape(name)}</string>
{cframe(0, 0, 0, m)}
<float name="Weight">1</float>
</Properties>{children}</Item>"""


def keyframe(ref, time, poses):
    return f"""<Item class="Keyframe" referent="{ref}"><Properties>
<string name="Name">Keyframe</string>
<float name="Time">{time:g}</float>
</Properties>{poses}</Item>"""


def animation(referent):
    """KeyframeSequence "Wave": both rigs' shoulder pose trees, two waves a
    second, no asset id (lives in the place)."""
    ref = referent + 1
    keyframes = []
    # (time, right-arm Z rotation): down, up, down, up, down.
    for time, angle in ((0.0, 0.0), (0.25, 1.3), (0.5, 0.1), (0.75, 1.3), (1.0, 0.0)):
        r15_arm = pose(ref, "RightUpperArm", rot_z(angle))
        ref += 1
        r15_torso = pose(ref, "UpperTorso", IDENTITY, r15_arm)
        ref += 1
        r6_arm = pose(ref, "Right Arm", rot_z(angle))
        ref += 1
        r6_torso = pose(ref, "Torso", IDENTITY, r6_arm)
        ref += 1
        keyframes.append(keyframe(ref, time, r15_torso + r6_torso))
        ref += 1
    return ref, f"""<Item class="KeyframeSequence" referent="{referent}"><Properties>
<string name="Name">Wave</string>
<bool name="Loop">true</bool>
<token name="Priority">2</token>
</Properties>{''.join(keyframes)}</Item>"""


def place_xml(avatar, probe_source):
    """avatar: 1 = R15, 0 = R6 (Enum.GameAvatarType)."""
    rig = "R15" if avatar else "R6"
    ref = 0

    def take():
        nonlocal ref
        value = ref
        ref += 1
        return value

    camera = take()
    baseplate = take()
    spawn = take()
    wall = take()
    decal = take()
    speaker = take()
    sound = take()
    workspace = take()
    lighting = take()
    replicated = take()
    animations = take()
    sequence_ref = take()
    seq_end, sequence = animation(sequence_ref)
    ref = max(ref, seq_end)
    sound_service = take()
    starter_player = take()
    starter_scripts = take()
    probe = take()
    players = take()
    server_scripts = take()

    cam_pos = (0.0, 7.0, 24.0)
    cam_rot = look_at(cam_pos, (0.0, 3.0, 0.0))
    pose_refs = sequence  # already carries its own referents from `animation`

    # No XML declaration: rbx_xml detects the format from the leading
    # "<roblox" and rejects a document that starts with "<?xml".
    return f"""<roblox version="4">
<Item class="Workspace" referent="{workspace}"><Properties>
<string name="Name">Workspace</string>
<float name="Gravity">196.2</float>
</Properties>
<Item class="Camera" referent="{camera}"><Properties>
<string name="Name">Camera</string>
{cframe(cam_pos[0], cam_pos[1], cam_pos[2], cam_rot)}
<float name="FieldOfView">70</float>
</Properties></Item>
{part(baseplate, "Baseplate", 0, -0.5, 0, (100, 1, 100), rgb(120, 128, 136))}
<Item class="SpawnLocation" referent="{spawn}"><Properties>
<string name="Name">SpawnLocation</string>
<bool name="Anchored">true</bool>
{cframe(0, 0.5, 0)}
<Vector3 name="size"><X>6</X><Y>1</Y><Z>6</Z></Vector3>
<bool name="Neutral">true</bool>
<bool name="Enabled">true</bool>
<int name="Duration">0</int>
<Color3uint8 name="Color3uint8">{rgb(80, 170, 90)}</Color3uint8>
</Properties></Item>
{part(wall, "TextureWall", 0, 6, -12, (24, 12, 1), rgb(58, 74, 104), f'''<Item class="Decal" referent="{decal}"><Properties>
<string name="Name">GameTexture</string>
<token name="Face">2</token>
<Content name="Texture"><url>rbxassetid://{TEXTURE_ID}</url></Content>
</Properties></Item>''')}
{part(speaker, "Speaker", 8, 2, -6, (2, 2, 2), rgb(230, 140, 60), f'''<Item class="Sound" referent="{sound}"><Properties>
<string name="Name">Theme</string>
<Content name="SoundId"><url>rbxassetid://{SOUND_ID}</url></Content>
<bool name="Looped">true</bool>
<bool name="Playing">true</bool>
<float name="Volume">0.5</float>
<float name="RollOffMaxDistance">120</float>
</Properties></Item>''')}
</Item>
<Item class="Lighting" referent="{lighting}"><Properties>
<string name="Name">Lighting</string>
<float name="ClockTime">14</float>
<float name="Brightness">2</float>
<bool name="GlobalShadows">true</bool>
</Properties></Item>
<Item class="ReplicatedStorage" referent="{replicated}"><Properties>
<string name="Name">ReplicatedStorage</string>
</Properties>
<Item class="Folder" referent="{animations}"><Properties>
<string name="Name">Animations</string>
</Properties>
{pose_refs}
</Item></Item>
<Item class="SoundService" referent="{sound_service}"><Properties>
<string name="Name">SoundService</string>
</Properties></Item>
<Item class="StarterPlayer" referent="{starter_player}"><Properties>
<string name="Name">StarterPlayer</string>
<token name="GameSettingsAvatar">{avatar}</token>
<token name="AvatarJointUpgrade">1</token>
</Properties>
<Item class="StarterPlayerScripts" referent="{starter_scripts}"><Properties>
<string name="Name">StarterPlayerScripts</string>
</Properties>
<Item class="LocalScript" referent="{probe}"><Properties>
<string name="Name">FixtureProbe</string>
<ProtectedString name="Source">{escape(probe_source)}</ProtectedString>
</Properties></Item>
</Item></Item>
<Item class="Players" referent="{players}"><Properties>
<string name="Name">Players</string>
</Properties></Item>
<Item class="ServerScriptService" referent="{server_scripts}"><Properties>
<string name="Name">ServerScriptService</string>
</Properties></Item>
</roblox>
"""


def write_png(path, size):
    """A checkerboard with a bright disc: unmistakable in a screenshot."""
    rows = bytearray()
    for y in range(size):
        rows.append(0)  # filter: None
        for x in range(size):
            checker = ((x // 32) + (y // 32)) % 2
            dx, dy = x - size / 2, y - size / 2
            disc = dx * dx + dy * dy < (size * 0.28) ** 2
            stripe = abs((x - y) % 64 - 32) < 6
            if disc:
                pixel = (255, 140, 40)
            elif stripe:
                pixel = (240, 240, 240)
            elif checker:
                pixel = (40, 90, 170)
            else:
                pixel = (20, 50, 110)
            rows.extend(pixel)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as file:
        file.write(png)


def write_wav(path):
    """Two seconds of a soft arpeggio (A4, C#5, E5, A5) that loops cleanly."""
    rate = 22050
    notes = (440.0, 554.37, 659.25, 880.0)
    amplitude = 0.28
    frames = bytearray()
    for i in range(int(rate * len(notes) * 0.5)):
        t = i / rate
        note = int(t / 0.5) % len(notes)
        local = t % 0.5
        freq = notes[note]
        # 12 ms attack, 60 ms release, no click at the loop point.
        env = min(1.0, local / 0.012, (0.5 - local) / 0.06)
        sample = math.sin(2 * math.pi * freq * t)
        sample += 0.25 * math.sin(2 * math.pi * freq * 2 * t)
        value = int(max(-1.0, min(1.0, sample * amplitude * env)) * 32767)
        frames.extend(struct.pack("<h", value))
    with wave.open(path, "wb") as file:
        file.setnchannels(1)
        file.setsampwidth(2)
        file.setframerate(rate)
        file.writeframes(bytes(frames))


def main():
    with open(os.path.join(HERE, "scripts/FixtureProbe.luau"), encoding="utf-8") as file:
        probe = file.read()
    places = os.path.join(HERE, "place")
    assets = os.path.join(HERE, "assets")
    os.makedirs(places, exist_ok=True)
    os.makedirs(assets, exist_ok=True)

    for name, avatar in (("game.rbxlx", 1), ("game-r6.rbxlx", 0)):
        with open(os.path.join(places, name), "w", encoding="utf-8") as file:
            file.write(place_xml(avatar, probe))
    write_png(os.path.join(assets, f"{TEXTURE_ID}.png"), 256)
    write_wav(os.path.join(assets, f"{SOUND_ID}.wav"))
    print("wrote place/game.rbxlx, place/game-r6.rbxlx, and the two assets")


if __name__ == "__main__":
    main()
