# AltCords

Talk in voice chat by typing. Press a hotkey, type a line, and it comes out of
your microphone in a cloned voice, synthesized on your own GPU.

Built for people who cannot or would rather not speak on voice channels, and
who do not want their words going through a cloud service to get there.

Synthesis runs on
[qwen3-tts-burn](https://github.com/teatray-top/qwen3-tts-burn), a Vulkan port
of [Qwen3-TTS](https://github.com/QwenLM/Qwen3-TTS): any GPU with a Vulkan
driver will do, and there is no CUDA toolkit to install.

## Requirements

- Windows and a GPU with a Vulkan driver.
- A virtual audio device. AltCords plays into it and the receiving app
  listens on its capture side; VB-Cable and VoiceMeeter are both found
  automatically, and `output_device` in the config names any other one.
- Qwen3-TTS weights, downloaded on first run if absent (about 4.5 GB).

## Build

The inference engine is a git submodule, so clone with it:

```
git clone --recurse-submodules https://github.com/teatray-top/altcords
cd altcords
cargo build --release
```

An existing clone, or a download that came without submodules, needs
`git submodule update --init --recursive` first. No MSVC or CUDA setup is
involved.

## Use

Press the hotkey, type, press Enter. The line is queued and played as soon as
it is synthesized. A second hotkey stops playback and clears the queue.

Settings hold the voice and the intonation. A voice supplies the timbre and can
be added from any reference clip; an intonation supplies the delivery and needs
the clip plus its transcript.

The interface is available in Korean and English, selectable at the top of the
settings window. The installer's choice applies on first run.

## Model layout

Assets are read from the directory `ALTCORDS_ROOT` names, or from the
executable's own directory:

```
<root>/
  models/base/          # voice-clone (Base) model
  refs/                 # reference-audio clips
```

If the base model is not there, it is downloaded to
`%LOCALAPPDATA%\AltCords\models\base` on first run.

Settings (`config.json`) and the log go to `ALTCORDS_ROOT` if it is set, and
otherwise to `%LOCALAPPDATA%\AltCords`, which stays writable when the program
is installed somewhere read-only. Settings written beside an older build are
read once and carried over.

The log records what the app does, not what you type. Set `ALTCORDS_LOG_TEXT=1`
to include the text as well when diagnosing something.

## Korean text handling

Korean input is reshaped before synthesis, in the app rather than the engine:

- Two-consonant codas are rewritten to how they are actually pronounced,
  including the case where the second consonant carries over to the next
  syllable. Single-consonant codas are left alone; converting those made the
  model worse, not better.
- Syllables outside the KS X 1001 common set are mapped to the nearest
  pronounceable one.
- Syllables the model cannot say are dropped as you type, with a notice. The
  list is editable in settings.

## Releasing

Version lives in `relay/Cargo.toml` (`package.version`) and is mirrored in
`CHANGELOG.md`. For a tagged release:

1. Update `CHANGELOG.md` and bump the crate version if needed.
2. Commit on `master`, then tag annotated: `git tag -a v0.1.0 -m "v0.1.0"`.
3. Build on Windows after submodules are initialized:
   `git submodule update --init --recursive && cargo build --release`
4. Ship the `target/release/altcords.exe` (and any installer you build beside
   it). Model weights are **not** part of the release archive; they download
   on first run under their own license terms.
5. Push the tag (`git push origin v0.1.0`) and create a GitHub Release from it.
   Attach the zip there — do not commit `*.zip` / `*.exe` into the repo
   (see `.gitignore`).

There is no CI release pipeline yet; packaging is manual.


## License
MIT, see [LICENSE](LICENSE). Model weights are licensed separately by their
authors; see the [Qwen3-TTS repository](https://github.com/QwenLM/Qwen3-TTS).

## Credits

- [Qwen3-TTS](https://github.com/QwenLM/Qwen3-TTS) for the model.
- [burn](https://github.com/tracel-ai/burn) for the inference runtime.
- [mlx-audio](https://github.com/Blaizzy/mlx-audio) for a reference
  implementation that clarified model details.
