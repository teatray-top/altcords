# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-03

### Added

- Initial public release of AltCords: type a line, synthesize it locally with
  Qwen3-TTS on Vulkan (`qwen3-tts-burn`), and play PCM into a virtual audio
  cable that other apps read as a microphone.
- Hotkey overlay for compose / stop-and-clear queue, settings UI (voice +
  intonation), Korean and English interface.
- Korean text reshaping before synthesis (compound coda handling, KS X 1001
  mapping, drop list for unsayable syllables).
- First-run model download (~4.5 GB) and optional `--warmup` kernel cache step.
